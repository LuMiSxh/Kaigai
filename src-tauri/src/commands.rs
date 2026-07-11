// `tauri::ipc::command::CommandArg` is only implemented for owned `AppHandle`/
// `State<T>`, and IPC payloads decode straight into owned values — every
// `#[tauri::command]` handler below necessarily takes its arguments by value.
#![allow(clippy::needless_pass_by_value)]

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State, ipc::Channel};
use tauri_specta::Event;

use crate::{
    events::{
        DiagnosticsEvent, SessionErrorEvent, SessionStateEvent, SettingsUpdatedEvent,
        SubtitleClearEvent,
    },
    models::{self, ModelInfo},
    pipeline,
    settings::{self, AppSettings},
    state::{AppFeed, AppState, SessionStatus},
    tools::{self, Tool, ToolStatus},
    tracing_setup,
};

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
pub async fn start_session(
    stream_url: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppSnapshot, String> {
    let stream_url = stream_url.trim().to_owned();
    if stream_url.is_empty() {
        return Err("stream URL cannot be empty".into());
    }

    let settings = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned")?
        .clone();
    tracing::info!(
        cookie_mode = %settings.cookie_mode,
        model = %settings.model,
        "starting stream session"
    );

    let cancel = Arc::new(AtomicBool::new(false));
    let generation = {
        let mut session = state.session.lock().map_err(|_| "session lock poisoned")?;
        if !matches!(session.status, SessionStatus::Idle | SessionStatus::Failed) {
            return Err("a stream session is already active".into());
        }
        session.generation = session.generation.wrapping_add(1);
        session.status = SessionStatus::Starting;
        session.stream_url = Some(stream_url.clone());
        session.cancel = Some(cancel.clone());
        session.generation
    };
    state.send_feed(AppFeed::State {
        state: SessionStatus::Starting,
    });
    emit_state(&app, SessionStatus::Starting)?;

    let worker = std::thread::spawn({
        let app = app.clone();
        move || {
            let result = pipeline::run(app.clone(), stream_url, settings, cancel.clone());
            finish_session(&app, generation, &cancel, result);
        }
    });
    state
        .session
        .lock()
        .map_err(|_| "session lock poisoned")?
        .worker = Some(worker);
    snapshot(&state)
}

#[tauri::command]
#[specta::specta]
pub fn connect_feed(feed: Channel<AppFeed>, state: State<'_, AppState>) {
    if let Ok(mut slot) = state.feed.lock() {
        *slot = Some(feed);
    }
}

/// Stop any active session and join its worker so Whisper's Metal context is
/// dropped before the process exits (otherwise GGML aborts in its destructor).
pub fn shutdown(app: &AppHandle) {
    let worker = {
        let managed = app.state::<AppState>();
        let Ok(mut session) = managed.session.lock() else {
            app.exit(0);
            return;
        };
        if let Some(cancel) = &session.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        session.worker.take()
    };
    if let Some(worker) = worker {
        let _ = worker.join();
    }
    app.exit(0);
}

#[tauri::command]
#[specta::specta]
pub fn stop_session(app: AppHandle, state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let should_stop = {
        let mut session = state.session.lock().map_err(|_| "session lock poisoned")?;
        if matches!(session.status, SessionStatus::Idle) {
            false
        } else {
            session.status = SessionStatus::Stopping;
            if let Some(cancel) = &session.cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            true
        }
    };
    if should_stop {
        tracing::info!("stopping stream session");
        emit_state(&app, SessionStatus::Stopping)?;
    }
    snapshot(&state)
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
const WINDOW_LABELS: [&str; 4] = ["main", "settings", "developer", "onboarding"];
#[cfg(not(debug_assertions))]
const WINDOW_LABELS: [&str; 3] = ["main", "settings", "onboarding"];

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
pub fn quit_app(app: AppHandle) {
    shutdown(&app);
}

#[tauri::command]
#[specta::specta]
pub fn get_recent_logs(after_id: Option<u64>) -> Vec<tracing_setup::DeveloperLogEntry> {
    tracing_setup::recent(after_id)
}

#[tauri::command]
#[specta::specta]
pub async fn get_tool_statuses(app: AppHandle) -> Vec<ToolStatus> {
    // `tools::status` spawns `--version` subprocesses; run them off the main
    // thread so probing tool versions never stalls the UI.
    tokio::task::spawn_blocking(move || {
        vec![
            tools::status(&app, Tool::YtDlp),
            tools::status(&app, Tool::Ffmpeg),
            tools::status(&app, Tool::QuickJs),
        ]
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub fn system_yt_dlp_available() -> bool {
    tools::system_yt_dlp_available()
}

/// Returns the latest published yt-dlp version when it differs from what's
/// currently resolved, or `None` when already current.
#[tauri::command]
#[specta::specta]
pub async fn check_yt_dlp_update(app: AppHandle) -> Result<Option<String>, String> {
    let current = tools::status(&app, Tool::YtDlp).version;
    let latest = tools::latest_yt_dlp_version().await?;
    Ok((current.as_deref() != Some(latest.as_str())).then_some(latest))
}

#[tauri::command]
#[specta::specta]
pub async fn install_yt_dlp(app: AppHandle) -> Result<ToolStatus, String> {
    tools::install_yt_dlp(app).await
}

#[tauri::command]
#[specta::specta]
pub fn cancel_tool_download(app: AppHandle) -> Result<(), String> {
    tools::cancel_tool_download(&app)
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
    settings::save(&app, &fresh).map_err(|error| error.clone())?;
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

fn emit_state(app: &AppHandle, state: SessionStatus) -> Result<(), String> {
    SessionStateEvent { state }
        .emit(app)
        .map_err(|error| error.to_string())
}

fn finish_session(
    app: &AppHandle,
    generation: u64,
    cancel: &AtomicBool,
    result: Result<(), String>,
) {
    let managed = app.state::<AppState>();
    let Ok(mut session) = managed.session.lock() else {
        return;
    };
    if session.generation != generation {
        return;
    }

    session.cancel = None;
    if cancel.load(Ordering::Relaxed) {
        tracing::info!("stream session stopped");
        session.status = SessionStatus::Idle;
        session.stream_url = None;
        managed.send_feed(AppFeed::Clear);
        managed.send_feed(AppFeed::State {
            state: SessionStatus::Idle,
        });
        let _ = SubtitleClearEvent.emit(app);
        let _ = emit_state(app, SessionStatus::Idle);
    } else if let Err(error) = result {
        tracing::error!(error = %error, "stream session failed");
        session.status = SessionStatus::Failed;
        managed.send_feed(AppFeed::Error {
            message: error.clone(),
        });
        managed.send_feed(AppFeed::State {
            state: SessionStatus::Failed,
        });
        let _ = DiagnosticsEvent {
            message: format!("[error] {error}"),
        }
        .emit(app);
        let _ = SessionErrorEvent { message: error }.emit(app);
        let _ = emit_state(app, SessionStatus::Failed);
    } else {
        tracing::info!("stream session ended");
        session.status = SessionStatus::Idle;
        session.stream_url = None;
        managed.send_feed(AppFeed::State {
            state: SessionStatus::Idle,
        });
        let _ = emit_state(app, SessionStatus::Idle);
    }
}
