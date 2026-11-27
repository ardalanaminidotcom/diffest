use crate::git::{GITLINK_MODE, SYMLINK_MODE};
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
        path_bytes: Option<Vec<u8>>,
        previous_path_bytes: Option<Vec<u8>>,
        status: char,
    },
    Worktree {
        fingerprint: String,
        path: String,
        previous_path: Option<String>,
        path_bytes: Option<Vec<u8>>,
        previous_path_bytes: Option<Vec<u8>>,
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
                path_bytes,
                previous_path_bytes,
                status,
            } => {
                let path = path_bytes.as_deref().unwrap_or(path.as_bytes());
                let before_path = previous_path_bytes
                    .as_deref()
                    .or_else(|| previous_path.as_deref().map(str::as_bytes))
                    .unwrap_or(path);
                let review_objects = self.review_objects(&checkout)?;
                let objects = review_objects.directory.is_dir().then_some(&review_objects);
                Ok(FileDetails {
                    before: (status != 'A')
                        .then(|| tree_version(&checkout, &start, before_path, None)),
                    after: (status != 'D').then(|| tree_version(&checkout, &end, path, objects)),
                })
            }
            FileDetailsSource::Worktree {
                fingerprint,
                path,
                previous_path,
                path_bytes,
                previous_path_bytes,
            } => {
                let path = path_bytes.as_deref().unwrap_or(path.as_bytes());
                let previous = previous_path_bytes
                    .as_deref()
                    .or_else(|| previous_path.as_deref().map(str::as_bytes));
                let paths = std::iter::once(path)
                    .chain(previous)
                    .map(git::os_path)
                    .collect::<Result<Vec<_>, _>>()?;
                let patch = git::uncommitted_patches_for(&checkout, &paths)?
                    .into_iter()
                    .find(|patch| patch.fingerprint == fingerprint)
                    .ok_or(
                        "The patch changed. Refresh uncommitted changes to inspect its details.",
                    )?;
                let path = patch.file.path_bytes();
                let before_path = patch.file.previous_path_bytes().unwrap_or(path);
                let before = patch.before.map(|version| {
                    object_version(
                        &checkout,
                        before_path,
                        &version.mode,
                        &version.object_id,
                        None,
                    )
                });
                if patch.file.status == 'U' {
                    return Ok(FileDetails {
                        before,
                        after: working_version(&checkout, path),
                    });
                }
                Ok(FileDetails {
                    before,
                    after: patch.after.map(|version| match patch.side {
                        PatchSide::Staged => {
                            object_version(&checkout, path, &version.mode, &version.object_id, None)
                        }
                        PatchSide::Unstaged => working_version(&checkout, path)
                            .unwrap_or_else(|| version_details(path, &version.mode, None, None)),
                    }),
                })
            }
        }
    }
}

fn tree_version(
    checkout: &Path,
    revision: &str,
    path: &[u8],
    objects: Option<&git::ReviewObjects>,
) -> FileVersionDetails {
    let entry = git::resolve_tree(checkout, revision, objects).and_then(|tree| {
        git::git_with_objects(
            checkout,
            &["--literal-pathspecs", "ls-tree", "-z", &tree, "--"],
            &[git::os_path(path)?],
            objects,
        )
    });
    if let Ok(entry) = entry
        && let Some(tab) = entry.iter().position(|&byte| byte == b'\t')
        && entry[tab + 1..].strip_suffix(b"\0") == Some(path)
        && let Ok(header) = std::str::from_utf8(&entry[..tab])
    {
        let mut fields = header.split_whitespace();
        if let (Some(mode), Some(_), Some(object_id)) =
            (fields.next(), fields.next(), fields.next())
        {
            return object_version(checkout, path, mode, object_id, objects);
        }
    }
    version_details(path, "", None, None)
}

fn object_version(
    checkout: &Path,
    path: &[u8],
    mode: &str,
    object_id: &str,
    objects: Option<&git::ReviewObjects>,
) -> FileVersionDetails {
    if mode == GITLINK_MODE {
        return version_details(path, mode, None, None);
    }
    let size = git::git_with_objects(checkout, &["cat-file", "-s", object_id], &[], objects)
        .ok()
        .and_then(|size| String::from_utf8_lossy(&size).trim().parse::<u64>().ok());
    let bytes = size
        .filter(|&size| size <= PREVIEW_LIMIT && mode != SYMLINK_MODE)
        .and_then(|_| {
            git::git_with_objects(checkout, &["cat-file", "blob", object_id], &[], objects).ok()
        });
    version_details(path, mode, size, bytes.as_deref())
}

fn working_version(checkout: &Path, path: &[u8]) -> Option<FileVersionDetails> {
    let file = checkout.join(git::os_path(path).ok()?);
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
        return Some(version_details(path, SYMLINK_MODE, size, None));
    }
    if metadata.is_dir() {
        return Some(version_details(path, GITLINK_MODE, None, None));
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
    path: &[u8],
    mode: &str,
    byte_size: Option<u64>,
    bytes: Option<&[u8]>,
) -> FileVersionDetails {
    let path = git::display_path(path);
    let mut preview = None;
    let data = bytes.unwrap_or_default();
    let format = match (mode, data) {
        (SYMLINK_MODE, _) => Some("Symbolic link"),
        (GITLINK_MODE, _) => Some("Submodule"),
        _ => {
            if let Some((format, mime)) = image_format(data) {
                preview = Some(format!("data:{mime};base64,{}", STANDARD.encode(data)));
                Some(format)
            } else if data.starts_with(b"%PDF-") {
                Some("PDF")
            } else if data.starts_with(b"PK\x03\x04") {
                Some("ZIP")
            } else {
                match Path::new(&path)
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
        path,
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
