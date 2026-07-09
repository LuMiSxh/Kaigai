use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::{settings::AppSettings, state::AppState};

mod ytdlp;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    YtDlp,
    Ffmpeg,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ToolSource {
    Managed,
    Bundled,
    Path,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub tool: Tool,
    pub source: ToolSource,
    pub path: Option<String>,
    pub version: Option<String>,
}

impl Tool {
    fn base_name(self) -> &'static str {
        match self {
            Self::YtDlp => "yt-dlp",
            Self::Ffmpeg => "ffmpeg",
        }
    }

    /// File name carrying the Rust target triple, used both for the ffmpeg
    /// resource bundled at build time and the yt-dlp binary `Kaigai` manages
    /// itself — keeping one naming convention for both.
    fn platform_file_name(self) -> String {
        let suffix = if env::consts::OS == "windows" {
            ".exe"
        } else {
            ""
        };
        format!(
            "{}-{}{}",
            self.base_name(),
            env!("KAIGAI_TARGET_TRIPLE"),
            suffix
        )
    }

    fn version_arg(self) -> &'static str {
        match self {
            Self::YtDlp => "--version",
            Self::Ffmpeg => "-version",
        }
    }
}

/// Where `Kaigai` stores tools it downloads and updates itself, as opposed
/// to ffmpeg's build-time-bundled resource.
pub fn managed_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("tools"))
        .map_err(|error| error.to_string())
}

/// Where a managed yt-dlp install lives, downloaded and verified by
/// [`crate::tools::ytdlp`].
pub fn yt_dlp_managed_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(managed_dir(app)?.join(Tool::YtDlp.platform_file_name()))
}

/// Locates a system yt-dlp using the platform's path-lookup utility.
///
/// On macOS/Linux the lookup runs through a login shell (`/bin/sh -lc which
/// yt-dlp`) so that the user's full PATH — including Homebrew at
/// `/opt/homebrew/bin` — is visible even when the app is launched from the
/// Dock rather than a terminal.
pub fn find_system_yt_dlp() -> Option<PathBuf> {
    let output = if cfg!(target_os = "windows") {
        Command::new("where").arg("yt-dlp").output()
    } else {
        Command::new("/bin/sh")
            .args(["-lc", "which yt-dlp"])
            .output()
    };
    let out = output.ok().filter(|o| o.status.success())?;
    let path = PathBuf::from(String::from_utf8(out.stdout).ok()?.trim());
    executable(&path).then_some(path)
}

/// Whether a system `yt-dlp` can be found, used during onboarding to offer
/// "use my system yt-dlp" only when one actually exists.
pub fn system_yt_dlp_available() -> bool {
    find_system_yt_dlp().is_some()
}

/// The PATH a login shell would compute. A GUI app launched from Finder/Dock
/// inherits launchd's bare PATH, not the user's shell profile — yt-dlp needs
/// the fuller one on PATH itself (not just to be located) to find a JS
/// runtime (deno/node) for solving `YouTube`'s PO-token challenge, or it
/// silently returns "No video formats found" instead of an error.
#[cfg(not(target_os = "windows"))]
pub fn login_shell_path() -> Option<String> {
    let output = Command::new("/bin/sh")
        .args(["-lc", "printf %s \"$PATH\""])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
        .filter(|path| !path.is_empty())
}

#[cfg(target_os = "windows")]
pub fn login_shell_path() -> Option<String> {
    None
}

/// The latest yt-dlp version GitHub has published, for comparison against
/// whatever `status(app, Tool::YtDlp)` currently resolves to.
pub async fn latest_yt_dlp_version() -> Result<String, String> {
    ytdlp::latest_version().await
}

/// Downloads and installs the latest yt-dlp into the managed directory, used
/// both for the first managed install and for later manual/automatic updates.
pub async fn install_yt_dlp(app: AppHandle) -> Result<ToolStatus, String> {
    let cancel = begin_tool_download(&app)?;
    let result = ytdlp::install(app.clone(), cancel).await;
    finish_tool_download(&app);
    result
}

/// Silently installs a newer managed yt-dlp build if one is available and the
/// user has both opted into automatic tool updates and chosen the managed
/// source. Failures (no internet, GitHub unreachable, etc.) are logged, not
/// surfaced — a stale yt-dlp keeps working until the next successful check.
pub async fn maybe_auto_update_yt_dlp(app: AppHandle, settings: &AppSettings) {
    if !settings.automatic_tool_updates || settings.yt_dlp_source != "managed" {
        return;
    }
    let Some(current) = status(&app, Tool::YtDlp).version else {
        return;
    };
    let latest = match latest_yt_dlp_version().await {
        Ok(latest) => latest,
        Err(error) => {
            tracing::warn!(error = %error, "failed to check for yt-dlp updates");
            return;
        }
    };
    if current == latest {
        return;
    }
    tracing::info!(latest, "installing newer managed yt-dlp build");
    if let Err(error) = install_yt_dlp(app).await {
        tracing::warn!(error = %error, "automatic yt-dlp update failed");
    }
}

pub fn cancel_tool_download(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let active = state
        .tool_download
        .lock()
        .map_err(|_| "tool download lock poisoned")?;
    active
        .as_ref()
        .ok_or("no tool download is active")?
        .store(true, Ordering::Relaxed);
    Ok(())
}

fn begin_tool_download(app: &AppHandle) -> Result<Arc<AtomicBool>, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    let state = app.state::<AppState>();
    let mut active = state
        .tool_download
        .lock()
        .map_err(|_| "tool download lock poisoned")?;
    if active.is_some() {
        return Err("another tool download is already active".into());
    }
    *active = Some(cancel.clone());
    Ok(cancel)
}

fn finish_tool_download(app: &AppHandle) {
    if let Ok(mut active) = app.state::<AppState>().tool_download.lock() {
        *active = None;
    }
}

pub fn resolve(app: &AppHandle, tool: Tool) -> Result<PathBuf, String> {
    resolve_with_source(app, tool).and_then(|status| {
        status
            .path
            .map(PathBuf::from)
            .ok_or_else(|| format!("required tool is missing: {}", tool.base_name()))
    })
}

pub fn status(app: &AppHandle, tool: Tool) -> ToolStatus {
    resolve_with_source(app, tool).unwrap_or(ToolStatus {
        tool,
        source: ToolSource::Missing,
        path: None,
        version: None,
    })
}

fn resolve_with_source(app: &AppHandle, tool: Tool) -> Result<ToolStatus, String> {
    match tool {
        // ffmpeg always ships bundled (staged by build.rs); it's never user-
        // replaceable, so there's no managed or PATH tier to check.
        Tool::Ffmpeg => resolve_bundled(app, tool),
        Tool::YtDlp => resolve_yt_dlp(app),
    }
}

fn resolve_bundled(app: &AppHandle, tool: Tool) -> Result<ToolStatus, String> {
    let file_name = tool.platform_file_name();
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?;
    for bundled in [
        resource_dir.join("bin").join(&file_name),
        resource_dir.join("resources").join("bin").join(&file_name),
    ] {
        if executable(&bundled) {
            return Ok(status_for(tool, ToolSource::Bundled, &bundled));
        }
    }
    Err(format!("required bundled tool is missing: {file_name}"))
}

fn resolve_yt_dlp(app: &AppHandle) -> Result<ToolStatus, String> {
    if yt_dlp_source(app) == "system" {
        return match find_system_yt_dlp() {
            Some(path) => Ok(status_for(Tool::YtDlp, ToolSource::Path, &path)),
            None => Err("system yt-dlp not found — install it (e.g. brew install yt-dlp)".into()),
        };
    }

    let managed = yt_dlp_managed_path(app)?;
    if executable(&managed) {
        return Ok(status_for(Tool::YtDlp, ToolSource::Managed, &managed));
    }
    Err("yt-dlp has not been installed yet".into())
}

fn yt_dlp_source(app: &AppHandle) -> String {
    app.state::<AppState>().settings.lock().map_or_else(
        |_| "managed".into(),
        |settings| settings.yt_dlp_source.clone(),
    )
}

/// Whether `path` is a runnable executable: it must be a regular file, and on
/// Unix it must also carry an executable bit (so a downloaded-but-unchmod'd
/// sidecar is treated as missing rather than failing later at spawn time).
fn executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn status_for(tool: Tool, source: ToolSource, path: &Path) -> ToolStatus {
    let version = Command::new(path)
        .arg(tool.version_arg())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .and_then(|output| output.lines().next().map(str::to_owned));
    ToolStatus {
        tool,
        source,
        path: Some(path.to_string_lossy().into_owned()),
        version,
    }
}
