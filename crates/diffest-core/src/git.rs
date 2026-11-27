use crate::{ChangedFile, Commit, PatchSide, PatchVersion, Revisions, Worktree, WorktreePatch};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fmt::Write as _,
    fs::{self, OpenOptions},
    io::{self, Read},
    iter::once,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
};

const GIT_OUTPUT_LIMIT: usize = 16 * 1024 * 1024;
const DIFF_LIMIT: usize = 8 * 1024 * 1024;
const WORKTREE_FILE_DIFF_LIMIT: usize = 512;
const PATCH_LIMIT: usize = 1024 * 1024;
const DIFF_SIZE_ERROR: &str = "The diff is too large to display. Compare a smaller set of changes.";
const PATCH_SIZE_ERROR: &str = "A file patch exceeds the 1 MiB review limit. Compare a smaller change.";

const MISSING_MODE: &str = "000000";
const REGULAR_MODE: &str = "100644";
const EXECUTABLE_MODE: &str = "100755";
pub(crate) const SYMLINK_MODE: &str = "120000";
pub(crate) const GITLINK_MODE: &str = "160000";

const PATCH_OPTIONS: &[&str] = &[
    "--color=never",
    "--no-ext-diff",
    "--no-textconv",
    "--submodule=short",
    "--ignore-submodules=none",
    "--unified=3",
];

pub(crate) fn git(directory: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    git_with_paths(directory, args, &[])
}

pub(crate) fn git_with_paths(
    directory: &Path,
    args: &[&str],
    paths: &[&OsStr],
) -> Result<Vec<u8>, String> {
    git_with_index(directory, args, paths, None)
}

fn git_with_index(
    directory: &Path,
    args: &[&str],
    paths: &[&OsStr],
    index: Option<&Path>,
) -> Result<Vec<u8>, String> {
    git_with_storage(directory, args, paths, index, None)
}

pub(crate) struct ReviewObjects {
    pub directory: PathBuf,
    pub alternate: String,
}

pub(crate) fn git_with_objects(
    checkout: &Path,
    args: &[&str],
    paths: &[&OsStr],
    objects: Option<&ReviewObjects>,
) -> Result<Vec<u8>, String> {
    git_with_storage(checkout, args, paths, None, objects)
}

fn git_with_storage(
    directory: &Path,
    args: &[&str],
    paths: &[&OsStr],
    index: Option<&Path>,
    objects: Option<&ReviewObjects>,
) -> Result<Vec<u8>, String> {
    let mut command = Command::new("git");
    command
        .current_dir(directory)
        .env_remove("GIT_DIR")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_OPTIONAL_LOCKS", "0")
        // Keep index operations from using a checkout-configured fsmonitor hook.
        .args(["-c", "core.fsmonitor=false"])
        .args(args)
        .args(paths);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    if let Some(objects) = objects {
        command
            .env("GIT_OBJECT_DIRECTORY", &objects.directory)
            .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", &objects.alternate);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let is_diff = args.starts_with(&["diff"]) || args.starts_with(&["--literal-pathspecs", "diff"]);
    let limit = if is_diff {
        DIFF_LIMIT
    } else {
        GIT_OUTPUT_LIMIT
    };
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Cannot run Git in {}: {error}", directory.display()))?;
    let stdout = child.stdout.take().expect("Git stdout is piped");
    let stderr = child.stderr.take().expect("Git stderr is piped");
    let (status, stdout, stderr) = thread::scope(|scope| -> Result<_, String> {
        let diagnostics = scope.spawn(move || -> io::Result<Vec<u8>> {
            let mut stderr = stderr;
            let mut message = Vec::new();
            stderr.by_ref().take(8192).read_to_end(&mut message)?;
            io::copy(&mut stderr, &mut io::sink())?;
            Ok(message)
        });
        let mut output = Vec::new();
        let result = stdout.take(limit as u64 + 1).read_to_end(&mut output);
        if result.is_err() || output.len() > limit {
            let _ = child.kill();
        }
        let status = child
            .wait()
            .map_err(|error| format!("Cannot wait for Git: {error}"))?;
        if output.len() > limit {
            let error = if is_diff {
                DIFF_SIZE_ERROR
            } else {
                "Git output exceeds the 16 MiB limit."
            };
            return Err(error.into());
        }
        result.map_err(|error| format!("Cannot read Git output: {error}"))?;
        let message = diagnostics
            .join()
            .expect("Git stderr reader")
            .map_err(|error| format!("Cannot read Git diagnostics: {error}"))?;
        Ok((status, output, message))
    })?;
    if status.success()
        || (args.starts_with(&["diff", "--no-index"]) && status.code() == Some(1))
    {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&stderr);
        let detail = if stderr.trim().is_empty() {
            status.to_string()
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
) -> Result<(Option<PathBuf>, Vec<Worktree>), String> {
    let output = git(
        common_directory,
        &["--git-dir=.", "worktree", "list", "--porcelain", "-z"],
    )?;
    let output = std::str::from_utf8(&output).map_err(|_| "Git checkout paths must be UTF-8")?;
    let mut main_checkout = None;
    let mut worktrees = Vec::new();
    let mut records = output
        .split("\0\0")
        .filter(|record| !record.is_empty())
        .peekable();
    if records.peek().is_none() {
        return Err("Git returned no worktrees for the project".into());
    }
    for (index, record) in records.enumerate() {
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
        if index == 0 && available {
            // Git can report the metadata directory instead of a separate main checkout.
            let Some(checkout) = main_worktree(common_directory, &path, main_hint) else {
                continue;
            };
            main_checkout = Some(checkout.clone());
            path = checkout;
        }
        if available {
            worktrees.push(Worktree { path, branch });
        }
    }
    Ok((main_checkout, worktrees))
}

fn main_worktree(common_directory: &Path, reported: &Path, main_hint: &Path) -> Option<PathBuf> {
    let verify = |candidate: &Path| {
        let root = checkout_root(candidate).ok()?;
        (root != common_directory && worktree_directory(&root).ok()?.as_path() == common_directory)
            .then_some(root)
    };
    for candidate in [reported, main_hint] {
        if candidate != common_directory
            && let Some(checkout) = verify(candidate)
        {
            return Some(checkout);
        }
    }
    let configured = git(
        common_directory,
        &["--git-dir=.", "config", "--path", "--get", "core.worktree"],
    )
    .ok()?;
    let path = os_path(configured.strip_suffix(b"\n").unwrap_or(&configured)).ok()?;
    verify(&common_directory.join(path))
}

pub(crate) fn revisions(checkout: &Path) -> Result<Revisions, String> {
    let refs = git(
        checkout,
        &[
            "for-each-ref",
            "--format=%(refname)%09%(objectname)%09%(symref)%09%(upstream)",
            "refs/heads/",
            "refs/remotes/",
        ],
    )?;
    let refs = String::from_utf8_lossy(&refs);
    let refs: BTreeMap<_, _> = refs
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            Some((
                fields.next()?,
                (fields.next()?, fields.next()?, fields.next()?),
            ))
        })
        .collect();
    let branches: Vec<_> = refs
        .keys()
        .filter_map(|name| name.strip_prefix("refs/heads/").map(str::to_owned))
        .collect();
    let tracked: BTreeMap<_, _> = refs
        .iter()
        .filter_map(|(name, (tip, _, upstream))| {
            if name.starts_with("refs/heads/")
                && refs
                    .get(upstream)
                    .is_some_and(|(remote_tip, _, _)| remote_tip == tip)
            {
                Some((*upstream, *name))
            } else {
                None
            }
        })
        .collect();
    let remote_branches = refs
        .iter()
        .filter_map(|(reference, (tip, symbolic, _))| {
            let name = reference.strip_prefix("refs/remotes/")?;
            let (_, branch) = name.split_once('/')?;
            let local = format!("refs/heads/{branch}");
            let redundant = refs.get(local.as_str()).map_or_else(
                || tracked.contains_key(reference),
                |(local, _, _)| local == tip,
            );
            (symbolic.is_empty() && !redundant).then(|| name.to_owned())
        })
        .collect();
    let default_branch = refs
        .get("refs/remotes/origin/HEAD")
        .or_else(|| {
            refs.iter()
                .find(|(name, (_, target, _))| {
                    name.starts_with("refs/remotes/")
                        && name.ends_with("/HEAD")
                        && !target.is_empty()
                })
                .map(|(_, entry)| entry)
        })
        .and_then(|(_, target, _)| {
            let (_, branch) = target.strip_prefix("refs/remotes/")?.split_once('/')?;
            let local = format!("refs/heads/{branch}");
            Some(if refs.contains_key(local.as_str()) {
                local
            } else {
                tracked.get(target).unwrap_or(target).to_string()
            })
        })
        .or_else(|| {
            let configured = git(checkout, &["config", "--get", "init.defaultBranch"]).ok();
            configured
                .as_deref()
                .and_then(|value| std::str::from_utf8(value).ok())
                .map(str::trim)
                .into_iter()
                .chain(["main", "master"])
                .map(|name| format!("refs/heads/{name}"))
                .find(|name| refs.contains_key(name.as_str()))
                .or_else(|| {
                    ["refs/remotes/origin/main", "refs/remotes/origin/master"]
                        .into_iter()
                        .find(|name| refs.contains_key(*name))
                        .map(str::to_owned)
                })
                .or_else(|| (branches.len() == 1).then(|| format!("refs/heads/{}", branches[0])))
        });
    let current_branch = git(checkout, &["symbolic-ref", "--quiet", "HEAD"])
        .ok()
        .map(|name| String::from_utf8_lossy(&name).trim_end().into());
    let mut args = vec![
        "log",
        "--branches",
        "--remotes",
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
    Ok(Revisions {
        branches,
        remote_branches,
        default_branch,
        current_branch,
        commits,
    })
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

pub(crate) fn resolve_tree(
    checkout: &Path,
    revision: &str,
    objects: Option<&ReviewObjects>,
) -> Result<String, String> {
    let output = git_with_objects(
        checkout,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{tree}}"),
        ],
        &[],
        objects,
    )?;
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
    objects: Option<&ReviewObjects>,
) -> Result<Vec<ChangedFile>, String> {
    let mut args = vec!["diff", "--patch-with-raw", "-z", "--find-renames"];
    args.extend(PATCH_OPTIONS);
    args.extend([start, end, "--"]);
    let output = git_with_objects(checkout, &args, &[], objects)?;
    Ok(parse_patch(&output)?
        .into_iter()
        .map(RawPatch::file)
        .collect())
}

fn index_error(error: io::Error) -> String {
    format!("Cannot prepare the Git index: {error}")
}

fn copy_index(index: &Path, candidate: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(index) {
        Ok(metadata) if metadata.is_file() => {
            fs::copy(index, candidate).map_err(index_error)?;
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Ok(_) => Err("The Git index is not a regular file.".into()),
        Err(error) => Err(index_error(error)),
    }
}

pub(crate) fn snapshot_worktree(
    checkout: &Path,
    objects: &ReviewObjects,
) -> Result<String, String> {
    check_repository_filters(checkout)?;
    // Keep working-tree contents out of the project's object database and refs.
    fs::create_dir_all(&objects.directory)
        .map_err(|error| format!("Cannot store comparison objects: {error}"))?;
    let directory = worktree_directory(checkout)?;
    let scratch = tempfile::tempdir_in(&directory).map_err(index_error)?;
    let candidate = scratch.path().join("index");
    let hooks = format!("core.hooksPath={}", scratch.path().display());
    if !copy_index(&directory.join("index"), &candidate)? {
        git_with_storage(
            checkout,
            &["-c", &hooks, "read-tree", "HEAD"],
            &[],
            Some(&candidate),
            Some(objects),
        )?;
    }
    git_with_storage(
        checkout,
        &["-c", &hooks, "add", "--all", "--"],
        &[],
        Some(&candidate),
        Some(objects),
    )?;
    let tree = git_with_storage(
        checkout,
        &["-c", &hooks, "write-tree"],
        &[],
        Some(&candidate),
        Some(objects),
    )?;
    Ok(String::from_utf8_lossy(&tree).trim_end().into())
}

pub(crate) fn uncommitted_patches(checkout: &Path) -> Result<Vec<WorktreePatch>, String> {
    uncommitted_patches_for(checkout, &[])
}

pub(crate) fn uncommitted_patches_for(
    checkout: &Path,
    paths: &[&OsStr],
) -> Result<Vec<WorktreePatch>, String> {
    check_repository_filters(checkout)?;
    let unmerged = git_with_paths(
        checkout,
        &["--literal-pathspecs", "ls-files", "--unmerged", "-z", "--"],
        paths,
    )?;
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
    let mut remaining = DIFF_LIMIT;
    let mut charge = |output: &[u8]| -> Result<(), String> {
        remaining = remaining.checked_sub(output.len()).ok_or(DIFF_SIZE_ERROR)?;
        Ok(())
    };
    for (side, option) in [
        (PatchSide::Staged, "--cached"),
        (PatchSide::Unstaged, "--ours"),
    ] {
        let mut args = vec![
            "--literal-pathspecs",
            "diff",
            option,
            "--diff-filter=u",
            "--find-renames",
            "--ita-invisible-in-index",
        ];
        args.extend(["--patch-with-raw", "-z", "--full-index", "--abbrev=64"]);
        args.extend(PATCH_OPTIONS);
        args.push("--");
        let output = git_with_paths(checkout, &args, paths)?;
        charge(&output)?;
        for raw in parse_patch(&output)? {
            if !conflicts.contains_key(raw.path) {
                patches.push(raw.worktree_patch(side, None, true));
            }
        }
    }

    let untracked = git_with_paths(
        checkout,
        &[
            "--literal-pathspecs",
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
        ],
        paths,
    )?;
    let untracked_paths = untracked
        .split(|byte| *byte == 0)
        // Git lists an untracked nested repository as a directory.
        .filter(|path| !path.is_empty() && !path.ends_with(b"/"));
    if untracked_paths.clone().count() + conflicts.len() > WORKTREE_FILE_DIFF_LIMIT {
        return Err(format!(
            "Too many untracked or conflicted files to review (limit: {WORKTREE_FILE_DIFF_LIMIT})."
        ));
    }
    let trust_filemode = untracked.is_empty()
        || git(
            checkout,
            &[
                "config",
                "--bool",
                "--default=true",
                "--get",
                "core.filemode",
            ],
        )? != b"false\n";
    for path in untracked_paths {
        let operand = os_path(if path == b"-" { b"./-" } else { path })?;
        let mut args = vec![
            "diff",
            "--no-index",
            "--patch-with-raw",
            "-z",
            "--full-index",
            "--abbrev=64",
        ];
        args.extend(PATCH_OPTIONS);
        args.extend(["--", "/dev/null"]);
        let output = git_with_paths(checkout, &args, &[operand])?;
        charge(&output)?;
        for raw in parse_patch(&output)? {
            patches.push(raw.worktree_patch(PatchSide::Unstaged, Some(path), trust_filemode));
        }
    }

    for (path, stages) in conflicts {
        let (stage, before) = conflict_version(&stages)?;
        let mut args = vec![
            "--literal-pathspecs",
            "diff",
            stage,
            "--diff-filter=u",
            "--patch",
            "--full-index",
        ];
        args.extend(PATCH_OPTIONS);
        args.push("--");
        let output = git_with_paths(checkout, &args, &[os_path(path)?])?;
        charge(&output)?;
        if output.len() > PATCH_LIMIT {
            return Err(PATCH_SIZE_ERROR.into());
        }
        let mut patch = WorktreePatch {
            file: ChangedFile::new(
                'U',
                path,
                None,
                if output.is_empty() {
                    "Unmerged path".into()
                } else {
                    String::from_utf8_lossy(&output).into()
                },
            ),
            side: PatchSide::Unstaged,
            fingerprint: String::new(),
            before: Some(before),
            after: None,
            reviewed: false,
        };
        patch.fingerprint = fingerprint(&patch, &stages, &output);
        patches.push(patch);
    }
    Ok(patches)
}

// Git may invoke clean/process filters when comparing working files as well as staging.
fn check_repository_filters(checkout: &Path) -> Result<(), String> {
    let config = git(
        checkout,
        &["config", "--includes", "--show-scope", "--null", "--list"],
    )?;
    let mut entries = config.split(|&byte| byte == 0);
    while let (Some(scope), Some(entry)) = (entries.next(), entries.next()) {
        let key = entry.split(|&byte| byte == b'\n').next().unwrap_or_default();
        if matches!(scope, b"local" | b"worktree")
            && key.starts_with(b"filter.")
            && (key.ends_with(b".clean") || key.ends_with(b".process"))
        {
            return Err(
                "Diffest cannot review or stage changes with repository-local Git filters.".into(),
            );
        }
    }
    Ok(())
}

fn conflict_version(stages: &[&[u8]]) -> Result<(&'static str, PatchVersion), String> {
    for (number, option) in [("2", "--ours"), ("3", "--theirs"), ("1", "--base")] {
        for header in stages {
            let header = std::str::from_utf8(header).map_err(|_| "Invalid Git index stage")?;
            let mut fields = header.split_whitespace();
            if let (Some(mode), Some(object_id), Some(stage)) =
                (fields.next(), fields.next(), fields.next())
                && stage == number
                && let Some(version) = version(mode, object_id)
            {
                return Ok((option, version));
            }
        }
    }
    Err("Git returned no usable conflict stage".into())
}

pub(crate) fn stage_reviewed_patch(
    checkout: &Path,
    directory: &Path,
    fingerprint: &str,
) -> Result<WorktreePatch, String> {
    let index = directory.join("index");
    let lock_path = directory.join("index.lock");
    let open_error = |error| format!("Cannot lock the Git index: {error}");
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(open_error)?;
    let lock = tempfile::TempPath::try_from_path(lock_path).map_err(open_error)?;

    let changed = "The unstaged patch changed. Review it again before staging.";
    let patch = uncommitted_patches(checkout)?
        .into_iter()
        .find(|patch| patch.side == PatchSide::Unstaged && patch.fingerprint == fingerprint)
        .ok_or(changed)?;
    if patch.file.status == 'U' {
        return Err("Resolve this conflict in Git before staging.".into());
    }
    if patch
        .after
        .as_ref()
        .is_some_and(|version| version.mode == GITLINK_MODE)
        && patch
            .file
            .patch
            .lines()
            .any(|line| line.starts_with("+Subproject commit ") && line.ends_with("-dirty"))
    {
        return Err("The submodule has worktree changes that cannot be staged here.".into());
    }

    // Git can read the worktree again during add. Stage in a copy and publish it
    // only if the resulting entries match the reviewed versions and paths.
    let scratch = tempfile::tempdir_in(directory).map_err(index_error)?;
    let candidate = scratch.path().join("index");
    copy_index(&index, &candidate)?;

    let list = ["ls-files", "--stage", "-z"];
    let original = git(checkout, &list)?;
    let path = patch.file.path_bytes();
    let previous = patch.file.previous_path_bytes();
    let paths = once(path)
        .chain(previous)
        .map(os_path)
        .collect::<Result<Vec<_>, _>>()?;
    check_repository_filters(checkout)?;
    git_with_index(
        checkout,
        &["--literal-pathspecs", "add", "--all", "--"],
        &paths,
        Some(&candidate),
    )?;

    let mut expected: Vec<_> = index_records(&original)
        .filter(|record| {
            let recorded_path = record.splitn(2, |byte| *byte == b'\t').nth(1);
            recorded_path != Some(path) && recorded_path != previous
        })
        .collect();
    let after = patch.after.as_ref().map(|version| {
        let mut record = format!("{} {} 0\t", version.mode, version.object_id).into_bytes();
        record.extend_from_slice(path);
        record
    });
    expected.extend(after.as_deref());
    expected.sort_unstable();

    let staged = git_with_index(checkout, &list, &[], Some(&candidate))?;
    let mut actual: Vec<_> = index_records(&staged).collect();
    actual.sort_unstable();
    if actual != expected {
        return Err(changed.into());
    }

    fs::rename(candidate, &lock).map_err(index_error)?;
    lock.persist(index)
        .map_err(|error| format!("Cannot write the Git index: {error}"))?;
    Ok(patch)
}

fn index_records(list: &[u8]) -> impl Iterator<Item = &[u8]> {
    list.split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
}

pub(crate) fn os_path(path: &[u8]) -> Result<&OsStr, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(OsStr::from_bytes(path))
    }
    #[cfg(not(unix))]
    {
        std::str::from_utf8(path)
            .map(OsStr::new)
            .map_err(|_| "Git worktree paths must be UTF-8 on this platform".into())
    }
}

pub(crate) fn display_path(path: &[u8]) -> String {
    let mut label = String::new();
    for chunk in path.utf8_chunks() {
        label.push_str(chunk.valid());
        for byte in chunk.invalid() {
            let _ = write!(label, "\\x{byte:02x}");
        }
    }
    label
}

fn fingerprint(patch: &WorktreePatch, extra: &[&[u8]], diff: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update([patch.side as u8, patch.file.status as u8]);
    for field in [
        Some(patch.file.path_bytes()),
        patch.file.previous_path_bytes(),
    ] {
        hash.update(field.unwrap_or_default());
        hash.update([0]);
    }
    for version in [&patch.before, &patch.after] {
        // Conflict stage records already bind the mark to the index versions.
        if patch.file.status != 'U'
            && let Some(version) = version
        {
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
        ChangedFile::new(
            self.status,
            self.path,
            self.previous_path,
            String::from_utf8_lossy(self.patch).into(),
        )
    }

    fn worktree_patch(
        self,
        side: PatchSide,
        path: Option<&[u8]>,
        trust_filemode: bool,
    ) -> WorktreePatch {
        let mut fields = self.header.trim_start_matches(':').split_whitespace();
        let [old_mode, new_mode, old_id, new_id] =
            std::array::from_fn(|_| fields.next().unwrap_or_default());
        let normalize_mode = !trust_filemode && new_mode == EXECUTABLE_MODE;
        let new_mode = if normalize_mode {
            REGULAR_MODE
        } else {
            new_mode
        };
        let before = version(old_mode, old_id);
        let mut after_id = new_id;
        if new_mode != MISSING_MODE && new_id.bytes().all(|byte| byte == b'0') {
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
        let mut file = ChangedFile::new(
            self.status,
            path.unwrap_or(self.path),
            self.previous_path,
            String::from_utf8_lossy(diff).into(),
        );
        if normalize_mode {
            file.patch = file.patch.replacen(
                &format!("\nnew file mode {EXECUTABLE_MODE}\n"),
                &format!("\nnew file mode {REGULAR_MODE}\n"),
                1,
            );
        }
        let mut patch = WorktreePatch {
            file,
            side,
            fingerprint: String::new(),
            before,
            after,
            reviewed: false,
        };
        // For a new file with filemode disabled, hash the same normalized patch shown to the reviewer.
        let diff = if trust_filemode {
            diff
        } else {
            patch.file.patch.as_bytes()
        };
        patch.fingerprint = fingerprint(&patch, &[], diff);
        patch
    }
}

fn version(mode: &str, object_id: &str) -> Option<PatchVersion> {
    (mode != MISSING_MODE).then(|| PatchVersion {
        mode: mode.into(),
        object_id: object_id.into(),
    })
}

fn parse_patch(output: &[u8]) -> Result<Vec<RawPatch<'_>>, String> {
    let patches = parse_raw_patch(output).ok_or("Unexpected output from Git diff")?;
    if patches.iter().any(|file| file.patch.len() > PATCH_LIMIT) {
        return Err(PATCH_SIZE_ERROR.into());
    }
    Ok(patches)
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

#[cfg(test)]
mod tests {
    use super::git;
    use std::process::Command;

    #[test]
    fn git_commands_disable_the_repository_fsmonitor_setting() {
        let repo = tempfile::tempdir().unwrap();
        for args in [&["init", "--quiet"][..], &["config", "core.fsmonitor", "true"]] {
            assert!(Command::new("git")
                .current_dir(repo.path())
                .args(args)
                .status()
                .unwrap()
                .success());
        }
        let configured = git(repo.path(), &["config", "--bool", "core.fsmonitor"]).unwrap();
        assert_eq!(configured, b"false\n");
    }
}
