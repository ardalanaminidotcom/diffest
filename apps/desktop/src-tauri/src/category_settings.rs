use diffest_core::{Application, DiffLayout};
use diffest_core::category_settings::{Categorization, CategoryRule, CategorySettings};
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub async fn read_category_settings(
    app: State<'_, Application>,
    settings: State<'_, CategorySettings>,
    main_checkout: Option<PathBuf>,
    paths: Vec<String>,
) -> Result<Categorization, String> {
    let main_checkout = main_checkout
        .as_deref()
        .map(|path| app.require_registered_checkout(path, true))
        .transpose()?;
    Ok(settings.read(main_checkout.as_deref(), &paths))
}

#[tauri::command]
pub async fn save_category_settings(
    app: State<'_, Application>,
    settings: State<'_, CategorySettings>,
    main_checkout: Option<PathBuf>,
    version: String,
    rules: Option<Vec<CategoryRule>>,
) -> Result<(), String> {
    let main_checkout = main_checkout
        .as_deref()
        .map(|path| app.require_registered_checkout(path, true))
        .transpose()?;
    settings.save(main_checkout.as_deref(), &version, rules)
}

#[tauri::command]
pub async fn save_default_diff_layout(
    settings: State<'_, CategorySettings>,
    version: String,
    layout: DiffLayout,
) -> Result<(), String> {
    settings.save_default_diff_layout(&version, layout)
}
