use crate::{ChangedFile, Commit, PatchSide, PatchVersion, Revisions, Worktree, WorktreePatch};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    iter::once,
    path::{Path, PathBuf},
    process::Command,
};

const PATCH_OPTIONS: &[&str] = &[
    "--color=never",
    "--no-ext-diff",
    "--no-textconv",
    "--submodule=short",
    "--ignore-submodules=none",
    "--unified=3",
];

pub(crate) fn git(directory: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = Command::new("git");
    command
        .current_dir(directory)
        .env_remove("GIT_DIR")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|error| format!("Cannot run Git in {}: {error}", directory.display()))?;
    if output.status.success()
        || (args.starts_with(&["diff", "--no-index"]) && output.status.code() == Some(1))
    {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if stderr.trim().is_empty() {
            output.status.to_string()
        } else {
            stderr.trim().into()
        };
        Err(format!("Git {}: {detail}", args[0]))
    }
}

fn resolve_path(directory: &Path, option: &str) -> Result<PathBuf, String> {
    let output = git(directory, &["rev-parse", "--path-format=absolute", option])?;
    let path = std::str::from_utf8(&output).map_err(|_| "Git checkout paths must be UTF-8")?;
    let path = Path::new(path.strip_suffix('\n').unwrap_or(path));
    path.canonicalize()
        .map_err(|error| format!("Cannot open {}: {error}", path.display()))
}

pub(crate) fn checkout_root(directory: &Path) -> Result<PathBuf, String> {
    resolve_path(directory, "--show-toplevel")
}

pub(crate) fn common_directory(checkout: &Path) -> Result<PathBuf, String> {
    resolve_path(checkout, "--git-common-dir")
}

pub(crate) fn worktree_directory(checkout: &Path) -> Result<PathBuf, String> {
    resolve_path(checkout, "--absolute-git-dir")
}

pub(crate) fn worktrees(
    common_directory: &Path,
    main_hint: &Path,
) -> Result<(PathBuf, Vec<Worktree>), String> {
    let output = git(
        common_directory,
        &["--git-dir=.", "worktree", "list", "--porcelain", "-z"],
    )?;
    let output = std::str::from_utf8(&output).map_err(|_| "Git checkout paths must be UTF-8")?;
    let mut main_checkout = None;
    let mut worktrees = Vec::new();
    for record in output.split("\0\0").filter(|record| !record.is_empty()) {
        let mut fields = record.split('\0');
        let Some(path) = fields
            .next()
            .and_then(|field| field.strip_prefix("worktree "))
        else {
            return Err("Unexpected output from Git worktree list".into());
        };
        let path = PathBuf::from(path);
        let mut path = path.canonicalize().unwrap_or(path);

        let mut branch = None;
        let mut available = true;
        for field in fields {
            if let Some(name) = field.strip_prefix("branch refs/heads/") {
                branch = Some(name.into());
            }
            if field == "bare" || field == "prunable" || field.starts_with("prunable ") {
                available = false;
            }
        }
        if main_checkout.is_none() && path == common_directory && available {
            // With separate Git metadata, Git lists the metadata directory as the main worktree.
            if main_hint != common_directory
                && resolve_path(main_hint, "--absolute-git-dir")
                    .is_ok_and(|directory| directory == common_directory)
            {
                path = main_hint.to_path_buf();
            } else {
                available = false;
            }
        }
        main_checkout.get_or_insert_with(|| path.clone());
        if available {
            worktrees.push(Worktree { path, branch });
        }
    }
    let main_checkout = main_checkout.ok_or("Git returned no worktrees for the project")?;
    Ok((main_checkout, worktrees))
}

pub(crate) fn revisions(checkout: &Path) -> Result<Revisions, String> {
    let branches = git(
        checkout,
        &[
            "for-each-ref",
            "--format=%(refname:lstrip=2)",
            "refs/heads/",
        ],
    )?;
    let branches = String::from_utf8_lossy(&branches)
        .lines()
        .map(str::to_owned)
        .collect();
    let mut args = vec![
        "log",
        "--branches",
        "--max-count=100",
        "--no-show-signature",
        "--no-notes",
        "--no-decorate",
        "--color=never",
        "-z",
        "--format=%H%x00%s",
    ];
    if resolve_commit(checkout, "HEAD").is_ok() {
        args.push("HEAD");
    }
    args.push("--");
    let output = git(checkout, &args)?;
    let output = String::from_utf8_lossy(&output);
    let mut fields = output.split_terminator('\0');
    let mut commits = Vec::new();
    while let Some(id) = fields.next() {
        let subject = fields.next().ok_or("Unexpected output from Git log")?;
        commits.push(Commit {
            id: id.into(),
            subject: subject.into(),
        });
    }
    Ok(Revisions { branches, commits })
}

pub(crate) fn resolve_commit(checkout: &Path, revision: &str) -> Result<String, String> {
    let output = git(
        checkout,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
    )
    .map_err(|error| format!("Cannot resolve revision {revision:?}: {error}"))?;
    Ok(String::from_utf8_lossy(&output).trim_end().into())
}

pub(crate) fn merge_base(checkout: &Path, base: &str, target: &str) -> Result<String, String> {
    let output = git(checkout, &["merge-base", base, target])
        .map_err(|error| format!("Cannot find a common ancestor: {error}"))?;
    Ok(String::from_utf8_lossy(&output).trim_end().into())
}

pub(crate) fn compare_commits(
    checkout: &Path,
    start: &str,
    end: &str,
) -> Result<Vec<ChangedFile>, String> {
    let mut args = vec!["diff", "--patch-with-raw", "-z", "--find-renames"];
    args.extend(PATCH_OPTIONS);
    args.extend([start, end, "--"]);
    let output = git(checkout, &args)?;
    Ok(parse_patch(&output)?
        .into_iter()
        .map(RawPatch::file)
        .collect())
}

pub(crate) fn uncommitted_patches(checkout: &Path) -> Result<Vec<WorktreePatch>, String> {
    let unmerged = git(checkout, &["ls-files", "--unmerged", "-z"])?;
    let mut conflicts = BTreeMap::<&[u8], Vec<&[u8]>>::new();
    for record in unmerged
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("Unexpected output from Git ls-files --unmerged")?;
        conflicts
            .entry(&record[tab + 1..])
            .or_default()
            .push(&record[..tab]);
    }

    let mut patches = Vec::new();
    for (side, option) in [
        (PatchSide::Staged, "--cached"),
        (PatchSide::Unstaged, "--ours"),
    ] {
        let mut args = vec![
            "diff",
            option,
            "--diff-filter=u",
            "--find-renames",
            "--ita-invisible-in-index",
        ];
        args.extend(["--patch-with-raw", "-z", "--full-index", "--abbrev=64"]);
        args.extend(PATCH_OPTIONS);
        args.push("--");
        let output = git(checkout, &args)?;
        for raw in parse_patch(&output)? {
            if !conflicts.contains_key(raw.path) {
                patches.push(raw.worktree_patch(side, None)?);
            }
        }
    }

    let untracked = git(
        checkout,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    for path in untracked
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        if path.ends_with(b"/") {
            // An untracked nested repository is reported as a directory by ls-files.
            continue;
        }
        let path = worktree_path(path)?;
        let operand = if path == "-" { "./-" } else { path };
        let mut args = vec![
            "diff",
            "--no-index",
            "--patch-with-raw",
            "-z",
            "--full-index",
            "--abbrev=64",
        ];
        args.extend(PATCH_OPTIONS);
        args.extend(["--", "/dev/null", operand]);
        let output = git(checkout, &args)?;
        for raw in parse_patch(&output)? {
            patches.push(raw.worktree_patch(PatchSide::Unstaged, Some(path))?);
        }
    }

    for (path, stages) in conflicts {
        let path = worktree_path(path)?;
        let stage = if stages.iter().any(|stage| stage.ends_with(b" 2")) {
            "--ours"
        } else if stages.iter().any(|stage| stage.ends_with(b" 3")) {
            "--theirs"
        } else {
            "--base"
        };
        let mut args = vec![
            "--literal-pathspecs",
            "diff",
            stage,
            "--diff-filter=u",
            "--patch",
            "--full-index",
        ];
        args.extend(PATCH_OPTIONS);
        args.extend(["--", path]);
        let output = git(checkout, &args)?;
        let mut patch = WorktreePatch {
            file: ChangedFile {
                status: 'U',
                path: path.into(),
                previous_path: None,
                patch: if output.is_empty() {
                    "Unmerged path".into()
                } else {
                    String::from_utf8_lossy(&output).into()
                },
            },
            side: PatchSide::Unstaged,
            fingerprint: String::new(),
            before: None,
            after: None,
            reviewed: false,
        };
        patch.fingerprint = fingerprint(&patch, &stages, &output);
        patches.push(patch);
    }
    Ok(patches)
}

fn worktree_path(path: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(path).map_err(|_| "Git worktree paths must be UTF-8".into())
}

fn fingerprint(patch: &WorktreePatch, extra: &[&[u8]], diff: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update([patch.side as u8, patch.file.status as u8]);
    for field in [
        Some(patch.file.path.as_str()),
        patch.file.previous_path.as_deref(),
    ] {
        hash.update(field.unwrap_or_default());
        hash.update([0]);
    }
    for version in [&patch.before, &patch.after] {
        if let Some(version) = version {
            hash.update(&version.mode);
            hash.update(&version.object_id);
        }
        hash.update([0]);
    }
    for field in extra {
        hash.update(field);
        hash.update([0]);
    }
    hash.update(diff);
    format!("{:x}", hash.finalize())
}

struct RawPatch<'a> {
    header: &'a str,
    status: char,
    path: &'a [u8],
    previous_path: Option<&'a [u8]>,
    patch: &'a [u8],
}

impl RawPatch<'_> {
    fn file(self) -> ChangedFile {
        ChangedFile {
            status: self.status,
            path: String::from_utf8_lossy(self.path).into(),
            previous_path: self
                .previous_path
                .map(|path| String::from_utf8_lossy(path).into()),
            patch: String::from_utf8_lossy(self.patch).into(),
        }
    }

    fn worktree_patch(self, side: PatchSide, path: Option<&str>) -> Result<WorktreePatch, String> {
        let mut fields = self.header.trim_start_matches(':').split_whitespace();
        let [old_mode, new_mode, old_id, new_id] =
            std::array::from_fn(|_| fields.next().unwrap_or_default());
        let before = version(old_mode, old_id);
        let mut after_id = new_id;
        if new_mode != "000000" && new_id.bytes().all(|byte| byte == b'0') {
            // The raw record has no object ID for the working tree. A mode-only patch
            // omits the index line because its content stayed the same.
            after_id = self
                .patch
                .split(|byte| *byte == b'\n')
                .find_map(|line| {
                    let index = line.strip_prefix(b"index ")?;
                    let index = std::str::from_utf8(index).ok()?.split_whitespace().next()?;
                    let (_, id) = index.split_once("..")?;
                    id.bytes().any(|byte| byte != b'0').then_some(id)
                })
                .unwrap_or(old_id);
        }
        let after = version(new_mode, after_id);
        let diff = self.patch;
        worktree_path(self.path)?;
        self.previous_path.map(worktree_path).transpose()?;
        let mut file = self.file();
        if let Some(path) = path {
            file.path = path.into();
        }
        let mut patch = WorktreePatch {
            file,
            side,
            fingerprint: String::new(),
            before,
            after,
            reviewed: false,
        };
        patch.fingerprint = fingerprint(&patch, &[], diff);
        Ok(patch)
    }
}

fn version(mode: &str, object_id: &str) -> Option<PatchVersion> {
    (mode != "000000").then(|| PatchVersion {
        mode: mode.into(),
        object_id: object_id.into(),
    })
}

fn parse_patch(output: &[u8]) -> Result<Vec<RawPatch<'_>>, String> {
    parse_raw_patch(output).ok_or_else(|| "Unexpected output from Git diff".into())
}

fn parse_raw_patch(output: &[u8]) -> Option<Vec<RawPatch<'_>>> {
    if output.is_empty() {
        return Some(Vec::new());
    }
    // Git emits NUL-delimited raw records followed by the unified patches in the same order.
    let separator = output.windows(2).position(|bytes| bytes == b"\0\0")?;
    let records = &output[..separator];
    let patch = &output[separator + 2..];
    if !patch.starts_with(b"diff --git ") {
        return None;
    }
    let mut offsets = once(0)
        .chain(
            patch
                .windows(b"\ndiff --git ".len())
                .enumerate()
                .filter(|(_, bytes)| *bytes == b"\ndiff --git ")
                .map(|(offset, _)| offset + 1),
        )
        .chain(once(patch.len()))
        .peekable();
    let mut records = records.split(|byte| *byte == 0);
    let mut files = Vec::new();
    while let Some(header) = records.next() {
        let header = std::str::from_utf8(header).ok()?;
        let status = header.split_whitespace().last()?.chars().next()?;
        let first_path = records.next()?;
        let (previous_path, path) = if matches!(status, 'R' | 'C') {
            (Some(first_path), records.next()?)
        } else {
            (None, first_path)
        };
        let start = offsets.next()?;
        if status == 'T' {
            // Git renders a type change as deletion and addition patches for one record.
            offsets.next()?;
        }
        let end = *offsets.peek()?;
        files.push(RawPatch {
            header,
            status,
            path,
            previous_path,
            patch: &patch[start..end],
        });
    }
    (offsets.next()? == patch.len()).then_some(files)
}
