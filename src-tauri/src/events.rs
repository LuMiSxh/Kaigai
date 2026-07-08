use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use crate::{settings::AppSettings, state::SessionStatus};

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SessionStateEvent {
    pub state: SessionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SessionErrorEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleEvent {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub inference_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SubtitlePartialEvent {
    pub stable_text: String,
    pub unstable_text: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct SubtitleClearEvent;

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct MetricsEvent {
    pub audio_ms: u64,
    pub chunk_ms: u64,
    pub inference_ms: Option<u64>,
    pub realtime_factor: Option<f64>,
    pub reason: String,
    pub queue_depth: usize,
    pub queue_delay_ms: u64,
    pub pcm_gap_ms: u64,
    pub capture_lag_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdatedEvent {
    pub settings: AppSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadEvent {
    pub model_id: String,
    pub state: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub bytes_per_second: f64,
    pub eta_seconds: Option<u64>,
    pub error: Option<String>,
}

impl ModelDownloadEvent {
    pub fn started(model_id: &str, total_bytes: u64) -> Self {
        Self::new(model_id, "downloading", 0, total_bytes)
    }

    pub fn progress(
        model_id: &str,
        downloaded_bytes: u64,
        total_bytes: u64,
        bytes_per_second: f64,
        eta_seconds: Option<u64>,
    ) -> Self {
        Self {
            model_id: model_id.into(),
            state: "downloading".into(),
            downloaded_bytes,
            total_bytes,
            bytes_per_second,
            eta_seconds,
            error: None,
        }
    }

    pub fn verifying(model_id: &str, downloaded_bytes: u64, total_bytes: u64) -> Self {
        Self::new(model_id, "verifying", downloaded_bytes, total_bytes)
    }

    pub fn installing(model_id: &str, downloaded_bytes: u64, total_bytes: u64) -> Self {
        Self::new(model_id, "installing", downloaded_bytes, total_bytes)
    }

    pub fn ready(model_id: &str, downloaded_bytes: u64, total_bytes: u64) -> Self {
        Self::new(model_id, "ready", downloaded_bytes, total_bytes)
    }

    pub fn cancelled(model_id: &str) -> Self {
        Self::new(model_id, "cancelled", 0, 0)
    }

    pub fn failed(model_id: &str, error: &str) -> Self {
        let mut event = Self::new(model_id, "failed", 0, 0);
        event.error = Some(error.into());
        event
    }

    fn new(model_id: &str, state: &str, downloaded_bytes: u64, total_bytes: u64) -> Self {
        Self {
            model_id: model_id.into(),
            state: state.into(),
            downloaded_bytes,
            total_bytes,
            bytes_per_second: 0.0,
            eta_seconds: None,
            error: None,
        }
    }
}
