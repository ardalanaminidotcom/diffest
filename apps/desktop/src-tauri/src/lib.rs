mod category_settings;

use diffest_core::category_settings::CategorySettings;
use diffest_core::{Application, Comparison, ComparisonMode, Project, Registration, Revisions};
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
            category_settings::read_category_settings,
            category_settings::save_category_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
