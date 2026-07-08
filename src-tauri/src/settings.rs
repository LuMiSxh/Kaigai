use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

const LEGACY_IDENTIFIER: &str = "com.lumisxh.kaigaisub";

/// Default subtitle font stack: the system UI font, falling back to common
/// Japanese gothic faces so CJK glyphs always render.
pub const DEFAULT_FONT_FAMILY: &str =
    "system-ui, 'Hiragino Kaku Gothic ProN', 'Yu Gothic', Meiryo, sans-serif";

/// All persisted user settings. Kept as one flat struct so the serialized JSON
/// and the generated TypeScript binding stay simple; the comments group the
/// fields by concern (engine, chunking, overlay, authentication, maintenance).
///
/// `#[serde(default)]` makes deserialization forward-compatible: settings files
/// written by an older version that lack newer fields fall back to the defaults
/// instead of failing to load.
// Mirrors the flat shape the frontend binds to directly; splitting these into
// enums would break the generated TypeScript contract for no behavioral gain.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    // Engine / inference
    pub model: String,
    pub model_path: String,
    pub inference_backend: String,
    pub source_language: String,
    pub task: String,
    // Chunking
    pub chunk_mode: String,
    /// Silero probability preset: "high" detects quieter speech, "strict"
    /// rejects more music/noise, and "balanced" is the general default.
    pub vad_sensitivity: String,
    pub minimum_chunk_ms: u32,
    pub maximum_chunk_ms: u32,
    pub end_silence_ms: u32,
    pub overlap_ms: u32,
    // Subtitle overlay
    pub subtitle_offset_ms: i32,
    pub font_size_px: u32,
    pub font_weight: u32,
    pub font_family: String,
    pub text_color: String,
    pub background_color: String,
    pub background_opacity: f32,
    pub always_on_top: bool,
    pub click_through: bool,
    // Stream authentication
    pub cookie_mode: String,
    pub browser: String,
    pub browser_profile: String,
    pub cookie_file: String,
    // Maintenance
    pub automatic_tool_updates: bool,
    /// Whether yt-dlp is resolved from `PATH` ("system") or downloaded and
    /// self-updated by `Kaigai` ("managed"). ffmpeg has no such choice: it
    /// ships bundled with the app and is never resolved at runtime.
    pub yt_dlp_source: String,
    /// Whether the first-run setup tour has been completed.
    pub onboarded: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            model: "small".into(),
            model_path: String::new(),
            inference_backend: "metal".into(),
            source_language: "ja".into(),
            task: "translate".into(),
            chunk_mode: "adaptive".into(),
            vad_sensitivity: "balanced".into(),
            minimum_chunk_ms: 1_000,
            maximum_chunk_ms: 6_000,
            end_silence_ms: 250,
            overlap_ms: 600,
            subtitle_offset_ms: 0,
            font_size_px: 36,
            font_weight: 600,
            font_family: DEFAULT_FONT_FAMILY.into(),
            text_color: "#ffffff".into(),
            background_color: "#000000".into(),
            background_opacity: 0.72,
            always_on_top: true,
            click_through: false,
            cookie_mode: "none".into(),
            browser: "firefox".into(),
            browser_profile: String::new(),
            cookie_file: String::new(),
            automatic_tool_updates: true,
            yt_dlp_source: "managed".into(),
            onboarded: false,
        }
    }
}

/// Moves data written under the pre-rename bundle identifier into Kaigai's
/// current storage directories. Each entry is moved only when the destination
/// does not exist, so this is safe to run on every startup.
pub fn migrate_legacy_storage(app: &AppHandle) -> Result<(), String> {
    let current_config = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;
    let current_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;

    if let Some(legacy_config) = dirs::config_dir().map(|path| path.join(LEGACY_IDENTIFIER)) {
        move_if_missing(
            &legacy_config.join("settings.json"),
            &current_config.join("settings.json"),
        )?;
    }

    if let Some(legacy_data) = dirs::data_dir().map(|path| path.join(LEGACY_IDENTIFIER)) {
        let legacy_models = legacy_data.join("models");
        let current_models = current_data.join("models");
        move_if_missing(&legacy_models, &current_models)?;
        move_if_missing(&legacy_data.join("tools"), &current_data.join("tools"))?;
        rewrite_legacy_model_path(
            &current_config.join("settings.json"),
            &legacy_models,
            &current_models,
        )?;
    }

    Ok(())
}

fn move_if_missing(source: &Path, destination: &Path) -> Result<bool, String> {
    if !source.exists() || destination.exists() {
        return Ok(false);
    }
    let parent = destination
        .parent()
        .ok_or("migration destination has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    fs::rename(source, destination).map_err(|error| error.to_string())?;
    Ok(true)
}

fn rewrite_legacy_model_path(
    settings_path: &Path,
    legacy_models: &Path,
    current_models: &Path,
) -> Result<(), String> {
    let Ok(contents) = fs::read_to_string(settings_path) else {
        return Ok(());
    };
    let mut settings: serde_json::Value =
        serde_json::from_str(&contents).map_err(|error| error.to_string())?;
    let Some(model_path) = settings.get("modelPath").and_then(|value| value.as_str()) else {
        return Ok(());
    };
    let Ok(relative_path) = Path::new(model_path).strip_prefix(legacy_models) else {
        return Ok(());
    };
    let migrated_path = current_models.join(relative_path);
    if !migrated_path.is_file() {
        return Ok(());
    }
    settings["modelPath"] = serde_json::Value::String(migrated_path.to_string_lossy().into_owned());

    let temporary = settings_path.with_extension("json.tmp");
    let contents = serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?;
    fs::write(&temporary, contents).map_err(|error| error.to_string())?;
    fs::rename(&temporary, settings_path).map_err(|error| error.to_string())
}

pub fn load(app: &AppHandle) -> AppSettings {
    path(app)
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    validate(settings)?;
    let path = path(app)?;
    let parent = path.parent().ok_or("settings path has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;

    let temporary = path.with_extension("json.tmp");
    let contents = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(&temporary, contents).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}

pub fn validate(settings: &AppSettings) -> Result<(), String> {
    choice(
        &settings.inference_backend,
        &["metal", "coreml"],
        "inference backend",
    )?;
    choice(
        &settings.source_language,
        &["ja", "ko", "zh", "en", "auto"],
        "source language",
    )?;
    choice(&settings.task, &["translate", "transcribe"], "task")?;
    choice(&settings.chunk_mode, &["adaptive", "fixed"], "chunk mode")?;
    choice(
        &settings.vad_sensitivity,
        &["high", "balanced", "strict"],
        "voice detection sensitivity",
    )?;
    choice(
        &settings.cookie_mode,
        &["none", "browser", "file"],
        "cookie mode",
    )?;
    choice(
        &settings.browser,
        &["firefox", "safari", "chrome", "edge", "brave"],
        "browser",
    )?;
    choice(
        &settings.yt_dlp_source,
        &["managed", "system"],
        "yt-dlp source",
    )?;

    if !(16..=96).contains(&settings.font_size_px) {
        return Err("font size must be between 16 and 96".into());
    }
    if !(100..=900).contains(&settings.font_weight) {
        return Err("font weight must be between 100 and 900".into());
    }
    if !(500..=20_000).contains(&settings.minimum_chunk_ms)
        || settings.maximum_chunk_ms < 4_000
        || settings.maximum_chunk_ms < settings.minimum_chunk_ms
        || settings.maximum_chunk_ms > 30_000
    {
        return Err("invalid chunk duration range".into());
    }
    if !(150..=2_000).contains(&settings.end_silence_ms) {
        return Err("end silence must be between 150 and 2000 ms".into());
    }
    if settings.overlap_ms > 3_000 || settings.overlap_ms >= settings.maximum_chunk_ms {
        return Err("invalid overlap duration".into());
    }
    if !(-10_000..=10_000).contains(&settings.subtitle_offset_ms) {
        return Err("subtitle offset must be between -10000 and 10000 ms".into());
    }
    if !(0.0..=1.0).contains(&settings.background_opacity) {
        return Err("background opacity must be between 0 and 1".into());
    }
    if !is_hex_color(&settings.text_color) || !is_hex_color(&settings.background_color) {
        return Err("subtitle colors must use #RRGGBB format".into());
    }
    if !settings.model_path.trim().is_empty() && !Path::new(&settings.model_path).is_file() {
        return Err("configured Whisper model does not exist".into());
    }
    if settings.cookie_mode == "file" {
        let cookie_file = settings.cookie_file.trim();
        if !cookie_file.is_empty() && !Path::new(cookie_file).is_file() {
            return Err("configured cookie file does not exist".into());
        }
    }
    Ok(())
}

fn choice(value: &str, allowed: &[&str], label: &str) -> Result<(), String> {
    allowed
        .contains(&value)
        .then_some(())
        .ok_or_else(|| format!("invalid {label}"))
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join("settings.json"))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert!(validate(&AppSettings::default()).is_ok());
    }

    #[test]
    fn rejects_out_of_range_font_size() {
        assert!(
            validate(&AppSettings {
                font_size_px: 8,
                ..AppSettings::default()
            })
            .is_err()
        );
        assert!(
            validate(&AppSettings {
                font_size_px: 200,
                ..AppSettings::default()
            })
            .is_err()
        );
    }

    #[test]
    fn rejects_inverted_chunk_range() {
        assert!(
            validate(&AppSettings {
                minimum_chunk_ms: 10_000,
                maximum_chunk_ms: 5_000,
                ..AppSettings::default()
            })
            .is_err()
        );
    }

    #[test]
    fn rejects_overlap_longer_than_maximum() {
        let defaults = AppSettings::default();
        assert!(
            validate(&AppSettings {
                overlap_ms: defaults.maximum_chunk_ms,
                ..defaults
            })
            .is_err()
        );
    }

    #[test]
    fn rejects_unknown_vad_sensitivity() {
        assert!(
            validate(&AppSettings {
                vad_sensitivity: "extreme".into(),
                ..AppSettings::default()
            })
            .is_err()
        );
    }

    #[test]
    fn rejects_unknown_choice() {
        assert!(
            validate(&AppSettings {
                chunk_mode: "smart-ish".into(),
                ..AppSettings::default()
            })
            .is_err()
        );
    }

    #[test]
    fn rejects_out_of_range_timing() {
        assert!(
            validate(&AppSettings {
                end_silence_ms: 50,
                ..AppSettings::default()
            })
            .is_err()
        );
        assert!(
            validate(&AppSettings {
                overlap_ms: 3_500,
                ..AppSettings::default()
            })
            .is_err()
        );
    }

    #[test]
    fn rejects_invalid_subtitle_color() {
        assert!(
            validate(&AppSettings {
                text_color: "white".into(),
                ..AppSettings::default()
            })
            .is_err()
        );
    }

    #[test]
    fn rejects_opacity_outside_unit_range() {
        assert!(
            validate(&AppSettings {
                background_opacity: 1.5,
                ..AppSettings::default()
            })
            .is_err()
        );
    }

    #[test]
    fn rejects_missing_cookie_file_only_in_file_mode() {
        let with_file = AppSettings {
            cookie_file: "/definitely/not/here.txt".into(),
            ..AppSettings::default()
        };
        // Ignored unless the cookie mode actually uses a file.
        assert!(validate(&with_file).is_ok());
        assert!(
            validate(&AppSettings {
                cookie_mode: "file".into(),
                ..with_file
            })
            .is_err()
        );
    }
}
