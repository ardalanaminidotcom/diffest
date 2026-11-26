use diffest_core::{
    Application, CommitReview, Comparison, ComparisonMode, Project, Registration, Revisions,
};
use std::path::PathBuf;
use tauri::{Manager, State};

#[tauri::command]
async fn list_projects(app: State<'_, Application>) -> Result<Vec<Project>, String> {
    app.list_projects()
}

#[tauri::command]
async fn register_project(
    app: State<'_, Application>,
    checkout: PathBuf,
) -> Result<Registration, String> {
    app.register_project(&checkout)
}

#[tauri::command]
async fn open_worktree(
    app: State<'_, Application>,
    checkout: PathBuf,
) -> Result<Revisions, String> {
    app.open_worktree(&checkout)
}

#[tauri::command]
async fn compare_revisions(
    app: State<'_, Application>,
    checkout: PathBuf,
    base: String,
    target: String,
    mode: ComparisonMode,
) -> Result<Comparison, String> {
    app.compare_revisions(&checkout, &base, &target, mode)
}

#[tauri::command]
async fn save_commit_review(
    app: State<'_, Application>,
    checkout: PathBuf,
    comparison: Comparison,
) -> Result<i64, String> {
    app.save_commit_review(&checkout, &comparison)
}

#[tauri::command]
async fn open_commit_review(
    app: State<'_, Application>,
    checkout: PathBuf,
    id: i64,
) -> Result<CommitReview, String> {
    app.open_commit_review(&checkout, id)
}

#[tauri::command]
async fn set_commit_file_reviewed(
    app: State<'_, Application>,
    checkout: PathBuf,
    id: i64,
    file_index: usize,
    reviewed: bool,
) -> Result<Vec<usize>, String> {
    app.set_commit_file_reviewed(&checkout, id, file_index, reviewed)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let database = app.path().app_data_dir()?.join("diffest.sqlite3");
            app.manage(Application::new(database));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_projects,
            register_project,
            open_worktree,
            compare_revisions,
            save_commit_review,
            open_commit_review,
            set_commit_file_reviewed
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
