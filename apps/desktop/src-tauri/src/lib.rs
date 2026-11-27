mod category_settings;

use diffest_core::category_settings::CategorySettings;
use diffest_core::{
    Application, CommitReview, CommitReviewProgress, Comparison, ComparisonMode, ComparisonSession,
    DiffLayout, FileDetails, FileDetailsSource, Project, Registration, Revisions,
    StageWorktreePatchResult, WorktreePatch,
};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
async fn list_projects(app: State<'_, Application>) -> Result<Vec<Project>, String> {
    app.list_projects()
}

#[tauri::command]
async fn register_project(
    handle: AppHandle,
    app: State<'_, Application>,
) -> Result<Option<Registration>, String> {
    let Some(folder) = handle
        .dialog()
        .file()
        .set_title("Select a Git checkout")
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let checkout = folder
        .into_path()
        .map_err(|error| format!("Cannot open checkout: {error}"))?
        .canonicalize()
        .map_err(|error| format!("Cannot open checkout: {error}"))?;
    app.register_project(&checkout).map(Some)
}

#[tauri::command]
async fn open_worktree(
    app: State<'_, Application>,
    checkout: PathBuf,
) -> Result<Revisions, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.open_worktree(&checkout)
}

#[tauri::command]
async fn compare_revisions(
    app: State<'_, Application>,
    checkout: PathBuf,
    base: String,
    target: String,
    mode: ComparisonMode,
) -> Result<ComparisonSession, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    let comparison = app.compare_revisions(&checkout, &base, &target, mode)?;
    let progress = app.find_commit_review(&checkout, &comparison)?;
    Ok(ComparisonSession {
        comparison,
        progress,
    })
}

#[tauri::command]
async fn start_commit_review(
    app: State<'_, Application>,
    checkout: PathBuf,
    comparison: Comparison,
    layout: Option<DiffLayout>,
    file_index: usize,
) -> Result<CommitReviewProgress, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.start_commit_review(&checkout, &comparison, layout, file_index)
}

#[tauri::command]
async fn open_commit_review(
    app: State<'_, Application>,
    checkout: PathBuf,
    id: i64,
) -> Result<CommitReview, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.open_commit_review(&checkout, id)
}

#[tauri::command]
async fn set_commit_file_reviewed(
    app: State<'_, Application>,
    checkout: PathBuf,
    id: i64,
    file_index: usize,
    reviewed: bool,
) -> Result<CommitReviewProgress, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.update_commit_file_review(&checkout, id, file_index, reviewed)
}

#[tauri::command]
async fn open_worktree_review(
    app: State<'_, Application>,
    checkout: PathBuf,
) -> Result<Vec<WorktreePatch>, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.open_worktree_review(&checkout)
}

#[tauri::command]
async fn set_worktree_patch_reviewed(
    app: State<'_, Application>,
    checkout: PathBuf,
    fingerprint: String,
    reviewed: bool,
) -> Result<Vec<WorktreePatch>, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.set_worktree_patch_reviewed(&checkout, &fingerprint, reviewed)
}

#[tauri::command]
async fn stage_worktree_patch(
    app: State<'_, Application>,
    checkout: PathBuf,
    fingerprint: String,
) -> Result<StageWorktreePatchResult, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.stage_worktree_patch(&checkout, &fingerprint)
}

#[tauri::command]
async fn read_review_layout(
    app: State<'_, Application>,
    checkout: PathBuf,
    id: Option<i64>,
) -> Result<Option<DiffLayout>, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.read_review_layout(&checkout, id)
}

#[tauri::command]
async fn set_review_layout(
    app: State<'_, Application>,
    checkout: PathBuf,
    id: Option<i64>,
    layout: DiffLayout,
) -> Result<(), String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.set_review_layout(&checkout, id, layout)
}

#[tauri::command]
async fn read_file_details(
    app: State<'_, Application>,
    checkout: PathBuf,
    source: FileDetailsSource,
) -> Result<FileDetails, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.file_details(&checkout, source)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let database = app.path().app_data_dir()?.join("diffest.sqlite3");
            app.manage(Application::new(database));
            app.manage(CategorySettings::new(&app.path().home_dir()?));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_projects,
            register_project,
            open_worktree,
            compare_revisions,
            start_commit_review,
            open_commit_review,
            set_commit_file_reviewed,
            open_worktree_review,
            set_worktree_patch_reviewed,
            stage_worktree_patch,
            read_review_layout,
            set_review_layout,
            read_file_details,
            category_settings::read_category_settings,
            category_settings::save_category_settings,
            category_settings::save_default_diff_layout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
