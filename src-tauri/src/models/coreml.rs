use std::{
    fs, io,
    path::{Path, PathBuf},
};

use std::sync::{Arc, atomic::AtomicBool};

use tauri::AppHandle;
use tauri_specta::Event;
use tokio::fs as async_fs;

use crate::{download, events::ModelDownloadEvent};

use super::{definitions::ModelDefinition, huggingface_url};

pub struct CoreMlDefinition {
    pub size_bytes: u64,
    pub sha256: &'static str,
}

pub fn archive_name(model_id: &str) -> String {
    format!("ggml-{model_id}-encoder.mlmodelc.zip")
}

pub fn active_path(directory: &Path, model_id: &str) -> PathBuf {
    directory.join(format!("ggml-{model_id}-encoder.mlmodelc"))
}

pub fn installed(directory: &Path, model_id: &str) -> bool {
    active_path(directory, model_id).is_dir()
}

pub async fn install(
    app: &AppHandle,
    model: &ModelDefinition,
    directory: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let metadata = model.core_ml.as_ref().ok_or("model has no Core ML build")?;
    let archive = directory.join(archive_name(model.id));
    ModelDownloadEvent::started(model.id, metadata.size_bytes)
        .emit(app)
        .map_err(|error| error.to_string())?;
    download::verified(
        app,
        model.id,
        &huggingface_url(model.repo, &archive_name(model.id)),
        &archive,
        metadata.size_bytes,
        metadata.sha256,
        &cancel,
    )
    .await?;
    ModelDownloadEvent::installing(model.id, metadata.size_bytes, metadata.size_bytes)
        .emit(app)
        .ok();
    let archive_for_extract = archive.clone();
    let destination = active_path(directory, model.id);
    tokio::task::spawn_blocking(move || extract_archive(&archive_for_extract, &destination))
        .await
        .map_err(|error| error.to_string())??;
    let _ = async_fs::remove_file(archive).await;
    ModelDownloadEvent::ready(model.id, metadata.size_bytes, metadata.size_bytes)
        .emit(app)
        .ok();
    Ok(())
}

pub async fn uninstall(directory: &Path, model_id: &str) -> Result<(), String> {
    let path = active_path(directory, model_id);
    if path.exists() {
        async_fs::remove_dir_all(path)
            .await
            .map_err(|error| error.to_string())?;
    }
    cleanup(directory, model_id).await;
    Ok(())
}

pub async fn cleanup(directory: &Path, model_id: &str) {
    let _ = async_fs::remove_file(directory.join(archive_name(model_id))).await;
    let staging = active_path(directory, model_id).with_extension("mlmodelc.extracting");
    let _ = async_fs::remove_dir_all(staging).await;
}

pub fn extract_archive(archive: &Path, destination: &Path) -> Result<(), String> {
    let staging = destination.with_extension("mlmodelc.extracting");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;

    let file = fs::File::open(archive).map_err(|error| error.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| error.to_string())?;
        let relative = entry
            .enclosed_name()
            .ok_or("Core ML archive contains an unsafe path")?;
        let output = staging.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut file = fs::File::create(&output).map_err(|error| error.to_string())?;
            io::copy(&mut entry, &mut file).map_err(|error| error.to_string())?;
        }
    }

    let extracted = find_bundle(&staging)?;
    let _ = fs::remove_dir_all(destination);
    fs::rename(&extracted, destination).map_err(|error| error.to_string())?;
    let _ = fs::remove_dir_all(staging);
    Ok(())
}

fn find_bundle(directory: &Path) -> Result<PathBuf, String> {
    if directory
        .extension()
        .is_some_and(|extension| extension == "mlmodelc")
    {
        return Ok(directory.to_owned());
    }
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            if path
                .extension()
                .is_some_and(|extension| extension == "mlmodelc")
            {
                return Ok(path);
            }
            if let Ok(found) = find_bundle(&path) {
                return Ok(found);
            }
        }
    }
    Err("Core ML archive did not contain an .mlmodelc bundle".into())
}
