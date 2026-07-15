use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
};

use crate::events::ModelDownloadEvent;

pub async fn verified(
    app: &AppHandle,
    event_id: &str,
    url: &str,
    temporary: &Path,
    expected_size: u64,
    expected_hash: &str,
    cancel: &AtomicBool,
) -> Result<u64, String> {
    let _ = fs::remove_file(temporary).await;
    let result = download_and_verify(
        app,
        event_id,
        url,
        temporary,
        expected_size,
        expected_hash,
        cancel,
    )
    .await;
    if result.is_err() {
        let _ = fs::remove_file(temporary).await;
    }
    result
}

// Progress rates and ETAs are display estimates; sub-byte precision is irrelevant.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
async fn download_and_verify(
    app: &AppHandle,
    event_id: &str,
    url: &str,
    temporary: &Path,
    expected_size: u64,
    expected_hash: &str,
    cancel: &AtomicBool,
) -> Result<u64, String> {
    let started = Instant::now();
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| format!("download request failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("download request failed: {error}"))?;
    let mut stream = response.bytes_stream();
    let mut file = File::create(temporary)
        .await
        .map_err(|error| format!("failed to create {}: {error}", temporary.display()))?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    let mut last_emit = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            ModelDownloadEvent::cancelled(event_id).emit(app).ok();
            return Err("download cancelled".into());
        }
        let chunk = chunk.map_err(|error| format!("model download interrupted: {error}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("failed to write {}: {error}", temporary.display()))?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;

        if last_emit.elapsed() >= Duration::from_millis(200) || downloaded == expected_size {
            let rate = downloaded as f64 / started.elapsed().as_secs_f64().max(0.001);
            let eta = (rate > 0.0)
                .then(|| ((expected_size.saturating_sub(downloaded)) as f64 / rate) as u64);
            ModelDownloadEvent::progress(event_id, downloaded, expected_size, rate, eta)
                .emit(app)
                .ok();
            last_emit = Instant::now();
        }
    }
    file.flush()
        .await
        .map_err(|error| format!("failed to flush {}: {error}", temporary.display()))?;
    drop(file);
    ModelDownloadEvent::verifying(event_id, downloaded, expected_size)
        .emit(app)
        .ok();
    let actual_hash = format!("{:x}", hasher.finalize());
    if actual_hash != expected_hash || downloaded != expected_size {
        return Err(format!(
            "download verification failed for {url}: expected {expected_size} bytes and {expected_hash}, got {downloaded} bytes and {actual_hash}"
        ));
    }
    Ok(downloaded)
}
