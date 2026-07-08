use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::fs;

use crate::{
    download,
    events::{ModelDownloadEvent, SettingsUpdatedEvent},
    settings,
    state::{AppFeed, AppState, SessionStatus},
};

mod coreml;

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

pub(super) struct ModelDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub provider: &'static str,
    /// What sets this model apart, shown to the user while picking a model.
    pub description: &'static str,
    /// Hugging Face repo the GGML weights (and Core ML archive, if any) are
    /// downloaded from — not every model lives in `ggerganov/whisper.cpp`.
    pub repo: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
    pub core_ml: Option<coreml::CoreMlDefinition>,
    /// Whether this model supports the Whisper `translate` task.
    /// Distilled/turbo models are fine-tuned on transcription only and
    /// silently ignore the translate flag, outputting the source language.
    pub supports_translate: bool,
}

const MODELS: [ModelDefinition; 7] = [
    ModelDefinition {
        id: "tiny",
        label: "Tiny",
        provider: "OpenAI",
        description: "Fastest and lightest. Expect noticeably more mistakes, especially in Japanese — best for quick tests, not daily use.",
        repo: "ggerganov/whisper.cpp",
        size_bytes: 77_691_713,
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
        core_ml: Some(coreml::CoreMlDefinition {
            size_bytes: 15_037_446,
            sha256: "c88cbd2648e1f5415092bcf5256add463a0f19943e6938f46e8d4ffdebd47739",
        }),
        supports_translate: true,
    },
    ModelDefinition {
        id: "base",
        label: "Base",
        provider: "OpenAI",
        description: "A step up from Tiny in accuracy, still very fast. Fine for casual viewing where occasional mistakes are okay.",
        repo: "ggerganov/whisper.cpp",
        size_bytes: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        core_ml: Some(coreml::CoreMlDefinition {
            size_bytes: 37_922_638,
            sha256: "7e6ab77041942572f239b5b602f8aaa1c3ed29d73e3d8f20abea03a773541089",
        }),
        supports_translate: true,
    },
    ModelDefinition {
        id: "small",
        label: "Small",
        provider: "OpenAI",
        description: "Balanced speed and accuracy. A solid default for live Japanese captioning.",
        repo: "ggerganov/whisper.cpp",
        size_bytes: 487_601_967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        core_ml: Some(coreml::CoreMlDefinition {
            size_bytes: 163_083_239,
            sha256: "de43fb9fed471e95c19e60ae67575c2bf09e8fb607016da171b06ddad313988b",
        }),
        supports_translate: true,
    },
    ModelDefinition {
        id: "medium",
        label: "Medium",
        provider: "OpenAI",
        description: "More accurate than Small on accents and background noise, but slower — may lag behind fast speech.",
        repo: "ggerganov/whisper.cpp",
        size_bytes: 1_533_763_059,
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
        core_ml: Some(coreml::CoreMlDefinition {
            size_bytes: 567_829_413,
            sha256: "79b0b8d436d47d3f24dd3afc91f19447dd686a4f37521b2f6d9c30a642133fbd",
        }),
        supports_translate: true,
    },
    ModelDefinition {
        id: "large-v3",
        label: "Large v3",
        provider: "OpenAI",
        description: "OpenAI's most accurate general-purpose model. Best on difficult audio, but the slowest — real-time captioning may fall behind.",
        repo: "ggerganov/whisper.cpp",
        size_bytes: 3_095_033_483,
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
        core_ml: Some(coreml::CoreMlDefinition {
            size_bytes: 1_175_711_232,
            sha256: "47837be7594a29429ec08620043390c4d6d467f8bd362df09e9390ace76a55a4",
        }),
        supports_translate: true,
    },
    ModelDefinition {
        id: "large-v3-turbo",
        label: "Large v3 Turbo",
        provider: "OpenAI",
        description: "A distilled Large v3: about 6x faster with nearly identical Japanese accuracy. The best accuracy-to-speed tradeoff for live captioning. Translation to English is not supported by this model.",
        repo: "ggerganov/whisper.cpp",
        size_bytes: 1_624_555_275,
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
        core_ml: Some(coreml::CoreMlDefinition {
            size_bytes: 1_173_393_014,
            sha256: "84bedfe895bd7b5de6e8e89a0803dfc5addf8c0c5bc4c937451716bf7cf7988a",
        }),
        supports_translate: false,
    },
    ModelDefinition {
        id: "kotoba-whisper-v2.0",
        label: "Kotoba Whisper v2.0",
        provider: "Kotoba-Whisper",
        description: "Fine-tuned specifically for Japanese speech (distilled from Large v3). Reported to beat OpenAI's Large v3 on Japanese benchmarks while running about 6x faster. No Core ML build available. Translation to English is not supported by this model.",
        repo: "kotoba-tech/kotoba-whisper-v2.0-ggml",
        size_bytes: 1_519_521_155,
        sha256: "eff70a8a236e731abba774ba71e1f6d0fce53302137208c32207e694e0bf4546",
        core_ml: None,
        supports_translate: false,
    },
];

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
        current.inference_backend = "metal".into();
        settings::save(&app, &current)?;
        *state
            .settings
            .lock()
            .map_err(|_| "settings lock poisoned")? = current.clone();
        SettingsUpdatedEvent {
            settings: current.clone(),
        }
        .emit(&app)
        .map_err(|error| error.to_string())?;
        state.send_feed(AppFeed::Settings {
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

async fn install_model(
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

fn activate(app: &AppHandle, model: &ModelDefinition, destination: &Path) -> Result<(), String> {
    let backend = if coreml::installed(
        destination.parent().ok_or("model path has no parent")?,
        model.id,
    ) {
        "coreml"
    } else {
        "metal"
    };
    save_active_model(app, model.id, destination, backend)
}

fn update_active_backend(app: &AppHandle, model_id: &str) -> Result<(), String> {
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
        "metal"
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

fn model_info(model: &ModelDefinition, directory: &Path, active_path: &str) -> ModelInfo {
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

fn definition(id: &str) -> Result<&'static ModelDefinition, String> {
    MODELS
        .iter()
        .find(|model| model.id == id)
        .ok_or_else(|| format!("unknown Whisper model: {id}"))
}

fn begin_download(app: &AppHandle) -> Result<Arc<AtomicBool>, String> {
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

fn finish_download(app: &AppHandle) {
    if let Ok(mut active) = app.state::<AppState>().model_download.lock() {
        *active = None;
    }
}

async fn cleanup_download(app: &AppHandle, model_id: &str) {
    if let Ok(directory) = model_dir(app) {
        let _ = fs::remove_file(
            directory
                .join(file_name(model_id))
                .with_extension("bin.part"),
        )
        .await;
    }
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
