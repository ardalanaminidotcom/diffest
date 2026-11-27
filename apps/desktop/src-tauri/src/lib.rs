mod category_settings;

use diffest_core::category_settings::CategorySettings;
use diffest_core::{
    Application, CommitReviewProgress, Comparison, ComparisonMode, ComparisonSession,
    ComparisonUpdate, DiffLayout, FileDetails, FileDetailsSource, Project, Registration,
    ReviewRetention, ReviewSelection, Revisions, StageWorktreePatchResult, WorktreePatch,
};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
async fn list_projects(app: State<'_, Application>) -> Result<Vec<Project>, String> {
    app.clean_comparison_reviews()?;
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
    app.check_deleted_reviews(&checkout)?;
    app.open_worktree(&checkout)
}

#[tauri::command]
async fn fetch_revisions(handle: AppHandle, checkout: PathBuf) -> Result<Revisions, String> {
    tauri::async_runtime::spawn_blocking(move || {
        handle.state::<Application>().fetch_revisions(&checkout)
    })
    .await
    .map_err(|error| format!("Cannot run fetch: {error}"))?
}

#[tauri::command]
async fn compare_revisions(
    app: State<'_, Application>,
    checkout: PathBuf,
    base: String,
    target: String,
    mode: ComparisonMode,
    refresh: Option<bool>,
) -> Result<ComparisonSession, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.open_comparison(&checkout, &base, &target, mode, refresh.unwrap_or(false))
}

#[tauri::command]
async fn start_commit_review(
    app: State<'_, Application>,
    checkout: PathBuf,
    selection: ReviewSelection,
    comparison: Comparison,
    layout: Option<DiffLayout>,
    file_index: usize,
) -> Result<CommitReviewProgress, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.start_comparison_review(&checkout, &selection, &comparison, layout, file_index)
}

#[tauri::command]
async fn comparison_update(
    app: State<'_, Application>,
    checkout: PathBuf,
    selection: ReviewSelection,
    base: String,
    end: String,
) -> Result<ComparisonUpdate, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.comparison_update(&checkout, &selection, &base, &end)
}

#[tauri::command]
async fn leave_comparison_review(app: State<'_, Application>) -> Result<(), String> {
    app.leave_comparison_review()
}

#[tauri::command]
async fn set_commit_file_reviewed(
    app: State<'_, Application>,
    checkout: PathBuf,
    id: i64,
    file_index: usize,
    reviewed: bool,
) -> Result<Option<CommitReviewProgress>, String> {
    let checkout = app.require_registered_checkout(&checkout, false)?;
    app.mark_comparison_file(&checkout, id, file_index, reviewed)
}

#[tauri::command]
async fn read_review_retention(
    app: State<'_, Application>,
    main_checkout: Option<PathBuf>,
) -> Result<ReviewRetention, String> {
    let checkout = main_checkout
        .as_deref()
        .map(|path| app.require_registered_checkout(path, true))
        .transpose()?;
    app.review_retention(checkout.as_deref())
}

#[tauri::command]
async fn save_review_retention(
    app: State<'_, Application>,
    main_checkout: Option<PathBuf>,
    days: Option<u32>,
) -> Result<ReviewRetention, String> {
    let checkout = main_checkout
        .as_deref()
        .map(|path| app.require_registered_checkout(path, true))
        .transpose()?;
    app.save_review_retention(checkout.as_deref(), days)
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
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                let _ = window.state::<Application>().leave_comparison_review();
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_projects,
            register_project,
            open_worktree,
            fetch_revisions,
            compare_revisions,
            start_commit_review,
            comparison_update,
            leave_comparison_review,
            read_review_retention,
            save_review_retention,
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
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|handle, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let _ = handle.state::<Application>().leave_comparison_review();
            }
        });
}
