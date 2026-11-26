use diffest_core::{Application, ComparisonMode};
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
