use diffest_core::{Application, PatchSide};
use std::{fs, path::Path, process::Command};

fn git(checkout: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(checkout)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn init(checkout: &Path) {
    fs::create_dir(checkout).unwrap();
    git(checkout, &["init", "-b", "main"]);
    git(checkout, &["config", "user.name", "Diffest Tests"]);
    git(checkout, &["config", "user.email", "tests@example.com"]);
}

#[test]
fn staging_a_reviewed_addition_preserves_its_mark_and_unrelated_index_entries() {
    let temp = tempfile::tempdir().unwrap();
    let main = temp.path().join("main");
    init(&main);
    fs::write(main.join("unrelated.txt"), "committed\n").unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-m", "Initial"]);
    git(&main, &["worktree", "add", "-b", "topic", "../linked"]);
    let checkout = temp.path().join("linked");
    fs::write(checkout.join("unrelated.txt"), "already staged\n").unwrap();
    git(&checkout, &["add", "unrelated.txt"]);
    fs::write(checkout.join("unrelated.txt"), "worktree edit\n").unwrap();
    fs::write(checkout.join("[literal].txt"), "reviewed addition\n").unwrap();
    fs::write(checkout.join("l.txt"), "another addition\n").unwrap();

    let app = Application::new(temp.path().join("review.sqlite3"));
    let patches = app.open_worktree_review(&checkout).unwrap();
    let added = patches
        .iter()
        .find(|patch| patch.file.path == "[literal].txt")
        .unwrap();
    let before = git(&checkout, &["ls-files", "--stage"]);
    let unreviewed = app
        .stage_worktree_patch(&checkout, &added.fingerprint)
        .unwrap();
    assert!(unreviewed.error.is_some());
    app.set_worktree_patch_reviewed(&checkout, &added.fingerprint, true)
        .unwrap();
    assert_eq!(git(&checkout, &["ls-files", "--stage"]), before);

    let result = app
        .stage_worktree_patch(&checkout, &added.fingerprint)
        .unwrap();
    assert_eq!(result.error, None);
    assert_eq!(
        git(&checkout, &["show", ":[literal].txt"]),
        "reviewed addition\n"
    );
    assert_eq!(
        git(&checkout, &["show", ":unrelated.txt"]),
        "already staged\n"
    );
    assert_eq!(git(&checkout, &["ls-files", "l.txt"]), "");
    assert_eq!(git(&main, &["show", ":unrelated.txt"]), "committed\n");
    let added: Vec<_> = result
        .patches
        .iter()
        .filter(|patch| patch.file.path == "[literal].txt")
        .collect();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].side, PatchSide::Staged);
    assert!(added[0].reviewed);
    assert!(
        app.open_worktree_review(&checkout)
            .unwrap()
            .iter()
            .any(|patch| {
                patch.file.path == "[literal].txt"
                    && patch.side == PatchSide::Staged
                    && patch.reviewed
            })
    );
}

fn mark_unstaged(app: &Application, checkout: &Path, path: &str) -> String {
    let patch = app
        .open_worktree_review(checkout)
        .unwrap()
        .into_iter()
        .find(|patch| patch.side == PatchSide::Unstaged && patch.file.path == path)
        .unwrap();
    app.set_worktree_patch_reviewed(checkout, &patch.fingerprint, true)
        .unwrap();
    patch.fingerprint
}

#[test]
fn stale_reviews_refresh_and_the_combined_staged_patch_requires_review() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = temp.path().join("project");
    init(&checkout);
    let file = checkout.join("partial.txt");
    fs::write(&file, "committed\n").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "Initial"]);
    fs::write(&file, "staged\n").unwrap();
    git(&checkout, &["add", "partial.txt"]);
    fs::write(&file, "reviewed\n").unwrap();

    let app = Application::new(temp.path().join("review.sqlite3"));
    let staged = app.open_worktree_review(&checkout).unwrap();
    app.set_worktree_patch_reviewed(&checkout, &staged[0].fingerprint, true)
        .unwrap();
    let reviewed = mark_unstaged(&app, &checkout, "partial.txt");
    fs::write(&file, "edited\n").unwrap();

    let stale = app.stage_worktree_patch(&checkout, &reviewed).unwrap();
    assert!(stale.error.is_some());
    assert_eq!(git(&checkout, &["show", ":partial.txt"]), "staged\n");
    assert!(stale.patches[0].reviewed);
    assert!(!stale.patches[1].reviewed);
    assert_ne!(stale.patches[1].fingerprint, reviewed);
    assert!(stale.patches[1].file.patch.contains("+edited\n"));

    let reviewed = mark_unstaged(&app, &checkout, "partial.txt");
    let result = app.stage_worktree_patch(&checkout, &reviewed).unwrap();
    assert_eq!(result.error, None);
    assert_eq!(git(&checkout, &["show", ":partial.txt"]), "edited\n");
    assert_eq!(result.patches.len(), 1);
    let staged = &result.patches[0];
    assert_eq!(staged.side, PatchSide::Staged);
    assert!(staged.file.patch.contains("-committed\n+edited\n"));
    assert!(!staged.reviewed);
}

#[test]
fn a_reviewed_conflict_cannot_be_staged_and_its_index_stages_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = temp.path().join("project");
    init(&checkout);
    let file = checkout.join("conflict.txt");
    fs::write(&file, "base\n").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "Initial"]);
    git(&checkout, &["switch", "-c", "topic"]);
    fs::write(&file, "topic\n").unwrap();
    git(&checkout, &["commit", "-am", "Topic"]);
    git(&checkout, &["switch", "main"]);
    fs::write(&file, "main\n").unwrap();
    git(&checkout, &["commit", "-am", "Main"]);
    let merge = Command::new("git")
        .current_dir(&checkout)
        .args(["merge", "topic"])
        .output()
        .unwrap();
    assert!(!merge.status.success());
    let conflict = git(&checkout, &["ls-files", "--unmerged"]);
    assert_eq!(conflict.lines().count(), 3);
    fs::write(checkout.join("addition.bin"), b"\0new").unwrap();

    let app = Application::new(temp.path().join("review.sqlite3"));
    let reviewed = mark_unstaged(&app, &checkout, "conflict.txt");
    let refused = app.stage_worktree_patch(&checkout, &reviewed).unwrap();
    assert!(refused.error.is_some());
    assert!(
        refused
            .patches
            .iter()
            .any(|patch| { patch.file.status == 'U' && patch.reviewed })
    );
    assert_eq!(git(&checkout, &["ls-files", "--unmerged"]), conflict);

    let reviewed = mark_unstaged(&app, &checkout, "addition.bin");
    let result = app.stage_worktree_patch(&checkout, &reviewed).unwrap();
    assert_eq!(result.error, None);
    assert_eq!(git(&checkout, &["ls-files", "--unmerged"]), conflict);
    assert_eq!(git(&checkout, &["show", ":addition.bin"]), "\0new");
}

#[test]
fn a_worktree_edit_during_staging_cannot_enter_the_index() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = temp.path().join("project");
    init(&checkout);
    let file = checkout.join("reviewed.txt");
    fs::write(&file, "committed\n").unwrap();
    fs::write(checkout.join("unrelated.txt"), "committed\n").unwrap();
    fs::write(
        checkout.join(".gitattributes"),
        "reviewed.txt filter=race\n",
    )
    .unwrap();
    // Run the edit while Git reads the reviewed file for staging.
    fs::write(
        checkout.join(".git/race-filter"),
        r#"if [ -e .git/arm-race ] && [ -n "$GIT_INDEX_FILE" ]; then
    rm .git/arm-race
    printf 'concurrent edit\n' > reviewed.txt
    (unset GIT_INDEX_FILE; git add unrelated.txt) > .git/concurrent-add-log 2>&1
    cat >/dev/null
    cat reviewed.txt
else
    cat
fi
"#,
    )
    .unwrap();
    git(
        &checkout,
        &["config", "filter.race.clean", "sh .git/race-filter"],
    );
    git(&checkout, &["config", "filter.race.required", "true"]);
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "Initial"]);
    fs::write(checkout.join("unrelated.txt"), "already staged\n").unwrap();
    git(&checkout, &["add", "unrelated.txt"]);
    fs::write(checkout.join("unrelated.txt"), "another edit\n").unwrap();
    fs::write(&file, "reviewed\n").unwrap();

    let app = Application::new(temp.path().join("review.sqlite3"));
    let reviewed = mark_unstaged(&app, &checkout, "reviewed.txt");
    let index = git(&checkout, &["ls-files", "--stage"]);
    fs::write(checkout.join(".git/arm-race"), "").unwrap();

    let result = app.stage_worktree_patch(&checkout, &reviewed).unwrap();
    assert!(result.error.is_some());
    assert_eq!(fs::read_to_string(&file).unwrap(), "concurrent edit\n");
    assert!(
        fs::read_to_string(checkout.join(".git/concurrent-add-log"))
            .unwrap()
            .contains("index.lock")
    );
    assert_eq!(git(&checkout, &["ls-files", "--stage"]), index);
    assert!(result.patches.iter().any(|patch| {
        patch.file.path == "reviewed.txt"
            && patch.side == PatchSide::Unstaged
            && patch.file.patch.contains("+concurrent edit\n")
            && !patch.reviewed
    }));
}
