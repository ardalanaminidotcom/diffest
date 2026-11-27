use base64::{Engine as _, engine::general_purpose::STANDARD};
use diffest_core::{Application, ComparisonMode, FileDetailsSource, PatchSide};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim_end().into()
}

fn init_project(directory: &Path) -> PathBuf {
    let main = directory.join("project");
    fs::create_dir(&main).unwrap();
    git(&main, &["init", "-b", "main"]);
    configure_user(&main);
    main.canonicalize().unwrap()
}

fn worktree_source(patch: &diffest_core::WorktreePatch) -> FileDetailsSource {
    FileDetailsSource::Worktree {
        fingerprint: patch.fingerprint.clone(),
        path: patch.file.path.clone(),
        previous_path: patch.file.previous_path.clone(),
        path_bytes: patch.file.path_bytes.clone(),
        previous_path_bytes: patch.file.previous_path_bytes.clone(),
    }
}

#[test]
fn file_details_use_the_index_between_the_committed_and_working_versions() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    let path = main.join("notes.txt");
    fs::write(&path, "committed\n").unwrap();
    git(&main, &["add", "notes.txt"]);
    git(&main, &["commit", "-m", "Initial version"]);
    fs::write(&path, "index\n").unwrap();
    git(&main, &["add", "notes.txt"]);
    fs::write(&path, "working tree\n").unwrap();

    let app = Application::new(temp.path().join("reviews.sqlite3"));
    let patches = app.open_worktree_review(&main).unwrap();
    let sizes = |side| {
        let patch = patches.iter().find(|patch| patch.side == side).unwrap();
        let details = app
            .file_details(&main, worktree_source(patch))
            .unwrap();
        (
            details.before.unwrap().byte_size,
            details.after.unwrap().byte_size,
        )
    };
    assert_eq!(sizes(PatchSide::Staged), (Some(10), Some(6)));
    assert_eq!(sizes(PatchSide::Unstaged), (Some(6), Some(13)));

    let reviewed = patches
        .iter()
        .find(|patch| patch.side == PatchSide::Unstaged)
        .unwrap();
    fs::write(&path, "changed after opening\n").unwrap();
    assert!(
        app.file_details(&main, worktree_source(reviewed))
        .is_err()
    );
}

const GIF: &str = "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jP1sAAAAASUVORK5CYII=";

#[test]
fn added_and_deleted_images_show_their_present_side_and_corrupt_images_keep_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    let gif = STANDARD.decode(GIF).unwrap();
    let png = STANDARD.decode(PNG).unwrap();
    fs::write(main.join("deleted.gif"), &gif).unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-m", "Original image"]);
    fs::remove_file(main.join("deleted.gif")).unwrap();
    fs::write(main.join("added.png"), &png).unwrap();
    fs::write(main.join("broken.png"), "invalid image").unwrap();

    let app = Application::new(temp.path().join("reviews.sqlite3"));
    let patches = app.open_worktree_review(&main).unwrap();
    assert_eq!(patches.len(), 3);
    for patch in patches {
        let details = app
            .file_details(&main, worktree_source(&patch))
            .unwrap();
        match patch.file.path.as_str() {
            "deleted.gif" => {
                assert!(details.after.is_none());
                let before = details.before.unwrap();
                assert_eq!(before.byte_size, Some(gif.len() as u64));
                assert_eq!(before.format, Some("GIF"));
                assert_eq!(before.preview, Some(format!("data:image/gif;base64,{GIF}")));
            }
            "added.png" => {
                assert!(details.before.is_none());
                let after = details.after.unwrap();
                assert_eq!(after.byte_size, Some(png.len() as u64));
                assert_eq!(after.format, Some("PNG"));
                assert_eq!(after.preview, Some(format!("data:image/png;base64,{PNG}")));
            }
            "broken.png" => {
                assert!(details.before.is_none());
                let after = details.after.unwrap();
                assert_eq!(after.byte_size, Some(13));
                assert_eq!(after.format, Some("PNG"));
                assert!(after.preview.is_none());
            }
            path => panic!("Unexpected patch for {path}"),
        }
    }
}

#[test]
fn git_paired_rename_compares_both_paths_and_detects_their_actual_formats() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    fs::create_dir(main.join("images")).unwrap();
    // Shared trailing bytes allow Git to pair the files while their image headers differ.
    let padding = b"retained image metadata\n".repeat(64);
    let gif = [STANDARD.decode(GIF).unwrap(), padding.clone()].concat();
    let png = [STANDARD.decode(PNG).unwrap(), padding].concat();
    fs::write(main.join("images/source.gif"), &gif).unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-m", "Original format"]);
    fs::remove_file(main.join("images/source.gif")).unwrap();
    fs::write(main.join("images/destination.png"), &png).unwrap();
    git(&main, &["add", "-A"]);
    git(&main, &["commit", "-m", "Rename and convert"]);

    let app = Application::new(temp.path().join("reviews.sqlite3"));
    let comparison = app
        .compare_revisions(&main, "HEAD^", "HEAD", ComparisonMode::Between)
        .unwrap();
    assert_eq!(comparison.files.len(), 1);
    let file = comparison.files.into_iter().next().unwrap();
    assert_eq!(file.status, 'R');
    let details = app
        .file_details(
            &main,
            FileDetailsSource::Comparison {
                start: comparison.start,
                end: comparison.end,
                path: file.path,
                previous_path: file.previous_path,
                path_bytes: file.path_bytes,
                previous_path_bytes: file.previous_path_bytes,
                status: file.status,
            },
        )
        .unwrap();
    let before = details.before.unwrap();
    let after = details.after.unwrap();
    assert_eq!(before.path, "images/source.gif");
    assert_eq!(before.format, Some("GIF"));
    assert_eq!(before.byte_size, Some(gif.len() as u64));
    assert_eq!(after.path, "images/destination.png");
    assert_eq!(after.format, Some("PNG"));
    assert_eq!(after.byte_size, Some(png.len() as u64));
    assert!(before.preview.is_some() && after.preview.is_some());
}

#[test]
fn ongoing_review_tracks_each_patch_in_its_worktree_and_preserves_only_current_marks() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    fs::write(main.join(".gitignore"), "*.local\n").unwrap();
    fs::write(main.join("shared.local"), "committed\n").unwrap();
    git(&main, &["add", ".gitignore"]);
    git(&main, &["add", "-f", "shared.local"]);
    git(&main, &["commit", "-m", "Initial commit"]);
    git(&main, &["worktree", "add", "-b", "topic", "../linked"]);
    let linked = temp.path().join("linked");
    for checkout in [&main, &linked] {
        fs::write(checkout.join("shared.local"), "index\n").unwrap();
        git(checkout, &["add", "shared.local"]);
        fs::write(checkout.join("shared.local"), "worktree\n").unwrap();
        fs::write(checkout.join("new file.txt"), "new content\n").unwrap();
        fs::write(checkout.join("ignore-me.local"), "hidden\n").unwrap();
    }

    let database = temp.path().join("diffest.sqlite3");
    let app = Application::new(database.clone());
    app.register_project(&main).unwrap();
    let patches = app.open_worktree_review(&main).unwrap();
    assert_eq!(patches.len(), 3);
    let staged = &patches[0];
    let unstaged = &patches[1];
    let added = &patches[2];
    assert_eq!(staged.side, PatchSide::Staged);
    assert_eq!(unstaged.side, PatchSide::Unstaged);
    assert_eq!(added.side, PatchSide::Unstaged);
    assert_eq!(staged.file.path, "shared.local");
    assert_eq!(unstaged.file.path, "shared.local");
    assert!(staged.file.patch.contains("-committed\n+index\n"));
    assert!(unstaged.file.patch.contains("-index\n+worktree\n"));
    assert_eq!(staged.after, unstaged.before);
    assert_eq!(
        staged.after.as_ref().unwrap().object_id,
        git(&main, &["rev-parse", ":shared.local"])
    );
    assert_eq!(added.file.status, 'A');
    assert_eq!(added.file.path, "new file.txt");
    assert!(added.file.patch.contains("+new content\n"));
    assert_eq!(added.before, None);
    assert_eq!(
        added.after.as_ref().unwrap().object_id,
        git(&main, &["hash-object", "new file.txt"])
    );

    let marked = app
        .set_worktree_patch_reviewed(&main, &staged.fingerprint, true)
        .unwrap();
    assert!(marked[0].reviewed);
    assert!(!marked[1].reviewed);
    app.set_worktree_patch_reviewed(&main, &unstaged.fingerprint, true)
        .unwrap();
    app.set_worktree_patch_reviewed(&main, &added.fingerprint, true)
        .unwrap();
    assert!(
        app.open_worktree_review(&linked)
            .unwrap()
            .iter()
            .all(|patch| !patch.reviewed)
    );
    drop(app);

    let app = Application::new(database);
    assert!(
        app.open_worktree_review(&main)
            .unwrap()
            .iter()
            .all(|patch| patch.reviewed)
    );
    fs::write(main.join("shared.local"), "new worktree contents\n").unwrap();
    let refreshed = app
        .set_worktree_patch_reviewed(&main, &unstaged.fingerprint, true)
        .unwrap();
    assert!(refreshed[0].reviewed);
    assert!(!refreshed[1].reviewed);
    assert!(refreshed[2].reviewed);
    assert_ne!(refreshed[1].fingerprint, unstaged.fingerprint);
    let refreshed = app
        .set_worktree_patch_reviewed(&main, &staged.fingerprint, false)
        .unwrap();
    assert!(!refreshed[0].reviewed);
    assert!(refreshed[2].reviewed);

    fs::write(main.join("shared.local"), "worktree\n").unwrap();
    assert!(!app.open_worktree_review(&main).unwrap()[1].reviewed);
}

#[test]
fn conflicts_and_binary_changes_remain_reviewable_and_lose_marks_when_edited() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    fs::write(main.join("conflict.txt"), "original\n").unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-m", "Initial commit"]);
    git(&main, &["switch", "-c", "topic"]);
    fs::write(main.join("conflict.txt"), "topic\n").unwrap();
    git(&main, &["commit", "-am", "Topic edit"]);
    git(&main, &["switch", "main"]);
    fs::write(main.join("conflict.txt"), "main\n").unwrap();
    git(&main, &["commit", "-am", "Main edit"]);
    let merge = Command::new("git")
        .current_dir(&main)
        .args(["merge", "topic"])
        .output()
        .unwrap();
    assert!(!merge.status.success());
    fs::write(main.join("binary.bin"), b"\0a").unwrap();

    let app = Application::new(temp.path().join("diffest.sqlite3"));
    let patches = app.open_worktree_review(&main).unwrap();
    assert_eq!(patches.len(), 2);
    let binary = &patches[0];
    assert_eq!(binary.file.path, "binary.bin");
    assert!(binary.file.patch.contains("Binary files"));
    let conflict = &patches[1];
    assert_eq!(conflict.side, PatchSide::Unstaged);
    assert_eq!(conflict.file.path, "conflict.txt");
    assert_eq!(conflict.file.status, 'U');
    assert!(conflict.file.patch.contains("<<<<<<< HEAD"));
    let details = app
        .file_details(&main, worktree_source(conflict))
        .unwrap();
    assert_eq!(details.before.unwrap().byte_size, Some(5));
    for patch in &patches {
        app.set_worktree_patch_reviewed(&main, &patch.fingerprint, true)
            .unwrap();
    }
    fs::write(main.join("binary.bin"), b"\0b").unwrap();
    fs::write(main.join("conflict.txt"), "main\n").unwrap();
    let refreshed = app.open_worktree_review(&main).unwrap();
    assert_eq!(refreshed.len(), 2);
    assert_ne!(refreshed[0].after, binary.after);
    assert_eq!(refreshed[1].file.status, 'U');
    assert!(refreshed.iter().all(|patch| !patch.reviewed));
}

fn configure_user(main: &Path) {
    git(main, &["config", "user.name", "Diffest Tests"]);
    git(main, &["config", "user.email", "tests@example.com"]);
}

#[test]
fn adding_a_linked_checkout_selects_it_and_registers_one_project_across_restarts() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    git(&main, &["commit", "--allow-empty", "-m", "Initial commit"]);
    git(
        &main,
        &["worktree", "add", "-b", "topic", "../linked checkout"],
    );
    let linked = temp.path().join("linked checkout").canonicalize().unwrap();
    let database = temp.path().join("app-data/diffest.sqlite3");
    let app = Application::new(database.clone());

    assert!(app.register_project(&temp.path().join("missing")).is_err());
    assert!(app.list_projects().unwrap().is_empty());

    let registered = app.register_project(&linked).unwrap();
    assert_eq!(registered.checkout, linked);
    assert_eq!(
        registered.project.main_checkout.as_deref(),
        Some(main.as_path())
    );
    assert_eq!(registered.project.worktrees.len(), 2);

    let registered_again = app.register_project(&main).unwrap();
    assert_eq!(registered_again.checkout, main);
    assert_eq!(registered_again.project.id, registered.project.id);

    let reopened = Application::new(database).list_projects().unwrap();
    assert_eq!(reopened.len(), 1);
    assert_eq!(reopened[0].id, registered.project.id);
    assert_eq!(reopened[0].worktrees[0].branch.as_deref(), Some("main"));
    assert_eq!(reopened[0].worktrees[1].path, linked);
    assert_eq!(reopened[0].worktrees[1].branch.as_deref(), Some("topic"));
}

#[test]
fn reopening_a_project_with_separate_git_metadata_uses_its_registered_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let main = temp.path().join("separate checkout");
    fs::create_dir(&main).unwrap();
    let main = main.canonicalize().unwrap();
    git(
        &main,
        &["init", "-b", "main", "--separate-git-dir", "../git-storage"],
    );
    configure_user(&main);
    git(&main, &["commit", "--allow-empty", "-m", "Initial commit"]);
    git(
        &main,
        &["worktree", "add", "-b", "topic", "../linked checkout"],
    );
    let linked = temp.path().join("linked checkout").canonicalize().unwrap();
    let database = temp.path().join("diffest.sqlite3");
    let app = Application::new(database.clone());

    let first = app.register_project(&linked).unwrap();
    assert!(first.project.main_checkout.is_none());
    assert_eq!(first.project.worktrees.len(), 1);
    assert_eq!(first.project.worktrees[0].path, linked);
    assert!(first.project.worktree_error.is_some());
    assert!(app.list_projects().unwrap()[0].main_checkout.is_none());

    let registration = app.register_project(&main).unwrap();
    assert_eq!(registration.project.id, first.project.id);
    assert_eq!(registration.project.worktrees[0].path, main);
    assert!(registration.project.worktree_error.is_none());
    let sibling = app.register_project(&linked).unwrap();
    assert_eq!(sibling.project.id, registration.project.id);
    assert_eq!(sibling.project.worktrees[0].path, main);
    assert_eq!(sibling.project.worktrees[1].path, linked);
    let reopened = Application::new(database).list_projects().unwrap();
    assert_eq!(reopened[0].worktrees[0].path, main);
}

#[test]
fn linked_checkout_registration_recovers_the_main_checkout_from_git_config() {
    let temp = tempfile::tempdir().unwrap();
    let main = temp.path().join("main checkout");
    fs::create_dir(&main).unwrap();
    let main = main.canonicalize().unwrap();
    git(
        &main,
        &["init", "-b", "main", "--separate-git-dir", "../git-storage"],
    );
    git(&main, &["config", "core.worktree", "../main checkout"]);
    configure_user(&main);
    git(&main, &["commit", "--allow-empty", "-m", "Initial"]);
    git(&main, &["worktree", "add", "-b", "topic", "../linked"]);
    let linked = temp.path().join("linked").canonicalize().unwrap();

    let app = Application::new(temp.path().join("reviews.sqlite3"));
    let registration = app.register_project(&linked).unwrap();
    assert_eq!(
        registration.project.main_checkout.as_deref(),
        Some(main.as_path())
    );
    assert_eq!(registration.project.worktrees[0].path, main);
    assert_eq!(registration.project.worktrees[1].path, linked);
    assert!(registration.project.worktree_error.is_none());
}

#[test]
fn branch_choices_use_the_default_branch_and_distinguish_different_tips() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = init_project(temp.path());
    fs::write(checkout.join("tracked.txt"), "initial\n").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "Initial"]);
    git(&checkout, &["branch", "trunk"]);
    git(
        &checkout,
        &["update-ref", "refs/remotes/origin/trunk", "HEAD"],
    );
    git(
        &checkout,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
        ],
    );
    let app = Application::new(temp.path().join("diffest.sqlite3"));
    let synced = app.open_worktree(&checkout).unwrap();
    assert_eq!(synced.default_branch.as_deref(), Some("refs/heads/trunk"));
    assert_eq!(synced.current_branch.as_deref(), Some("refs/heads/main"));
    assert!(synced.remote_branches.is_empty());

    fs::write(checkout.join("tracked.txt"), "remote change\n").unwrap();
    git(&checkout, &["commit", "-am", "Remote change"]);
    git(
        &checkout,
        &["update-ref", "refs/remotes/origin/trunk", "HEAD"],
    );
    let diverged = app.open_worktree(&checkout).unwrap();
    assert_eq!(diverged.remote_branches, ["origin/trunk"]);
    assert!(diverged.branches.iter().any(|name| name == "trunk"));
}

#[test]
fn first_review_mark_saves_the_working_tree_snapshot_without_changing_the_index() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = init_project(temp.path());
    fs::write(checkout.join("tracked.txt"), "original\n").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "Base"]);
    fs::write(checkout.join("tracked.txt"), "staged\n").unwrap();
    git(&checkout, &["add", "tracked.txt"]);
    fs::write(checkout.join("tracked.txt"), "working\n").unwrap();
    fs::write(checkout.join("new.txt"), "untracked\n").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let hooks = checkout.join(".git/hooks");
        fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("post-index-change");
        fs::write(&hook, "#!/bin/sh\ntouch unexpected-hook.txt\n").unwrap();
        fs::set_permissions(hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let app = Application::new(temp.path().join("diffest.sqlite3"));
    app.register_project(&checkout).unwrap();
    let comparison = app
        .compare_revisions(&checkout, "HEAD", "Working tree", ComparisonMode::Between)
        .unwrap();
    assert!(!checkout.join("unexpected-hook.txt").exists());
    assert!(
        !Command::new("git")
            .current_dir(&checkout)
            .args(["cat-file", "-e", &comparison.end])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(git(&checkout, &["show", ":tracked.txt"]), "staged");
    assert_eq!(comparison.files.len(), 2);
    assert!(
        comparison
            .files
            .iter()
            .any(|file| file.patch.contains("+working"))
    );
    assert!(
        app.find_commit_review(&checkout, &comparison)
            .unwrap()
            .is_none()
    );
    assert!(
        app.start_commit_review(&checkout, &comparison, None, 2)
            .is_err()
    );
    assert!(
        app.find_commit_review(&checkout, &comparison)
            .unwrap()
            .is_none()
    );

    let first = app
        .start_commit_review(&checkout, &comparison, None, 0)
        .unwrap();
    assert_eq!(first.reviewed_files, [0]);
    let again = app
        .start_commit_review(&checkout, &comparison, None, 1)
        .unwrap();
    assert_eq!(again.summary.id, first.summary.id);
    let resumed = app
        .find_commit_review(&checkout, &comparison)
        .unwrap()
        .unwrap();
    assert_eq!(resumed.reviewed_files, [0, 1]);

    fs::write(checkout.join("tracked.txt"), "other edit\n").unwrap();
    let changed = app
        .compare_revisions(&checkout, "HEAD", "Working tree", ComparisonMode::Between)
        .unwrap();
    assert!(
        app.find_commit_review(&checkout, &changed)
            .unwrap()
            .is_none()
    );
    let original = app.open_commit_review(&checkout, first.summary.id).unwrap();
    assert!(
        original
            .comparison
            .files
            .iter()
            .any(|file| file.patch.contains("+working"))
    );
    let details = app
        .file_details(
            &checkout,
            FileDetailsSource::Comparison {
                start: original.comparison.start,
                end: original.comparison.end,
                path: "tracked.txt".into(),
                previous_path: None,
                path_bytes: None,
                previous_path_bytes: None,
                status: 'M',
            },
        )
        .unwrap();
    assert_eq!(details.after.unwrap().byte_size, Some(8));
}

#[test]
fn comparing_diverged_branches_uses_effective_commits_and_returns_each_files_patch() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    fs::write(main.join("old name.txt"), "Rename this file\n").unwrap();
    fs::write(main.join("remove.txt"), "Delete this file\n").unwrap();
    fs::write(main.join("type.txt"), "Alternate file type\n").unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-m", "Common ancestor"]);
    let ancestor = git(&main, &["rev-parse", "HEAD"]);

    git(
        &main,
        &["worktree", "add", "-b", "topic", "../linked checkout"],
    );
    let linked = temp.path().join("linked checkout");
    fs::write(main.join("main-only.txt"), "Only on main\n").unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-m", "Main change"]);
    let main_tip = git(&main, &["rev-parse", "HEAD"]);

    git(&linked, &["mv", "old name.txt", "new name.txt"]);
    git(&linked, &["rm", "remove.txt"]);
    fs::write(linked.join("topic-only.txt"), "diff --git inside a hunk\n").unwrap();
    git(&linked, &["add", "."]);
    let blob = git(&linked, &["rev-parse", "HEAD:type.txt"]);
    git(
        &linked,
        &[
            "update-index",
            "--cacheinfo",
            &format!("120000,{blob},type.txt"),
        ],
    );
    git(&linked, &["commit", "-m", "Topic changes"]);
    let topic_tip = git(&linked, &["rev-parse", "HEAD"]);

    let app = Application::new(temp.path().join("diffest.sqlite3"));
    let revisions = app.open_worktree(&linked).unwrap();
    assert_eq!(revisions.branches, ["main", "topic"]);
    assert!(
        revisions
            .commits
            .iter()
            .any(|commit| commit.id == ancestor && commit.subject == "Common ancestor")
    );

    let between = app
        .compare_revisions(&linked, "refs/heads/main", "HEAD", ComparisonMode::Between)
        .unwrap();
    assert_eq!(between.start, main_tip);
    assert_eq!(between.end, topic_tip);
    assert!(
        between
            .files
            .iter()
            .any(|file| file.path == "main-only.txt" && file.status == 'D')
    );

    let divergence = app
        .compare_revisions(
            &main,
            "refs/heads/main",
            "refs/heads/topic",
            ComparisonMode::Divergence,
        )
        .unwrap();
    assert_eq!(divergence.start, ancestor);
    assert_eq!(divergence.end, topic_tip);
    assert_eq!(divergence.files.len(), 4);
    let renamed = divergence
        .files
        .iter()
        .find(|file| file.status == 'R')
        .unwrap();
    assert_eq!(renamed.path, "new name.txt");
    assert_eq!(renamed.previous_path.as_deref(), Some("old name.txt"));
    assert!(
        renamed
            .patch
            .contains("rename from old name.txt\nrename to new name.txt")
    );
    let deleted = divergence
        .files
        .iter()
        .find(|file| file.status == 'D')
        .unwrap();
    assert_eq!(deleted.path, "remove.txt");
    assert!(deleted.patch.contains("-Delete this file"));
    let added = divergence
        .files
        .iter()
        .find(|file| file.status == 'A')
        .unwrap();
    assert_eq!(added.path, "topic-only.txt");
    assert!(added.patch.contains("+diff --git inside a hunk"));
    assert!(!added.patch.contains("Delete this file"));
    let type_change = divergence
        .files
        .iter()
        .find(|file| file.status == 'T')
        .unwrap();
    assert!(type_change.patch.contains("deleted file mode 100644"));
    assert!(type_change.patch.contains("new file mode 120000"));

    let same_branch = app
        .compare_revisions(&linked, &ancestor, &topic_tip, ComparisonMode::Between)
        .unwrap();
    assert_eq!(same_branch.files.len(), 4);
    let identical = app
        .compare_revisions(&linked, &topic_tip, "HEAD", ComparisonMode::Between)
        .unwrap();
    assert!(identical.files.is_empty());
    assert!(
        app.compare_revisions(&linked, "--help", "HEAD", ComparisonMode::Between)
            .is_err()
    );
}

#[test]
fn a_saved_divergence_comparison_reopens_from_a_sibling_after_the_branch_moves() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    git(&main, &["commit", "--allow-empty", "-m", "Common ancestor"]);
    let ancestor = git(&main, &["rev-parse", "HEAD"]);
    git(&main, &["worktree", "add", "-b", "topic", "../linked"]);
    let linked = temp.path().join("linked");

    fs::write(main.join("main.txt"), "Main branch\n").unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-m", "Main change"]);
    let main_tip = git(&main, &["rev-parse", "HEAD"]);

    fs::write(linked.join("one.txt"), "First change\n").unwrap();
    fs::write(linked.join("two.txt"), "Second change\n").unwrap();
    git(&linked, &["add", "."]);
    git(&linked, &["commit", "-m", "Topic changes"]);
    let topic_tip = git(&linked, &["rev-parse", "HEAD"]);

    let database = temp.path().join("diffest.sqlite3");
    let app = Application::new(database.clone());
    app.register_project(&main).unwrap();
    let comparison = app
        .compare_revisions(&main, "main", "topic", ComparisonMode::Divergence)
        .unwrap();

    fs::write(linked.join("one.txt"), "Updated branch\n").unwrap();
    git(&linked, &["commit", "-am", "Move topic before saving"]);
    let saved = app.save_commit_review(&main, &comparison).unwrap();
    assert_eq!(
        app.set_commit_file_reviewed(&main, saved, 0, true).unwrap(),
        [0]
    );
    fs::write(linked.join("two.txt"), "Updated after saving\n").unwrap();
    git(&linked, &["commit", "-am", "Move topic after saving"]);
    drop(app);

    let app = Application::new(database);
    let reopened = app.open_commit_review(&linked, saved).unwrap();
    let summary = &reopened.summary;
    assert_eq!(summary.id, saved);
    assert_eq!(summary.base_label, "main");
    assert_eq!(summary.target_label, "topic");
    assert_eq!(summary.file_count, 2);
    assert_eq!(summary.reviewed_count, 1);

    assert_eq!(reopened.comparison.base, main_tip);
    assert_eq!(reopened.comparison.start, ancestor);
    assert_eq!(reopened.comparison.end, topic_tip);
    assert_eq!(reopened.comparison.mode, ComparisonMode::Divergence);
    assert_eq!(reopened.comparison.files.len(), 2);
    assert_eq!(reopened.comparison.files[0].path, "one.txt");
    assert!(
        reopened.comparison.files[0]
            .patch
            .contains("+First change\n")
    );
    assert_eq!(reopened.comparison.files[1].path, "two.txt");
    assert_eq!(
        reopened.comparison.files[1].patch,
        comparison.files[1].patch
    );
    assert_eq!(reopened.reviewed_files, [0]);

    let updated = app
        .update_commit_file_review(&linked, saved, 1, true)
        .unwrap();
    assert_eq!(updated.reviewed_files, [0, 1]);
    let completed = &app.open_commit_review(&linked, saved).unwrap().summary;
    assert_eq!(completed.reviewed_count, completed.file_count);
    assert_eq!(&updated.summary, completed);

    assert!(
        app.set_commit_file_reviewed(&linked, saved, 2, true)
            .is_err()
    );
    assert_eq!(
        app.set_commit_file_reviewed(&linked, saved, 0, false)
            .unwrap(),
        [1]
    );
    assert_eq!(
        app.open_commit_review(&linked, saved)
            .unwrap()
            .summary
            .reviewed_count,
        1
    );

    let other = temp.path().join("other");
    fs::create_dir(&other).unwrap();
    let unrelated = init_project(&other);
    app.register_project(&unrelated).unwrap();
    assert!(app.open_commit_review(&unrelated, saved).is_err());
    assert!(
        app.set_commit_file_reviewed(&unrelated, saved, 1, false)
            .is_err()
    );
    assert_eq!(
        app.open_commit_review(&main, saved).unwrap().reviewed_files,
        [1]
    );
}

#[test]
fn layout_overrides_reopen_with_their_saved_review_or_worktree() {
    use diffest_core::DiffLayout;

    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    fs::write(main.join("changed.txt"), "before\n").unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-m", "Before"]);
    fs::write(main.join("changed.txt"), "after\n").unwrap();
    git(&main, &["commit", "-am", "After"]);
    git(&main, &["worktree", "add", "-b", "topic", "../linked"]);
    let linked = temp.path().join("linked");

    let database = temp.path().join("diffest.sqlite3");
    let app = Application::new(database.clone());
    app.register_project(&main).unwrap();
    let comparison = app
        .compare_revisions(&main, "HEAD^", "HEAD", ComparisonMode::Between)
        .unwrap();
    let first = app
        .save_commit_review_with_layout(&main, &comparison, Some(DiffLayout::Split))
        .unwrap();
    let second = app.save_commit_review(&main, &comparison).unwrap();
    app.set_review_layout(&main, Some(second), DiffLayout::Split)
        .unwrap();
    app.set_review_layout(&main, Some(second), DiffLayout::Unified)
        .unwrap();
    let inherited = app.save_commit_review(&main, &comparison).unwrap();
    app.set_review_layout(&main, None, DiffLayout::Split)
        .unwrap();
    drop(app);

    let app = Application::new(database);
    assert_eq!(
        app.read_review_layout(&linked, Some(first)).unwrap(),
        Some(DiffLayout::Split)
    );
    assert_eq!(
        app.read_review_layout(&linked, Some(second)).unwrap(),
        Some(DiffLayout::Unified)
    );
    assert_eq!(
        app.read_review_layout(&linked, Some(inherited)).unwrap(),
        None
    );
    assert_eq!(
        app.read_review_layout(&main, None).unwrap(),
        Some(DiffLayout::Split)
    );
    assert_eq!(app.read_review_layout(&linked, None).unwrap(), None);
    assert!(
        app.set_review_layout(&linked, Some(999), DiffLayout::Split)
            .is_err()
    );
}

#[cfg(unix)]
#[test]
fn worktree_review_stages_the_selected_byte_path_and_reads_its_details() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    let invalid = main.join(OsStr::from_bytes(b"note-\xff.txt"));
    fs::write(&invalid, "invalid name\n").unwrap();
    fs::write(main.join("note-�.txt"), "valid name\n").unwrap();

    let app = Application::new(temp.path().join("reviews.sqlite3"));
    let patches = app.open_worktree_review(&main).unwrap();
    assert_eq!(patches.len(), 2);
    let patch = patches
        .iter()
        .find(|patch| patch.file.patch.contains("+invalid name\n"))
        .unwrap();
    assert_eq!(patch.file.path, r"note-\xff.txt");
    let details = app
        .file_details(&main, worktree_source(patch))
        .unwrap();
    assert_eq!(details.after.unwrap().byte_size, Some(13));

    app.set_worktree_patch_reviewed(&main, &patch.fingerprint, true)
        .unwrap();
    let result = app.stage_worktree_patch(&main, &patch.fingerprint).unwrap();
    assert_eq!(result.error, None);
    assert!(
        result
            .patches
            .iter()
            .any(|patch| patch.side == PatchSide::Staged
                && patch.reviewed
                && patch.file.patch.contains("+invalid name\n"))
    );
    assert!(
        result
            .patches
            .iter()
            .any(|patch| patch.side == PatchSide::Unstaged
                && !patch.reviewed
                && patch.file.patch.contains("+valid name\n"))
    );
    assert_eq!(git(&main, &["ls-files"]), r#""note-\377.txt""#);
}

#[cfg(unix)]
#[test]
fn a_saved_comparison_keeps_distinct_committed_paths_with_the_same_lossy_spelling() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    git(&main, &["commit", "--allow-empty", "-m", "Initial"]);
    fs::write(
        main.join(OsStr::from_bytes(b"note-\xff.txt")),
        "invalid name\n",
    )
    .unwrap();
    fs::write(main.join("note-�.txt"), "valid name\n").unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-m", "Add both files"]);

    let app = Application::new(temp.path().join("reviews.sqlite3"));
    app.register_project(&main).unwrap();
    let comparison = app
        .compare_revisions(&main, "HEAD^", "HEAD", ComparisonMode::Between)
        .unwrap();
    let id = app.save_commit_review(&main, &comparison).unwrap();
    let saved = app.open_commit_review(&main, id).unwrap().comparison;
    assert_eq!(saved.files.len(), 2);
    for file in &saved.files {
        let mut request = serde_json::to_value(file).unwrap();
        request["kind"] = "comparison".into();
        request["start"] = saved.start.clone().into();
        request["end"] = saved.end.clone().into();
        let source = serde_json::from_value(request).unwrap();
        let details = app.file_details(&main, source).unwrap();
        let expected = if file.patch.contains("+invalid name\n") {
            13
        } else {
            11
        };
        assert_eq!(details.after.unwrap().byte_size, Some(expected));
    }
}

#[test]
fn oversized_file_patches_report_a_review_limit() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    git(&main, &["commit", "--allow-empty", "-m", "Initial"]);
    fs::write(main.join("large.txt"), "a normal text line
".repeat(60_000)).unwrap();
    let app = Application::new(temp.path().join("diffest.sqlite3"));
    let error = app.open_worktree_review(&main).err().unwrap();
    assert!(error.contains("review limit"), "{error}");

    git(&main, &["add", "large.txt"]);
    git(&main, &["commit", "-m", "Add large text file"]);
    let error = app
        .compare_revisions(&main, "HEAD^", "HEAD", ComparisonMode::Between)
        .err()
        .unwrap();
    assert!(error.contains("review limit"), "{error}");
}

#[test]
fn worktree_review_limits_the_combined_staged_and_untracked_diff() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    git(&main, &["commit", "--allow-empty", "-m", "Initial"]);
    let content = "ordinary text line
".repeat(46_000);
    for index in 0..10 {
        let path = format!("file-{index}.txt");
        fs::write(main.join(&path), &content).unwrap();
        if index < 5 {
            git(&main, &["add", &path]);
        }
    }
    let app = Application::new(temp.path().join("diffest.sqlite3"));
    let error = app.open_worktree_review(&main).err().unwrap();
    assert!(error.contains("diff is too large"), "{error}");
}

#[test]
fn selecting_a_file_skips_unrelated_oversized_worktree_patches() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    git(&main, &["commit", "--allow-empty", "-m", "Initial"]);
    fs::write(main.join("selected.txt"), "selected
").unwrap();
    let app = Application::new(temp.path().join("diffest.sqlite3"));
    let patches = app.open_worktree_review(&main).unwrap();
    fs::write(main.join("unrelated.txt"), "ordinary text line
".repeat(60_000)).unwrap();

    let details = app.file_details(&main, worktree_source(&patches[0])).unwrap();
    assert_eq!(details.after.unwrap().byte_size, Some(9));
}

#[test]
fn desktop_path_validation_requires_a_registered_worktree_or_project_root() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    git(&main, &["commit", "--allow-empty", "-m", "Initial"]);
    git(&main, &["worktree", "add", "-b", "topic", "../linked"]);
    let linked = temp.path().join("linked").canonicalize().unwrap();
    let app = Application::new(temp.path().join("diffest.sqlite3"));

    assert!(app.require_registered_checkout(&main, false).is_err());
    app.register_project(&linked).unwrap();
    assert_eq!(app.require_registered_checkout(&linked, false).unwrap(), linked);
    assert_eq!(app.require_registered_checkout(&main, true).unwrap(), main);
    assert!(app.require_registered_checkout(&linked, true).is_err());
    assert!(app.require_registered_checkout(temp.path(), true).is_err());
}

#[test]
fn selected_worktree_rename_keeps_both_file_versions() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    fs::write(main.join("original.txt"), "rename me
").unwrap();
    git(&main, &["add", "original.txt"]);
    git(&main, &["commit", "-m", "Initial"]);
    git(&main, &["mv", "original.txt", "renamed.txt"]);

    let app = Application::new(temp.path().join("diffest.sqlite3"));
    let patches = app.open_worktree_review(&main).unwrap();
    let details = app.file_details(&main, worktree_source(&patches[0])).unwrap();
    assert_eq!(details.before.unwrap().path, "original.txt");
    assert_eq!(details.after.unwrap().path, "renamed.txt");
}

#[test]
fn many_small_untracked_files_stop_worktree_review_before_diffing_each_file() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    git(&main, &["commit", "--allow-empty", "-m", "Initial"]);
    for index in 0..513 {
        fs::write(main.join(format!("untracked-{index}.txt")), "").unwrap();
    }
    let app = Application::new(temp.path().join("diffest.sqlite3"));
    let error = app.open_worktree_review(&main).err().unwrap();
    assert!(error.contains("Too many untracked or conflicted files"), "{error}");
}

#[test]
fn fetch_discovers_remote_targets_without_switching_the_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let upstream = init_project(temp.path());
    fs::write(upstream.join("notes.txt"), "initial\n").unwrap();
    git(&upstream, &["add", "notes.txt"]);
    git(&upstream, &["commit", "-m", "Initial"]);

    let checkout = temp.path().join("checkout");
    git(temp.path(), &["clone", "project", "checkout"]);
    let head = git(&checkout, &["rev-parse", "HEAD"]);
    fs::write(checkout.join("notes.txt"), "staged\n").unwrap();
    git(&checkout, &["add", "notes.txt"]);
    fs::write(checkout.join("notes.txt"), "working tree\n").unwrap();
    let index = git(&checkout, &["write-tree"]);

    git(&upstream, &["switch", "-c", "release"]);
    fs::write(upstream.join("notes.txt"), "release\n").unwrap();
    git(&upstream, &["commit", "-am", "Release updates"]);

    let app = Application::new(temp.path().join("reviews.sqlite3"));
    assert!(app.fetch_revisions(&checkout).is_err());
    app.register_project(&checkout).unwrap();
    assert!(
        app.open_worktree(&checkout)
            .unwrap()
            .remote_branches
            .is_empty()
    );
    let revisions = app.fetch_revisions(&checkout).unwrap();
    assert_eq!(revisions.branches, ["main"]);
    assert_eq!(revisions.remote_branches, ["origin/release"]);
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&checkout, &["write-tree"]), index);
    assert_eq!(
        fs::read_to_string(checkout.join("notes.txt")).unwrap(),
        "working tree\n"
    );

    let comparison = app
        .compare_revisions(
            &checkout,
            "HEAD",
            "refs/remotes/origin/release",
            ComparisonMode::Between,
        )
        .unwrap();
    assert_eq!(comparison.files[0].path, "notes.txt");

    let active = app
        .open_comparison(
            &checkout,
            "HEAD",
            "origin/release",
            ComparisonMode::Between,
            false,
        )
        .unwrap();
    let review = app
        .start_comparison_review(&checkout, &active.selection, &active.comparison, None, 0)
        .unwrap();
    git(&checkout, &["tag", "local-only"]);
    git(
        &checkout,
        &[
            "config",
            "--add",
            "remote.origin.fetch",
            "+refs/tags/*:refs/tags/*",
        ],
    );
    git(&upstream, &["switch", "main"]);
    git(&upstream, &["branch", "-D", "release"]);
    assert!(
        app.fetch_revisions(&checkout)
            .unwrap()
            .remote_branches
            .is_empty()
    );
    assert_eq!(git(&checkout, &["rev-parse", "local-only"]), head);
    let status = app
        .comparison_update(
            &checkout,
            &active.selection,
            &active.comparison.base,
            &active.comparison.end,
        )
        .unwrap();
    assert!(status == diffest_core::ComparisonUpdate::BranchDeleted);
    assert!(
        app.open_commit_review(&checkout, review.summary.id)
            .is_err()
    );
    assert!(active.comparison.files[0].patch.contains("+release"));
}

#[test]
fn branch_comparisons_resume_pinned_and_refresh_only_unchanged_marks() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = init_project(temp.path());
    fs::write(checkout.join("first.txt"), "base\n").unwrap();
    fs::write(checkout.join("second.txt"), "base\n").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "Base"]);
    git(&checkout, &["switch", "-c", "topic"]);
    fs::write(checkout.join("first.txt"), "reviewed first\n").unwrap();
    fs::write(checkout.join("second.txt"), "reviewed second\n").unwrap();
    git(&checkout, &["commit", "-am", "Topic"]);
    let app = Application::new(temp.path().join("reviews.sqlite3"));
    app.register_project(&checkout).unwrap();

    let open = |target, refresh| {
        app.open_comparison(
            &checkout,
            "main",
            target,
            ComparisonMode::Divergence,
            refresh,
        )
        .unwrap()
    };
    let initial = open("HEAD", false);
    let old_tip = initial.comparison.end.clone();
    let first = app
        .start_comparison_review(&checkout, &initial.selection, &initial.comparison, None, 0)
        .unwrap();
    app.mark_comparison_file(&checkout, first.summary.id, 1, true)
        .unwrap();
    app.leave_comparison_review().unwrap();
    fs::write(checkout.join("second.txt"), "needs another look\n").unwrap();
    git(&checkout, &["commit", "-am", "Change second file"]);

    let pinned = open("topic", false);
    assert_eq!(pinned.comparison.end, old_tip);
    assert_eq!(pinned.progress.unwrap().reviewed_files, [0, 1]);
    assert_eq!(open("topic", true).progress.unwrap().reviewed_files, [0]);
    assert!(open(&old_tip, false).progress.is_none());
}

#[test]
fn pinned_commit_progress_expires_with_project_limits_and_clears_on_completion() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = init_project(temp.path());
    fs::write(checkout.join("base.txt"), "base\n").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "Base"]);
    let base = git(&checkout, &["rev-parse", "HEAD"]);
    for file in ["one.txt", "two.txt"] {
        fs::write(checkout.join(file), "added\n").unwrap();
    }
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "More files"]);
    let tip = git(&checkout, &["rev-parse", "HEAD"]);

    let database = temp.path().join("reviews.sqlite3");
    let app = Application::new(database.clone());
    app.register_project(&checkout).unwrap();
    let open = || {
        app.open_comparison(&checkout, &base, &tip, ComparisonMode::Between, false)
            .unwrap()
    };
    let mark_first = || {
        let fresh = open();
        assert!(fresh.progress.is_none());
        app.start_comparison_review(&checkout, &fresh.selection, &fresh.comparison, None, 0)
            .unwrap()
            .summary
            .id
    };
    let age = |days: u32| {
        rusqlite::Connection::open(&database)
            .unwrap()
            .execute(
                "UPDATE comparison_selections SET last_viewed = unixepoch() - ?1 * 86400",
                [days],
            )
            .unwrap();
    };

    assert_eq!(app.review_retention(Some(&checkout)).unwrap().days, 7);
    app.save_review_retention(Some(&checkout), Some(14))
        .unwrap();
    mark_first();
    app.leave_comparison_review().unwrap();
    age(8);
    app.save_review_retention(None, Some(3)).unwrap();
    assert_eq!(open().progress.unwrap().reviewed_files, [0]);
    app.leave_comparison_review().unwrap();
    age(4);
    let inherited = app.save_review_retention(Some(&checkout), None).unwrap();
    assert_eq!(inherited.days, 3);
    assert_eq!(inherited.override_days, None);

    let id = mark_first();
    age(10);
    app.save_review_retention(None, Some(1)).unwrap();
    let completed = app
        .mark_comparison_file(&checkout, id, 1, true)
        .unwrap()
        .unwrap();
    assert_eq!(completed.reviewed_files, [0, 1]);
    app.leave_comparison_review().unwrap();

    let id = mark_first();
    assert!(
        app.mark_comparison_file(&checkout, id, 0, false)
            .unwrap()
            .is_none()
    );
    assert!(open().progress.is_none());
}
