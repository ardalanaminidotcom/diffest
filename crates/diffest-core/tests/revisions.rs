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
            .file_details(
                &main,
                FileDetailsSource::Worktree {
                    fingerprint: patch.fingerprint.clone(),
                },
            )
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
        app.file_details(
            &main,
            FileDetailsSource::Worktree {
                fingerprint: reviewed.fingerprint.clone()
            }
        )
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
            .file_details(
                &main,
                FileDetailsSource::Worktree {
                    fingerprint: patch.fingerprint,
                },
            )
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
    fs::write(main.join("conflict.txt"), "base\n").unwrap();
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
    assert_eq!(registered.project.main_checkout, main);
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

    let registration = app.register_project(&main).unwrap();
    assert_eq!(registration.project.worktrees[0].path, main);
    let sibling = app.register_project(&linked).unwrap();
    assert_eq!(sibling.project.id, registration.project.id);
    assert_eq!(sibling.project.worktrees[0].path, main);
    assert_eq!(sibling.project.worktrees[1].path, linked);
    let reopened = Application::new(database).list_projects().unwrap();
    assert_eq!(reopened[0].worktrees[0].path, main);
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
    let projects = app.list_projects().unwrap();
    assert_eq!(projects[0].saved_reviews.len(), 1);
    let summary = &projects[0].saved_reviews[0];
    assert_eq!(summary.id, saved);
    assert_eq!(summary.base_label, "main");
    assert_eq!(summary.target_label, "topic");
    assert_eq!(summary.file_count, 2);
    assert_eq!(summary.reviewed_count, 1);

    let reopened = app.open_commit_review(&linked, saved).unwrap();
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

    assert_eq!(
        app.set_commit_file_reviewed(&linked, saved, 1, true)
            .unwrap(),
        [0, 1]
    );
    let projects = app.list_projects().unwrap();
    let completed = &projects[0].saved_reviews[0];
    assert_eq!(completed.reviewed_count, completed.file_count);

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
        app.list_projects().unwrap()[0].saved_reviews[0].reviewed_count,
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
