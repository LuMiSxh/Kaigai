//! Structured application logging with a bounded developer-console buffer.

use std::io;

#[cfg(debug_assertions)]
use std::{
    collections::VecDeque,
    io::Write,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use specta::Type;
use tracing_appender::non_blocking::WorkerGuard;
#[cfg(not(debug_assertions))]
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(not(debug_assertions))]
const APP_IDENTIFIER: &str = "com.lumisxh.kaigai";
#[cfg(not(debug_assertions))]
const LOG_MAX_FILES: usize = 7;
#[cfg(debug_assertions)]
const MEMORY_LOG_LIMIT: usize = 500;
const DEFAULT_FILTER_DEV: &str = "info,kaigai_lib=debug";
const DEFAULT_FILTER_RELEASE: &str = "info";

#[cfg(debug_assertions)]
static MEMORY_LOGS: OnceLock<Arc<Mutex<VecDeque<DeveloperLogEntry>>>> = OnceLock::new();
#[cfg(debug_assertions)]
static NEXT_LOG_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeveloperLogEntry {
    pub id: u64,
    pub message: String,
}

pub fn init() -> Option<WorkerGuard> {
    #[cfg(debug_assertions)]
    {
        let memory = MEMORY_LOGS
            .get_or_init(|| Arc::new(Mutex::new(VecDeque::with_capacity(MEMORY_LOG_LIMIT))))
            .clone();
        init_stderr(true, memory)
    }

    #[cfg(not(debug_assertions))]
    {
        let Some(log_dir) = log_dir() else {
            return init_stderr(false);
        };
        if std::fs::create_dir_all(&log_dir).is_err() {
            return init_stderr(false);
        }

        let Ok(appender) = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("kaigai")
            .filename_suffix("log")
            .max_log_files(LOG_MAX_FILES)
            .build(log_dir)
        else {
            return init_stderr(false);
        };
        let (writer, guard) = tracing_appender::non_blocking(appender);
        tracing_subscriber::registry()
            .with(filter())
            .with(fmt::layer().with_writer(writer).with_ansi(false))
            .init();
        Some(guard)
    }
}

/// Fallback subscriber used whenever a rolling file appender cannot be created.
#[cfg(debug_assertions)]
fn init_stderr(ansi: bool, memory: Arc<Mutex<VecDeque<DeveloperLogEntry>>>) -> Option<WorkerGuard> {
    tracing_subscriber::registry()
        .with(filter())
        .with(fmt::layer().with_writer(io::stderr).with_ansi(ansi))
        .with(memory_layer(memory))
        .init();
    None
}

#[cfg(not(debug_assertions))]
fn init_stderr(ansi: bool) -> Option<WorkerGuard> {
    tracing_subscriber::registry()
        .with(filter())
        .with(fmt::layer().with_writer(io::stderr).with_ansi(ansi))
        .init();
    None
}

#[cfg(debug_assertions)]
pub fn recent(after_id: Option<u64>) -> Vec<DeveloperLogEntry> {
    let after_id = after_id.unwrap_or(0);
    MEMORY_LOGS
        .get()
        .and_then(|logs| {
            logs.lock().ok().map(|logs| {
                logs.iter()
                    .filter(|entry| entry.id > after_id)
                    .cloned()
                    .collect()
            })
        })
        .unwrap_or_default()
}

#[cfg(not(debug_assertions))]
pub fn recent(_after_id: Option<u64>) -> Vec<DeveloperLogEntry> {
    Vec::new()
}

/// Redact secrets before a log line reaches disk, the dev console, or a
/// diagnostics event. Called here and again in the in-memory writer (separate
/// sinks); idempotent, so the double pass is harmless.
pub fn sanitize(message: &str) -> String {
    if message.contains("Cookie:") || message.contains("Authorization:") {
        return "[redacted authentication data]".into();
    }
    message
        .split_whitespace()
        .map(redact_token)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Strip the query string from any URL-like token — signed media URLs (e.g.
/// `googlevideo.com`) carry credentials there. Keep the host for context.
fn redact_token(token: &str) -> String {
    if token.contains("://")
        && let Some(index) = token.find('?')
    {
        return format!("{}?[redacted-query]", &token[..index]);
    }
    token.to_owned()
}

#[cfg(not(debug_assertions))]
fn log_dir() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|directory| directory.join(APP_IDENTIFIER).join("logs"))
}

fn filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(if cfg!(debug_assertions) {
            DEFAULT_FILTER_DEV
        } else {
            DEFAULT_FILTER_RELEASE
        })
    })
}

#[cfg(debug_assertions)]
fn memory_layer<S>(
    memory: Arc<Mutex<VecDeque<DeveloperLogEntry>>>,
) -> tracing_subscriber::fmt::Layer<
    S,
    tracing_subscriber::fmt::format::DefaultFields,
    tracing_subscriber::fmt::format::Format,
    MemoryMakeWriter,
>
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(MemoryMakeWriter { memory })
}

#[cfg(debug_assertions)]
#[derive(Clone)]
struct MemoryMakeWriter {
    memory: Arc<Mutex<VecDeque<DeveloperLogEntry>>>,
}

#[cfg(debug_assertions)]
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for MemoryMakeWriter {
    type Writer = MemoryWriter;

    fn make_writer(&'a self) -> Self::Writer {
        MemoryWriter {
            memory: self.memory.clone(),
        }
    }
}

#[cfg(debug_assertions)]
struct MemoryWriter {
    memory: Arc<Mutex<VecDeque<DeveloperLogEntry>>>,
}

#[cfg(debug_assertions)]
impl Write for MemoryWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let message = sanitize(&String::from_utf8_lossy(buffer));
        if let Ok(mut logs) = self.memory.lock() {
            for line in message.lines().filter(|line| !line.trim().is_empty()) {
                if logs.len() == MEMORY_LOG_LIMIT {
                    logs.pop_front();
                }
                logs.push_back(DeveloperLogEntry {
                    id: NEXT_LOG_ID.fetch_add(1, Ordering::Relaxed),
                    message: line.to_owned(),
                });
            }
        }
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn redacts_authentication_headers_wholesale() {
        assert_eq!(
            sanitize("request Cookie: SID=secret; HSID=more"),
            "[redacted authentication data]"
        );
        assert_eq!(
            sanitize("Authorization: Bearer abc.def"),
            "[redacted authentication data]"
        );
    }

    #[test]
    fn strips_query_string_from_any_url() {
        assert_eq!(
            sanitize("fetching https://r1.googlevideo.com/videoplayback?sig=SECRET&ip=1.2.3.4"),
            "fetching https://r1.googlevideo.com/videoplayback?[redacted-query]"
        );
        assert_eq!(
            sanitize("https://cdn.example.com/seg.ts?token=abc"),
            "https://cdn.example.com/seg.ts?[redacted-query]"
        );
    }

    #[test]
    fn leaves_plain_text_and_query_free_urls_untouched() {
        assert_eq!(sanitize("decoded 320 frames ok"), "decoded 320 frames ok");
        assert_eq!(
            sanitize("opening https://example.com/path"),
            "opening https://example.com/path"
        );
    }
}
