use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::fs;

use crate::{
    events::ModelDownloadEvent,
    settings,
    state::{AppState, SessionStatus},
};

mod coreml;
mod definitions;
mod install;

use definitions::{MODELS, definition};
use install::{
    begin_download, cleanup_download, fallback_backend, finish_download, install_model,
    update_active_backend,
};

// Mirrors the flat shape the frontend binds to directly; splitting these into
// enums would break the generated TypeScript contract for no behavioral gain.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub description: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub installed: bool,
    pub active: bool,
    pub supports_translate: bool,
    pub core_ml_available: bool,
    pub core_ml_installed: bool,
    pub core_ml_enabled: bool,
    pub core_ml_size_bytes: Option<u64>,
}

pub fn catalog(app: &AppHandle) -> Result<Vec<ModelInfo>, String> {
    let state = app.state::<AppState>();
    let settings = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned")?
        .clone();
    let directory = model_dir(app)?;

    Ok(MODELS
        .iter()
        .map(|model| model_info(model, &directory, &settings.model_path))
        .collect())
}

pub async fn install(app: AppHandle, model_id: String) -> Result<ModelInfo, String> {
    let model = definition(&model_id)?;
    let cancel = begin_download(&app)?;
    let result = install_model(&app, model, cancel.clone()).await;
    finish_download(&app);

    if let Err(error) = &result {
        cleanup_download(&app, model.id).await;
        if !cancel.load(Ordering::Relaxed) {
            ModelDownloadEvent::failed(model.id, error).emit(&app).ok();
        }
    }
    result?;
    catalog_entry(&app, model.id)
}

pub async fn set_core_ml(
    app: AppHandle,
    model_id: String,
    enabled: bool,
) -> Result<ModelInfo, String> {
    if !cfg!(target_os = "macos") {
        return Err("Core ML is only available on macOS".into());
    }
    let model = definition(&model_id)?;
    if model.core_ml.is_none() {
        return Err("this model has no Core ML build".into());
    }
    let directory = model_dir(&app)?;
    if !directory.join(file_name(model.id)).is_file() {
        return Err("download the Whisper model before enabling Core ML".into());
    }

    if enabled && !coreml::installed(&directory, model.id) {
        let cancel = begin_download(&app)?;
        let result = coreml::install(&app, model, &directory, cancel.clone()).await;
        finish_download(&app);
        if let Err(error) = &result {
            coreml::cleanup(&directory, model.id).await;
            if !cancel.load(Ordering::Relaxed) {
                ModelDownloadEvent::failed(model.id, error).emit(&app).ok();
            }
        }
        result?;
    } else if !enabled {
        coreml::uninstall(&directory, model.id).await?;
    }

    update_active_backend(&app, model.id)?;
    catalog_entry(&app, model.id)
}

/// Remove a model's GGML weights and optional Core ML encoder bundle.
///
/// An active inference session owns the model context, so deletion is only
/// allowed while idle. Removing the selected model also clears its persisted
/// path; the user can then activate any other installed model normally.
pub async fn uninstall(app: AppHandle, model_id: String) -> Result<ModelInfo, String> {
    let model = definition(&model_id)?;
    let state = app.state::<AppState>();
    {
        let session = state.session.lock().map_err(|_| "session lock poisoned")?;
        if !matches!(session.status, SessionStatus::Idle | SessionStatus::Failed) {
            return Err("stop the active session before removing a model".into());
        }
    }
    if state
        .model_download
        .lock()
        .map_err(|_| "model download lock poisoned")?
        .is_some()
    {
        return Err("wait for the active model download to finish".into());
    }

    let directory = model_dir(&app)?;
    let model_path = directory.join(file_name(model.id));
    coreml::uninstall(&directory, model.id).await?;
    if model_path.exists() {
        fs::remove_file(&model_path)
            .await
            .map_err(|error| error.to_string())?;
    }
    cleanup_download(&app, model.id).await;

    let mut current = state
        .settings
        .lock()
        .map_err(|_| "settings lock poisoned")?
        .clone();
    if !current.model_path.is_empty() && Path::new(&current.model_path) == model_path {
        current.model_path.clear();
        current.inference_backend = fallback_backend().into();
        settings::save(&app, &current)?;
        *state
            .settings
            .lock()
            .map_err(|_| "settings lock poisoned")? = current.clone();
        crate::events::SettingsUpdatedEvent {
            settings: current.clone(),
        }
        .emit(&app)
        .map_err(|error| error.to_string())?;
        state.send_feed(crate::state::AppFeed::Settings {
            settings: Box::new(current),
        });
    }

    catalog_entry(&app, model.id)
}

pub fn cancel(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let active = state
        .model_download
        .lock()
        .map_err(|_| "model download lock poisoned")?;
    active
        .as_ref()
        .ok_or("no model download is active")?
        .store(true, Ordering::Relaxed);
    Ok(())
}

fn model_info(
    model: &definitions::ModelDefinition,
    directory: &Path,
    active_path: &str,
) -> ModelInfo {
    let path = directory.join(file_name(model.id));
    let core_ml_available = cfg!(target_os = "macos") && model.core_ml.is_some();
    let core_ml_installed = core_ml_available && coreml::installed(directory, model.id);
    ModelInfo {
        id: model.id.into(),
        label: model.label.into(),
        provider: model.provider.into(),
        description: model.description.into(),
        file_name: file_name(model.id),
        size_bytes: model.size_bytes,
        installed: path.is_file(),
        active: !active_path.is_empty() && path.as_path() == Path::new(active_path),
        supports_translate: model.supports_translate,
        core_ml_available,
        core_ml_installed,
        core_ml_enabled: core_ml_installed,
        core_ml_size_bytes: model.core_ml.as_ref().map(|core_ml| core_ml.size_bytes),
    }
}

fn catalog_entry(app: &AppHandle, model_id: &str) -> Result<ModelInfo, String> {
    catalog(app)?
        .into_iter()
        .find(|entry| entry.id == model_id)
        .ok_or_else(|| "model is missing from catalog".into())
}

fn model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("models"))
        .map_err(|error| error.to_string())
}

fn file_name(id: &str) -> String {
    format!("ggml-{id}.bin")
}

/// Builds a direct-download URL for a file in a Hugging Face model repo.
pub(super) fn huggingface_url(repo: &str, remote_name: &str) -> String {
    format!("https://huggingface.co/{repo}/resolve/main/{remote_name}?download=true")
}
