use diffest_core::{
    Application, ComparisonMode,
    category_settings::{CategoryRule, CategorySettings},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn git(checkout: &Path, args: &[&str]) {
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
}

fn init_project(directory: &Path) -> PathBuf {
    let checkout = directory.join("project");
    fs::create_dir(&checkout).unwrap();
    git(&checkout, &["init", "-b", "main"]);
    git(&checkout, &["config", "user.name", "Diffest Tests"]);
    git(&checkout, &["config", "user.email", "tests@example.com"]);
    checkout.canonicalize().unwrap()
}

#[test]
fn changed_files_use_first_matching_glob_and_choose_the_rename_destination() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = init_project(temp.path());
    fs::write(checkout.join("old.md"), "Rename me\n").unwrap();
    fs::write(checkout.join("deleted.md"), "Delete me\n").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "Initial commit"]);

    git(&checkout, &["mv", "old.md", "new.rs"]);
    git(&checkout, &["rm", "deleted.md"]);
    fs::write(checkout.join("unknown.asset"), "").unwrap();
    git(&checkout, &["add", "."]);
    git(&checkout, &["commit", "-m", "Changed files"]);

    let app = Application::new(temp.path().join("diffest.sqlite3"));
    let project = app.register_project(&checkout).unwrap().project;
    let files = app
        .compare_revisions(&checkout, "HEAD^", "HEAD", ComparisonMode::Between)
        .unwrap()
        .files;
    let paths = files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let settings = CategorySettings::new(&temp.path().join("home"));
    let project_settings = project
        .main_checkout
        .as_deref()
        .unwrap()
        .join(".diffest/settings.json");
    fs::create_dir_all(project_settings.parent().unwrap()).unwrap();
    fs::write(
        &project_settings,
        r#"{"categoryRules":[{"glob":"new.rs","category":"Implementation","subcategory":"Core"},{"glob":"**/*.rs","category":"Rust"},{"glob":"**/*.md","category":"Docs"}]}"#,
    )
    .unwrap();
    let categories = settings
        .read(project.main_checkout.as_deref(), &paths)
        .categories;
    for (file, category) in files.iter().zip(&categories) {
        match file.status {
            'R' => {
                assert_eq!(category.category, "Implementation");
                assert_eq!(category.subcategory.as_deref(), Some("Core"));
            }
            'D' => assert_eq!(category.category, "Docs"),
            'A' => assert_eq!(category.category, "Other"),
            _ => panic!("Unexpected status {}", file.status),
        }
    }
    assert_eq!(categories.len(), 3);
}

#[test]
fn both_settings_resets_restore_inheritance_across_linked_worktrees() {
    let temp = tempfile::tempdir().unwrap();
    let main = init_project(temp.path());
    git(&main, &["commit", "--allow-empty", "-m", "Initial commit"]);
    git(&main, &["worktree", "add", "-b", "topic", "../linked"]);
    let linked = temp.path().join("linked");
    let app = Application::new(temp.path().join("diffest.sqlite3"));
    let project = app.register_project(&linked).unwrap().project;
    assert_eq!(project.main_checkout.as_deref(), Some(main.as_path()));

    let settings = CategorySettings::new(&temp.path().join("home"));
    let paths = [
        "src/lib.rs",
        "src/unit.test.ts",
        "src/docs/start.md",
        "dist/lib.rs",
        "data.csv",
    ]
    .map(str::to_owned);
    let read = || settings.read(project.main_checkout.as_deref(), &paths);
    let categories = |snapshot: diffest_core::category_settings::Categorization| {
        snapshot
            .categories
            .into_iter()
            .map(|file| file.category)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        categories(read()),
        ["Source", "Tests", "Docs", "Generated", "Other"]
    );
    let global = read().global;
    let project = read().project.unwrap();
    assert_eq!(project.path, main.join(".diffest/settings.json"));
    assert_eq!(global.path, temp.path().join("home/.diffest/settings.json"));
    settings
        .save(
            None,
            &global.version.unwrap(),
            Some(vec![CategoryRule {
                glob: "**/*.rs".into(),
                category: "Global Rust".into(),
                subcategory: None,
            }]),
        )
        .unwrap();
    assert_eq!(
        categories(read()),
        ["Global Rust", "Other", "Other", "Global Rust", "Other"]
    );

    fs::create_dir_all(project.path.parent().unwrap()).unwrap();
    fs::write(
        &project.path,
        r#"{"categoryRules":[],"defaultLayout":"split"}"#,
    )
    .unwrap();
    let customized_project = read().project.unwrap();
    assert!(customized_project.custom);
    assert_eq!(categories(read()), ["Other"; 5]);
    settings
        .save(Some(&main), &customized_project.version.unwrap(), None)
        .unwrap();
    assert_eq!(
        categories(read()),
        ["Global Rust", "Other", "Other", "Global Rust", "Other"]
    );
    assert!(!read().project.unwrap().custom);
    assert_eq!(
        fs::read_to_string(&project.path).unwrap().trim(),
        "{\n  \"defaultLayout\": \"split\"\n}"
    );
    assert!(!linked.join(".diffest/settings.json").exists());

    settings
        .save(None, &read().global.version.unwrap(), None)
        .unwrap();
    assert!(!read().global.custom);
    assert_eq!(
        categories(read()),
        ["Source", "Tests", "Docs", "Generated", "Other"]
    );
}

#[test]
fn manual_edits_reload_and_invalid_files_keep_the_last_valid_rules() {
    let temp = tempfile::tempdir().unwrap();
    let settings = CategorySettings::new(temp.path());
    let paths = ["src/lib.rs".into()];
    let read = || settings.read(None, &paths);
    let initial = read().global;
    fs::create_dir_all(initial.path.parent().unwrap()).unwrap();
    let manual = r#"{"categoryRules":[{"glob":"**/*.rs","category":"Manual"}]}"#;
    fs::write(&initial.path, manual).unwrap();
    let snapshot = read();
    assert_eq!(snapshot.categories[0].category, "Manual");
    assert!(snapshot.global.error.is_none());

    let error = settings
        .save(None, &initial.version.unwrap(), Some(vec![]))
        .unwrap_err();
    assert!(error.contains("changed on disk"));
    assert_eq!(fs::read_to_string(&initial.path).unwrap(), manual);

    for (invalid, expected) in [
        ("{", "EOF"),
        (
            r#"{"categoryRules":[{"glob":"[","category":"Broken"}]}"#,
            "Rule 1",
        ),
    ] {
        fs::write(&initial.path, invalid).unwrap();
        let snapshot = read();
        assert_eq!(snapshot.categories[0].category, "Manual");
        let error = snapshot.global.error.unwrap();
        assert!(error.contains(initial.path.to_str().unwrap()));
        assert!(error.contains(expected), "{error}");
    }

    fs::write(
        &initial.path,
        r#"{"categoryRules":[{"glob":"**/*.rs","category":"Updated"}]}"#,
    )
    .unwrap();
    assert_eq!(read().categories[0].category, "Updated");
    fs::remove_file(&initial.path).unwrap();
    assert_eq!(read().categories[0].category, "Source");
}

#[test]
fn global_layout_reloads_from_json_and_preserves_category_rules() {
    use diffest_core::DiffLayout;

    let temp = tempfile::tempdir().unwrap();
    let settings = CategorySettings::new(temp.path());
    let initial = settings.read(None, &[]);
    assert_eq!(initial.default_diff_layout, DiffLayout::Unified);
    let path = initial.global.path;
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        r#"{"categoryRules":[{"glob":"*.txt","category":"Text"}]}"#,
    )
    .unwrap();

    assert!(
        settings
            .save_default_diff_layout(
                initial.global.version.as_deref().unwrap(),
                DiffLayout::Split
            )
            .is_err()
    );
    let read = settings.read(None, &[]);
    settings
        .save_default_diff_layout(read.global.version.as_deref().unwrap(), DiffLayout::Split)
        .unwrap();

    let settings = CategorySettings::new(temp.path());
    let reopened = settings.read(None, &["change.txt".into()]);
    assert_eq!(reopened.default_diff_layout, DiffLayout::Split);
    assert_eq!(reopened.categories[0].category, "Text");
    settings
        .save(None, reopened.global.version.as_deref().unwrap(), None)
        .unwrap();
    assert_eq!(
        settings.read(None, &[]).default_diff_layout,
        DiffLayout::Split
    );

    fs::write(&path, r#"{"defaultDiffLayout":"invalid"}"#).unwrap();
    let invalid = settings.read(None, &[]);
    assert_eq!(invalid.default_diff_layout, DiffLayout::Split);
    assert!(invalid.global.error.unwrap().contains("defaultDiffLayout"));

    fs::write(&path, "{}").unwrap();
    assert_eq!(
        settings.read(None, &[]).default_diff_layout,
        DiffLayout::Unified
    );
}
