use crate::{ChangedFile, Commit, Revisions, Worktree};
use std::{
    iter::once,
    path::{Path, PathBuf},
    process::Command,
};

fn git(directory: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = Command::new("git");
    command
        .current_dir(directory)
        .env_remove("GIT_DIR")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_WORK_TREE")
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
    if output.status.success() {
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
    let output = git(
        checkout,
        &[
            "diff",
            "--patch-with-raw",
            "-z",
            "--find-renames",
            "--color=never",
            "--no-ext-diff",
            "--no-textconv",
            "--submodule=short",
            "--ignore-submodules=none",
            "--unified=3",
            start,
            end,
            "--",
        ],
    )?;
    let output = String::from_utf8_lossy(&output);
    parse_patch(&output).ok_or_else(|| "Unexpected output from Git diff".into())
}

fn parse_patch(output: &str) -> Option<Vec<ChangedFile>> {
    if output.is_empty() {
        return Some(Vec::new());
    }
    // Git emits NUL-delimited raw records followed by the unified patches in the same order.
    let (records, patch) = output.split_once("\0\0")?;
    if !patch.starts_with("diff --git ") {
        return None;
    }
    let mut offsets = once(0)
        .chain(
            patch
                .match_indices("\ndiff --git ")
                .map(|(offset, _)| offset + 1),
        )
        .chain(once(patch.len()))
        .peekable();
    let mut records = records.split('\0');
    let mut files = Vec::new();
    while let Some(header) = records.next() {
        let status = header.split_whitespace().last()?.chars().next()?;
        let first_path = records.next()?;
        let (previous_path, path) = if matches!(status, 'R' | 'C') {
            (Some(first_path.into()), records.next()?)
        } else {
            (None, first_path)
        };
        let start = offsets.next()?;
        if status == 'T' {
            // Git renders a type change as deletion and addition patches for one record.
            offsets.next()?;
        }
        let end = *offsets.peek()?;
        files.push(ChangedFile {
            status,
            path: path.into(),
            previous_path,
            patch: patch[start..end].into(),
        });
    }
    (offsets.next()? == patch.len()).then_some(files)
}
