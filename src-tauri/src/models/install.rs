use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::fs;

use super::{coreml, definitions::ModelDefinition, file_name, huggingface_url, model_dir};
use crate::{
    download,
    events::{ModelDownloadEvent, SettingsUpdatedEvent},
    settings,
    state::AppState,
};

/// Backend label to report when Core ML isn't installed/active: whisper-rs
/// is built with the Metal backend on macOS and plain CPU everywhere else.
pub(super) fn fallback_backend() -> &'static str {
    if cfg!(target_os = "macos") {
        "metal"
    } else {
        "cpu"
    }
}

pub(super) async fn install_model(
    app: &AppHandle,
    model: &ModelDefinition,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let directory = model_dir(app)?;
    fs::create_dir_all(&directory)
        .await
        .map_err(|error| error.to_string())?;
    let destination = directory.join(file_name(model.id));

    ModelDownloadEvent::started(model.id, model.size_bytes)
        .emit(app)
        .map_err(|error| error.to_string())?;
    if !destination.is_file() {
        let temporary = destination.with_extension("bin.part");
        download::verified(
            app,
            model.id,
            &huggingface_url(model.repo, &file_name(model.id)),
            &temporary,
            model.size_bytes,
            model.sha256,
            &cancel,
        )
        .await?;
        fs::rename(temporary, &destination)
            .await
            .map_err(|error| error.to_string())?;
    }

    activate(app, model, &destination)?;
    ModelDownloadEvent::ready(model.id, model.size_bytes, model.size_bytes)
        .emit(app)
        .ok();
    Ok(())
}

pub(super) fn activate(
    app: &AppHandle,
    model: &ModelDefinition,
    destination: &Path,
) -> Result<(), String> {
    let backend = if coreml::installed(
        destination.parent().ok_or("model path has no parent")?,
        model.id,
    ) {
        "coreml"
    } else {
        fallback_backend()
    };
    save_active_model(app, model.id, destination, backend)
}

pub(super) fn update_active_backend(app: &AppHandle, model_id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let current = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned")?
        .clone();
    if current.model != model_id {
        return Ok(());
    }
    let backend = if coreml::installed(&model_dir(app)?, model_id) {
        "coreml"
    } else {
        fallback_backend()
    };
    save_active_model(app, model_id, &PathBuf::from(current.model_path), backend)
}

fn save_active_model(
    app: &AppHandle,
    model_id: &str,
    destination: &Path,
    backend: &str,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut current = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned")?
        .clone();
    current.model = model_id.into();
    current.model_path = destination.to_string_lossy().into_owned();
    current.inference_backend = backend.into();
    settings::save(app, &current)?;
    *state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned")? = current.clone();
    SettingsUpdatedEvent { settings: current }
        .emit(app)
        .map_err(|error| error.to_string())
}

pub(super) fn begin_download(app: &AppHandle) -> Result<Arc<AtomicBool>, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    let state = app.state::<AppState>();
    let mut active = state
        .model_download
        .lock()
        .map_err(|_| "model download lock poisoned")?;
    if active.is_some() {
        return Err("another model download is already active".into());
    }
    *active = Some(cancel.clone());
    Ok(cancel)
}

pub(super) fn finish_download(app: &AppHandle) {
    if let Ok(mut active) = app.state::<AppState>().model_download.lock() {
        *active = None;
    }
}

pub(super) async fn cleanup_download(app: &AppHandle, model_id: &str) {
    if let Ok(directory) = model_dir(app) {
        let _ = fs::remove_file(
            directory
                .join(file_name(model_id))
                .with_extension("bin.part"),
        )
        .await;
    }
}
