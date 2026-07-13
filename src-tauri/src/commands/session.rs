use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use super::{AppSnapshot, emit_state, snapshot};
use crate::{
    events::{DiagnosticsEvent, SessionErrorEvent, SubtitleClearEvent},
    pipeline,
    state::{AppFeed, AppState, SessionStatus},
};

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
