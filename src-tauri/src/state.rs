use std::{
    sync::{Arc, Mutex, atomic::AtomicBool},
    thread::JoinHandle,
};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::ipc::Channel;

use crate::settings::AppSettings;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Idle,
    Starting,
    Loading,
    Buffering,
    Running,
    Reconnecting,
    Stopping,
    Failed,
}

/// Messages pushed to the bar over a persistent [`Channel`] it registers at
/// startup — same IPC path as commands, so delivery to the (transparent) bar
/// window is reliable where broadcast events were not.
///
/// No `rename_all`: specta doesn't apply it to tagged-enum variant fields, so
/// both sides stay `snake_case` to avoid a type/runtime mismatch.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "type")]
pub enum AppFeed {
    State {
        state: SessionStatus,
    },
    Subtitle {
        start_ms: u64,
        end_ms: u64,
        text: String,
    },
    Partial {
        stable_text: String,
        unstable_text: String,
    },
    Clear,
    Error {
        message: String,
    },
    Settings {
        settings: Box<AppSettings>,
    },
}

pub struct AppState {
    pub settings: Mutex<AppSettings>,
    pub session: Mutex<SessionData>,
    pub model_download: Mutex<Option<Arc<AtomicBool>>>,
    /// Tracks an in-progress yt-dlp managed install/update, separately from
    /// `model_download` since the two can't meaningfully block each other.
    pub tool_download: Mutex<Option<Arc<AtomicBool>>>,
    /// The bar's live channel, registered once via `connect_feed`.
    pub feed: Mutex<Option<Channel<AppFeed>>>,
}

pub struct SessionData {
    pub status: SessionStatus,
    pub stream_url: Option<String>,
    pub cancel: Option<Arc<AtomicBool>>,
    pub generation: u64,
    /// Pipeline worker thread, joined on shutdown so Whisper/Metal frees cleanly.
    pub worker: Option<JoinHandle<()>>,
}

impl AppState {
    /// Push a message to the bar's channel if one is connected.
    pub fn send_feed(&self, message: AppFeed) {
        if let Ok(feed) = self.feed.lock()
            && let Some(channel) = feed.as_ref()
        {
            let _ = channel.send(message);
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            settings: Mutex::new(AppSettings::default()),
            model_download: Mutex::new(None),
            tool_download: Mutex::new(None),
            feed: Mutex::new(None),
            session: Mutex::new(SessionData {
                status: SessionStatus::Idle,
                stream_url: None,
                cancel: None,
                generation: 0,
                worker: None,
            }),
        }
    }
}
