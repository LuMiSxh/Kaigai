use std::{
    fmt::Write as _,
    io::{BufRead, BufReader, Read},
    process::{Child, ChildStdout, Command, Stdio},
};

use serde::Deserialize;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::{
    events::DiagnosticsEvent,
    settings::AppSettings,
    tools::{self, Tool},
};

pub struct MediaProcess {
    decoder: Child,
}

impl MediaProcess {
    pub fn spawn(
        app: &AppHandle,
        stream_url: &str,
        settings: &AppSettings,
    ) -> Result<(Self, ChildStdout), String> {
        let source = resolve_source(app, stream_url, settings)?;
        tracing::info!(protocol = %source.protocol, "resolved direct media source");

        let mut decoder = spawn_decoder(app, &source)?;
        pipe_stderr(app, "ffmpeg", decoder.stderr.take());
        let pcm = decoder
            .stdout
            .take()
            .ok_or("ffmpeg stdout pipe was not created")?;
        Ok((Self { decoder }, pcm))
    }

    pub fn stop(&mut self) {
        terminate(&mut self.decoder);
    }

    pub fn verify_exit(&mut self) -> Result<(), String> {
        let status = self.decoder.wait().map_err(|error| error.to_string())?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| format!("media decoder exited unexpectedly ({status})"))
    }
}

impl Drop for MediaProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

struct MediaSource {
    protocol: String,
    url: String,
    headers: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct YtDlpInfo {
    protocol: Option<String>,
    url: String,
    #[serde(default)]
    http_headers: std::collections::BTreeMap<String, String>,
}

fn resolve_source(
    app: &AppHandle,
    stream_url: &str,
    settings: &AppSettings,
) -> Result<MediaSource, String> {
    let executable = tools::resolve(app, Tool::YtDlp)?;
    tracing::debug!(path = %executable.display(), "resolved yt-dlp");
    let mut command = Command::new(executable);
    if let Some(path) = tools::login_shell_path() {
        command.env("PATH", path);
    }
    command.args([
        "--no-playlist",
        "--quiet",
        "--no-warnings",
        "--skip-download",
        "--format",
        "bestaudio/best",
        "--dump-single-json",
    ]);
    add_authentication(&mut command, settings);
    add_js_runtime(&mut command, app, settings);
    let output = command
        .arg(stream_url)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to resolve stream with yt-dlp: {error}"))?;

    if !output.status.success() {
        let error = crate::tracing_setup::sanitize(&String::from_utf8_lossy(&output.stderr));
        if is_safari_cookie_permission_error(settings, &error) {
            return Err(
                "yt-dlp could not read Safari's cookies (macOS denied access). Grant Kaigai \
                 Full Disk Access in System Settings \u{2192} Privacy & Security, quit Safari, \
                 then try again."
                    .to_string(),
            );
        }
        return Err(format!(
            "yt-dlp could not resolve the stream: {}",
            error.trim()
        ));
    }

    let info: YtDlpInfo = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("yt-dlp returned invalid stream metadata: {error}"))?;
    Ok(MediaSource {
        protocol: info.protocol.unwrap_or_else(|| "unknown".into()),
        url: info.url,
        headers: info.http_headers.into_iter().collect(),
    })
}

fn is_safari_cookie_permission_error(settings: &AppSettings, error: &str) -> bool {
    settings.cookie_mode == "browser"
        && settings.browser == "safari"
        && error.contains("Operation not permitted")
        && error.contains("Cookies.binarycookies")
}

fn add_authentication(command: &mut Command, settings: &AppSettings) {
    match settings.cookie_mode.as_str() {
        "browser" => {
            let mut browser = settings.browser.clone();
            if !settings.browser_profile.trim().is_empty() {
                browser.push(':');
                browser.push_str(settings.browser_profile.trim());
            }
            command.args(["--cookies-from-browser", &browser]);
        }
        "file" if !settings.cookie_file.trim().is_empty() => {
            command.args(["--cookies", settings.cookie_file.trim()]);
        }
        _ => {}
    }
}

/// yt-dlp only tries Deno by default for `YouTube`'s JS challenges — every
/// other runtime needs an explicit `--js-runtimes` flag. "bundled" points it
/// at Kaigai's pinned `QuickJS` sidecar; a missing binary falls back to
/// "system" behavior instead of failing stream resolution outright.
fn add_js_runtime(command: &mut Command, app: &AppHandle, settings: &AppSettings) {
    if settings.js_runtime_source != "bundled" {
        return;
    }
    match tools::resolve(app, Tool::QuickJs) {
        Ok(path) => {
            command.arg("--js-runtimes");
            command.arg(format!("quickjs:{}", path.display()));
        }
        Err(error) => {
            tracing::warn!(
                error = %error,
                "bundled QuickJS runtime unavailable, falling back to yt-dlp's own runtime detection"
            );
        }
    }
}

fn spawn_decoder(app: &AppHandle, source: &MediaSource) -> Result<Child, String> {
    let executable = tools::resolve(app, Tool::Ffmpeg)?;
    tracing::debug!(path = %executable.display(), "resolved ffmpeg");
    let mut command = Command::new(executable);
    command.args([
        "-hide_banner",
        "-loglevel",
        "warning",
        "-fflags",
        "+nobuffer",
        "-flags",
        "+low_delay",
        "-probesize",
        "1M",
        "-analyzeduration",
        "1M",
        "-reconnect",
        "1",
        "-reconnect_streamed",
        "1",
        "-reconnect_delay_max",
        "2",
    ]);
    if !source.headers.is_empty() {
        let headers = source
            .headers
            .iter()
            .fold(String::new(), |mut acc, (name, value)| {
                let _ = write!(acc, "{name}: {value}\r\n");
                acc
            });
        command.args(["-headers", &headers]);
    }
    command
        .args([
            "-i",
            &source.url,
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-f",
            "s16le",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start ffmpeg: {error}"))
}

fn pipe_stderr(app: &AppHandle, process: &'static str, stderr: Option<impl Read + Send + 'static>) {
    let Some(stderr) = stderr else {
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let sanitized = crate::tracing_setup::sanitize(&line);
            tracing::info!(process, message = %sanitized, "sidecar output");
            let _ = DiagnosticsEvent {
                message: format!("[{process}] {sanitized}"),
            }
            .emit(&app);
        }
    });
}

fn terminate(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
}
