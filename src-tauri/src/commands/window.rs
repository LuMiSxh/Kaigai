use tauri::{AppHandle, Manager};

use super::WINDOW_LABELS;
use crate::{
    settings::AppSettings,
    state::{AppState, SessionStatus},
    tracing_setup,
};

use super::session::shutdown;

#[tauri::command]
#[specta::specta]
pub fn show_window(app: AppHandle, label: String) -> Result<(), String> {
    if !WINDOW_LABELS.contains(&label.as_str()) {
        return Err("unknown window".into());
    }
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("window not found: {label}"))?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn hide_window(app: AppHandle, label: String) -> Result<(), String> {
    if !WINDOW_LABELS.contains(&label.as_str()) {
        return Err("unknown window".into());
    }
    app.get_webview_window(&label)
        .ok_or_else(|| format!("window not found: {label}"))?
        .hide()
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn quit_app(app: AppHandle) {
    shutdown(&app);
}

#[tauri::command]
#[specta::specta]
pub fn get_recent_logs(after_id: Option<u64>) -> Vec<tracing_setup::DeveloperLogEntry> {
    tracing_setup::recent(after_id)
}

/// Deletes all downloaded models and managed yt-dlp, resets settings to
/// defaults, and opens the onboarding window so the user can start fresh.
#[tauri::command]
#[specta::specta]
pub async fn reset_app(app: AppHandle) -> Result<(), String> {
    use tokio::fs;

    // Refuse while a session is running — the pipeline holds model state.
    {
        let state = app.state::<AppState>();
        let session = state.session.lock().map_err(|_| "session lock poisoned")?;
        if !matches!(session.status, SessionStatus::Idle | SessionStatus::Failed) {
            return Err("stop the active session before resetting".into());
        }
    }

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let _ = fs::remove_dir_all(data_dir.join("models")).await;
    let _ = fs::remove_dir_all(data_dir.join("tools")).await;

    let fresh = AppSettings::default();
    crate::settings::save(&app, &fresh).map_err(|error| error.clone())?;
    *app.state::<AppState>()
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned")? = fresh;

    crate::app::show_and_focus(&app, "onboarding");
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.hide();
    }
    Ok(())
}
