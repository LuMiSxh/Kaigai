use tauri::AppHandle;

use crate::models::{self, ModelInfo};

#[tauri::command]
#[specta::specta]
pub fn get_model_catalog(app: AppHandle) -> Result<Vec<ModelInfo>, String> {
    models::catalog(&app)
}

#[tauri::command]
#[specta::specta]
pub async fn install_model(app: AppHandle, model_id: String) -> Result<ModelInfo, String> {
    models::install(app, model_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn uninstall_model(app: AppHandle, model_id: String) -> Result<ModelInfo, String> {
    models::uninstall(app, model_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_core_ml_enabled(
    app: AppHandle,
    model_id: String,
    enabled: bool,
) -> Result<ModelInfo, String> {
    models::set_core_ml(app, model_id, enabled).await
}

#[tauri::command]
#[specta::specta]
pub fn cancel_model_download(app: AppHandle) -> Result<(), String> {
    models::cancel(&app)
}
