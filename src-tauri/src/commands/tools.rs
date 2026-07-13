use tauri::AppHandle;

use crate::tools::{self, Tool, ToolStatus};

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
