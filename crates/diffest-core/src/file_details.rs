use crate::{Application, PatchSide, git};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};

// Keep the image payload sent through Tauri IPC bounded.
const PREVIEW_LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FileDetailsSource {
    Comparison {
        start: String,
        end: String,
        path: String,
        previous_path: Option<String>,
        status: char,
    },
    Worktree {
        fingerprint: String,
    },
}

#[derive(Serialize)]
pub struct FileDetails {
    pub before: Option<FileVersionDetails>,
    pub after: Option<FileVersionDetails>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileVersionDetails {
    pub path: String,
    pub byte_size: Option<u64>,
    pub format: Option<&'static str>,
    pub preview: Option<String>,
}

impl Application {
    pub fn file_details(
        &self,
        checkout: &Path,
        source: FileDetailsSource,
    ) -> Result<FileDetails, String> {
        let checkout = git::checkout_root(checkout)?;
        match source {
            FileDetailsSource::Comparison {
                start,
                end,
                path,
                previous_path,
                status,
            } => {
                let before_path = previous_path.as_deref().unwrap_or(&path);
                Ok(FileDetails {
                    before: (status != 'A').then(|| tree_version(&checkout, &start, before_path)),
                    after: (status != 'D').then(|| tree_version(&checkout, &end, &path)),
                })
            }
            FileDetailsSource::Worktree { fingerprint } => {
                let patch = git::uncommitted_patches(&checkout)?
                    .into_iter()
                    .find(|patch| patch.fingerprint == fingerprint)
                    .ok_or(
                        "The patch changed. Refresh uncommitted changes to inspect its details.",
                    )?;
                let path = &patch.file.path;
                if patch.file.status == 'U' {
                    return Ok(FileDetails {
                        before: conflict_base(&checkout, path),
                        after: working_version(&checkout, path),
                    });
                }
                let before_path = patch.file.previous_path.as_deref().unwrap_or(path);
                Ok(FileDetails {
                    before: patch.before.map(|version| {
                        object_version(&checkout, before_path, &version.mode, &version.object_id)
                    }),
                    after: patch.after.map(|version| match patch.side {
                        PatchSide::Staged => {
                            object_version(&checkout, path, &version.mode, &version.object_id)
                        }
                        PatchSide::Unstaged => working_version(&checkout, path)
                            .unwrap_or_else(|| version_details(path, &version.mode, None, None)),
                    }),
                })
            }
        }
    }
}

fn tree_version(checkout: &Path, revision: &str, path: &str) -> FileVersionDetails {
    let entry = git::resolve_commit(checkout, revision).and_then(|commit| {
        git::git(
            checkout,
            &["--literal-pathspecs", "ls-tree", "-z", &commit, "--", path],
        )
    });
    if let Ok(entry) = entry
        && let Some((header, name)) = std::str::from_utf8(&entry)
            .ok()
            .and_then(|entry| entry.split_once('\t'))
        && name.strip_suffix('\0') == Some(path)
    {
        let mut fields = header.split_whitespace();
        if let (Some(mode), Some(_), Some(object_id)) =
            (fields.next(), fields.next(), fields.next())
        {
            return object_version(checkout, path, mode, object_id);
        }
    }
    version_details(path, "", None, None)
}

fn conflict_base(checkout: &Path, path: &str) -> Option<FileVersionDetails> {
    let output = git::git(
        checkout,
        &[
            "--literal-pathspecs",
            "ls-files",
            "--unmerged",
            "-z",
            "--",
            path,
        ],
    )
    .ok()?;
    let output = std::str::from_utf8(&output).ok()?;
    [2, 3, 1].into_iter().find_map(|wanted| {
        output.split_terminator('\0').find_map(|record| {
            let (header, _) = record.split_once('\t')?;
            let mut fields = header.split_whitespace();
            let (mode, object_id, stage) = (fields.next()?, fields.next()?, fields.next()?);
            (stage.parse::<u8>().ok()? == wanted)
                .then(|| object_version(checkout, path, mode, object_id))
        })
    })
}

fn object_version(checkout: &Path, path: &str, mode: &str, object_id: &str) -> FileVersionDetails {
    if mode == "160000" {
        return version_details(path, mode, None, None);
    }
    let size = git::git(checkout, &["cat-file", "-s", object_id])
        .ok()
        .and_then(|size| String::from_utf8_lossy(&size).trim().parse::<u64>().ok());
    let bytes = size
        .filter(|&size| size <= PREVIEW_LIMIT && mode != "120000")
        .and_then(|_| git::git(checkout, &["cat-file", "blob", object_id]).ok());
    version_details(path, mode, size, bytes.as_deref())
}

fn working_version(checkout: &Path, path: &str) -> Option<FileVersionDetails> {
    let file = checkout.join(path);
    let parent = file.parent()?.canonicalize().ok()?;
    if !parent.starts_with(checkout) {
        return None;
    }
    let file = parent.join(file.file_name()?);
    let metadata = file.symlink_metadata().ok()?;
    if metadata.is_symlink() {
        let size = fs::read_link(file)
            .ok()
            .map(|target| target.as_os_str().len() as u64);
        return Some(version_details(path, "120000", size, None));
    }
    if metadata.is_dir() {
        return Some(version_details(path, "160000", None, None));
    }
    let size = metadata.is_file().then(|| metadata.len());
    let bytes = size.filter(|&size| size <= PREVIEW_LIMIT).and_then(|size| {
        let mut bytes = Vec::new();
        fs::File::open(file)
            .ok()?
            .take(size + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        (bytes.len() as u64 == size).then_some(bytes)
    });
    Some(version_details(path, "", size, bytes.as_deref()))
}

fn version_details(
    path: &str,
    mode: &str,
    byte_size: Option<u64>,
    bytes: Option<&[u8]>,
) -> FileVersionDetails {
    let mut preview = None;
    let data = bytes.unwrap_or_default();
    let format = match (mode, data) {
        ("120000", _) => Some("Symbolic link"),
        ("160000", _) => Some("Submodule"),
        _ => {
            if let Some((format, mime)) = image_format(data) {
                preview = Some(format!("data:{mime};base64,{}", STANDARD.encode(data)));
                Some(format)
            } else if data.starts_with(b"%PDF-") {
                Some("PDF")
            } else if data.starts_with(b"PK\x03\x04") {
                Some("ZIP")
            } else {
                match Path::new(path)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref()
                {
                    Some("jpg" | "jpeg") => Some("JPEG"),
                    Some("png") => Some("PNG"),
                    Some("gif") => Some("GIF"),
                    Some("webp") => Some("WebP"),
                    _ if bytes.is_some()
                        && !data.contains(&0)
                        && std::str::from_utf8(data).is_ok() =>
                    {
                        Some("Text")
                    }
                    _ => None,
                }
            }
        }
    };
    FileVersionDetails {
        path: path.into(),
        byte_size,
        format,
        preview,
    }
}

fn image_format(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    match bytes {
        [0xff, 0xd8, 0xff, ..] => Some(("JPEG", "image/jpeg")),
        [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, ..] => Some(("PNG", "image/png")),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some(("GIF", "image/gif")),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some(("WebP", "image/webp")),
        _ => None,
    }
}
