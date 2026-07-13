// `tauri::ipc::command::CommandArg` is only implemented for owned `AppHandle`/
// `State<T>`, and IPC payloads decode straight into owned values — every
// `#[tauri::command]` handler below necessarily takes its arguments by value.
#![allow(clippy::needless_pass_by_value)]

use tauri::{AppHandle, Manager, State, ipc::Channel};
use tauri_specta::Event;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    events::{SessionStateEvent, SettingsUpdatedEvent},
    settings::{self, AppSettings},
    state::{AppFeed, AppState, SessionStatus},
};

mod model;
mod session;
mod tools;
mod window;

pub use model::{
    cancel_model_download, get_model_catalog, install_model, set_core_ml_enabled, uninstall_model,
};
pub use session::{shutdown, start_session, stop_session};
pub use tools::{
    cancel_tool_download, check_yt_dlp_update, get_tool_statuses, install_yt_dlp,
    system_yt_dlp_available,
};
pub use window::{get_recent_logs, hide_window, quit_app, reset_app, show_window};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub session_state: SessionStatus,
    pub stream_url: Option<String>,
    pub settings: AppSettings,
}

#[tauri::command]
#[specta::specta]
pub fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    snapshot(&state)
}

#[tauri::command]
#[specta::specta]
pub fn connect_feed(feed: Channel<AppFeed>, state: State<'_, AppState>) {
    if let Ok(mut slot) = state.feed.lock() {
        *slot = Some(feed);
    }
}

#[tauri::command]
#[specta::specta]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    settings::save(&app, &settings)?;
    apply_overlay_settings(&app, &settings)?;
    *state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned")? = settings.clone();

    tracing::info!(
        model = %settings.model,
        chunk_mode = %settings.chunk_mode,
        cookie_mode = %settings.cookie_mode,
        "settings updated"
    );
    SettingsUpdatedEvent {
        settings: settings.clone(),
    }
    .emit(&app)
    .map_err(|error| error.to_string())?;
    state.send_feed(AppFeed::Settings {
        settings: Box::new(settings.clone()),
    });
    Ok(settings)
}

#[cfg(debug_assertions)]
pub(super) const WINDOW_LABELS: [&str; 4] = ["main", "settings", "developer", "onboarding"];
#[cfg(not(debug_assertions))]
pub(super) const WINDOW_LABELS: [&str; 3] = ["main", "settings", "onboarding"];

pub fn apply_overlay_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    // Click-through is now driven per-mode by the bar itself, so only
    // always-on-top is applied here.
    if let Some(overlay) = app.get_webview_window("main") {
        overlay
            .set_always_on_top(settings.always_on_top)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn snapshot(state: &State<'_, AppState>) -> Result<AppSnapshot, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned")?
        .clone();
    let session = state.session.lock().map_err(|_| "session lock poisoned")?;
    Ok(AppSnapshot {
        session_state: session.status,
        stream_url: session.stream_url.clone(),
        settings,
    })
}

pub(super) fn emit_state(app: &AppHandle, state: SessionStatus) -> Result<(), String> {
    SessionStateEvent { state }
        .emit(app)
        .map_err(|error| error.to_string())
}
