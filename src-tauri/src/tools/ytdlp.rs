//! Managed yt-dlp installation and updates.

use std::sync::{Arc, atomic::AtomicBool};

use serde::Deserialize;
use tauri::AppHandle;
use tokio::fs;

use super::{Tool, ToolStatus, managed_dir, status, yt_dlp_managed_path};
use crate::download;

const REPO: &str = "yt-dlp/yt-dlp";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// The self-contained (no system Python required) release asset for our
/// target triple. Keyed by `KAIGAI_TARGET_TRIPLE`, the same build-time
/// triple `build.rs`/`tools::mod` use to pin ffmpeg and `QuickJS`, rather
/// than just `target_os` — so an unsupported target fails loudly here
/// instead of quietly grabbing the wrong platform's binary.
fn platform_asset_name() -> Result<&'static str, String> {
    match env!("KAIGAI_TARGET_TRIPLE") {
        "aarch64-apple-darwin" => Ok("yt-dlp_macos"),
        "x86_64-pc-windows-msvc" => Ok("yt-dlp.exe"),
        "x86_64-unknown-linux-gnu" => Ok("yt-dlp_linux"),
        other => Err(format!("no managed yt-dlp build for target {other}")),
    }
}

/// The latest published yt-dlp version, regardless of what's installed —
/// callers compare it against the currently resolved version themselves.
pub async fn latest_version() -> Result<String, String> {
    Ok(fetch_release().await?.tag_name)
}

/// Downloads, verifies, and installs the latest yt-dlp into the managed
/// directory, replacing whatever (if anything) was there before.
pub async fn install(app: AppHandle, cancel: Arc<AtomicBool>) -> Result<ToolStatus, String> {
    let release = fetch_release().await?;
    let asset = find_asset(&release, platform_asset_name()?)?;
    let sums_asset = find_asset(&release, "SHA2-256SUMS")?;

    let sums_text = reqwest::Client::new()
        .get(&sums_asset.browser_download_url)
        .header("User-Agent", "Kaigai")
        .send()
        .await
        .map_err(|error| format!("failed to fetch yt-dlp checksums: {error}"))?
        .text()
        .await
        .map_err(|error| format!("failed to read yt-dlp checksums: {error}"))?;
    let expected_hash = checksum_for(&sums_text, &asset.name)?;

    let size = reqwest::Client::new()
        .head(&asset.browser_download_url)
        .header("User-Agent", "Kaigai")
        .send()
        .await
        .map_err(|error| format!("failed to check yt-dlp download size: {error}"))?
        .content_length()
        .ok_or("could not determine the yt-dlp download size")?;

    let directory = managed_dir(&app)?;
    fs::create_dir_all(&directory)
        .await
        .map_err(|error| error.to_string())?;
    let destination = yt_dlp_managed_path(&app)?;
    let temporary = destination.with_extension("part");

    download::verified(
        &app,
        "yt-dlp",
        &asset.browser_download_url,
        &temporary,
        size,
        &expected_hash,
        &cancel,
    )
    .await?;
    fs::rename(&temporary, &destination)
        .await
        .map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o755))
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(status(&app, Tool::YtDlp))
}

fn find_asset<'a>(release: &'a Release, name: &str) -> Result<&'a Asset, String> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .ok_or_else(|| format!("yt-dlp release {} is missing {name}", release.tag_name))
}

async fn fetch_release() -> Result<Release, String> {
    reqwest::Client::new()
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header("User-Agent", "Kaigai")
        .send()
        .await
        .map_err(|error| format!("failed to check for yt-dlp updates: {error}"))?
        .error_for_status()
        .map_err(|error| format!("failed to check for yt-dlp updates: {error}"))?
        .json()
        .await
        .map_err(|error| format!("yt-dlp release response was malformed: {error}"))
}

/// Parses a `SHA2-256SUMS`-style file (`<hash>  <name>` per line, an optional
/// leading `*` before the name marks binary mode) for one asset's checksum.
fn checksum_for(sums_text: &str, asset_name: &str) -> Result<String, String> {
    sums_text
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let name = parts.next()?.trim_start_matches('*');
            (name == asset_name).then(|| hash.to_lowercase())
        })
        .ok_or_else(|| format!("no checksum found for {asset_name} in SHA2-256SUMS"))
}
