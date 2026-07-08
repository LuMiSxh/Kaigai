use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use self::{
    audio_window::{AudioWindow, RollingWindow, SAMPLE_RATE, decode_pcm_into, samples_to_ms},
    inference::process,
    media::MediaProcess,
    quality::CaptionQualityGate,
    queue::{ChunkQueue, Recv},
    stabilizer::Stabilizer,
    vad::SpeechDetector,
    whisper::WhisperEngine,
};
use crate::{
    events::{DiagnosticsEvent, SessionStateEvent},
    settings::AppSettings,
    state::{AppFeed, AppState, SessionStatus},
};

mod audio_window;
mod inference;
mod media;
mod quality;
mod queue;
mod stabilizer;
mod vad;
pub(crate) mod whisper;

/// PCM bytes read per syscall (~100 ms of 16 kHz mono `s16le`).
const BYTES_PER_READ: usize = SAMPLE_RATE / 10 * 2;
/// How long the consumer waits for a chunk before re-checking stop flags.
const RECV_POLL: Duration = Duration::from_millis(200);
/// How often the watchdog checks whether the stream has stalled.
const WATCHDOG_POLL: Duration = Duration::from_millis(500);
/// No PCM for this long while running marks the session as reconnecting.
const STALL_TIMEOUT: Duration = Duration::from_secs(5);

/// Flags shared between the capture, watchdog, and inference threads.
pub(super) struct Signals {
    /// Set by the user via `stop_session`.
    cancel: Arc<AtomicBool>,
    /// Set internally to tear the pipeline down on an inference error.
    abort: AtomicBool,
    /// Set by the reader once capture has finished.
    finished: AtomicBool,
    /// True while the watchdog considers the stream stalled.
    stalled: AtomicBool,
    /// Milliseconds (relative to `epoch`) of the last received PCM data.
    last_data_ms: AtomicU64,
    /// Milliseconds of the first received PCM data, used to estimate capture lag.
    first_data_ms: AtomicU64,
    epoch: Instant,
}

impl Signals {
    fn new(cancel: Arc<AtomicBool>) -> Self {
        Self {
            cancel,
            abort: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            stalled: AtomicBool::new(false),
            last_data_ms: AtomicU64::new(0),
            first_data_ms: AtomicU64::new(0),
            epoch: Instant::now(),
        }
    }

    fn stop_requested(&self) -> bool {
        self.cancel.load(Ordering::Relaxed) || self.abort.load(Ordering::Relaxed)
    }

    // Elapsed-millis (u128 -> u64) truncates only after ~584 million years of
    // continuous capture; never a real concern for a live transcription session.
    #[allow(clippy::cast_possible_truncation)]
    fn capture_lag_ms(&self, audio_end_sample: usize) -> u64 {
        let first = self.first_data_ms.load(Ordering::Relaxed);
        if first == 0 {
            return 0;
        }
        let wall_audio_ms = self
            .epoch
            .elapsed()
            .as_millis()
            .saturating_sub(u128::from(first)) as u64;
        wall_audio_ms.saturating_sub(samples_to_ms(audio_end_sample))
    }
}

// `app` and `settings` are cloned into the spawned 'static reader/watchdog
// threads below; owning them here avoids deref-clone gymnastics for a
// one-time, non-hot-path clone of cheap-to-clone types.
#[allow(clippy::needless_pass_by_value)]
pub fn run(
    app: AppHandle,
    stream_url: String,
    settings: AppSettings,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    tracing::debug!(
        chunk_mode = %settings.chunk_mode,
        model_configured = !settings.model_path.trim().is_empty(),
        "initializing media pipeline"
    );
    let (mut media_process, pcm) = MediaProcess::spawn(&app, &stream_url, &settings)?;
    // Loading the Whisper model (especially the larger ones) takes a few seconds
    // and happens on this background thread, so surface a distinct state the UI
    // can show a spinner for instead of an opaque "starting".
    if !settings.model_path.trim().is_empty() {
        set_state(&app, SessionStatus::Loading)?;
    }
    let mut whisper = WhisperEngine::load(&settings, cancel.clone())?;
    let speech_detector = SpeechDetector::load(&app, &settings)?;
    tracing::info!(
        whisper_enabled = whisper.is_some(),
        vad = "silero-v6.2.0",
        vad_sensitivity = %settings.vad_sensitivity,
        "media pipeline is receiving PCM"
    );
    set_state(&app, SessionStatus::Buffering)?;

    let signals = Arc::new(Signals::new(cancel));
    let queue = Arc::new(ChunkQueue::new());
    let settings_clone = settings.clone();

    let reader = thread::spawn({
        let app = app.clone();
        let signals = signals.clone();
        let queue = queue.clone();
        move || {
            read_pcm(
                &app,
                pcm,
                &settings_clone,
                &signals,
                &queue,
                speech_detector,
            )
        }
    });
    let watchdog = thread::spawn({
        let app = app.clone();
        let signals = signals.clone();
        move || watch_for_stall(&app, &signals)
    });

    // Inference consumer: drains chunks as fast as Whisper allows while staying
    // responsive to cancellation between chunks.
    let mut result = Ok(());
    let mut stabilizer = Stabilizer::new(&settings);
    let mut quality_gate = CaptionQualityGate::new(&settings);
    loop {
        if signals.stop_requested() {
            break;
        }
        match queue.recv_timeout(RECV_POLL) {
            Recv::Window(queued) => {
                if let Err(error) = process(
                    &app,
                    &mut whisper,
                    queued,
                    &queue,
                    &signals,
                    &mut stabilizer,
                    &mut quality_gate,
                ) {
                    result = Err(error);
                    signals.abort.store(true, Ordering::Relaxed);
                    break;
                }
            }
            Recv::Empty => {}
            Recv::Closed => break,
        }
    }

    signals.abort.store(true, Ordering::Relaxed);
    queue.close();
    media_process.stop(); // unblock a reader parked in a blocking read
    let read_result = reader
        .join()
        .unwrap_or_else(|_| Err("PCM reader thread panicked".into()));
    let _ = watchdog.join();

    if signals.cancel.load(Ordering::Relaxed) {
        return Ok(());
    }
    result?;
    read_result?;
    media_process.verify_exit()
}

// Elapsed-millis (u128 -> u64) truncates only after ~584 million years of
// continuous capture; never a real concern for a live transcription session.
#[allow(clippy::cast_possible_truncation)]
fn read_pcm(
    app: &AppHandle,
    mut pcm: impl Read,
    settings: &AppSettings,
    signals: &Signals,
    queue: &ChunkQueue,
    mut speech_detector: SpeechDetector,
) -> Result<(), String> {
    let mut windows = RollingWindow::new(settings);
    let mut bytes = vec![0_u8; BYTES_PER_READ];
    let mut decoded: Vec<f32> = Vec::with_capacity(BYTES_PER_READ / 2);
    let mut running = false;
    let mut read_error = None;
    let mut last_read = Instant::now();

    loop {
        if signals.stop_requested() {
            break;
        }
        let read = match pcm.read(&mut bytes) {
            Ok(read) => read,
            Err(error) => {
                read_error = Some(format!("failed reading ffmpeg PCM: {error}"));
                break;
            }
        };
        if read == 0 {
            break;
        }
        let pcm_gap_ms = last_read.elapsed().as_millis() as u64;
        last_read = Instant::now();
        mark_alive(app, signals, &mut running);

        decode_pcm_into(&bytes[..read], &mut decoded);
        let speaking = speech_detector.is_speech(&decoded)?;
        for mut window in windows.push_with_activity(&decoded, speaking) {
            window.pcm_gap_ms = pcm_gap_ms;
            enqueue(app, queue, window);
        }
    }

    if !signals.stop_requested()
        && let Some(window) = windows.flush()
    {
        enqueue(app, queue, window);
    }

    signals.finished.store(true, Ordering::Relaxed);
    queue.close();
    read_error.map_or(Ok(()), Err)
}

/// Record that PCM is flowing, transitioning to `Running` on first data and
/// recovering from a watchdog-detected stall.
// Elapsed-millis (u128 -> u64) truncates only after ~584 million years of
// continuous capture; never a real concern for a live transcription session.
#[allow(clippy::cast_possible_truncation)]
fn mark_alive(app: &AppHandle, signals: &Signals, running: &mut bool) {
    let now = signals.epoch.elapsed().as_millis() as u64;
    signals.last_data_ms.store(now, Ordering::Relaxed);
    let _ =
        signals
            .first_data_ms
            .compare_exchange(0, now.max(1), Ordering::Relaxed, Ordering::Relaxed);
    if !*running {
        let _ = set_state(app, SessionStatus::Running);
        *running = true;
    } else if signals.stalled.swap(false, Ordering::Relaxed) {
        let _ = DiagnosticsEvent {
            message: "[pipeline] stream resumed".into(),
        }
        .emit(app);
        let _ = set_state(app, SessionStatus::Running);
    }
}

// Elapsed-millis (u128 -> u64) truncates only after ~584 million years of
// continuous capture; never a real concern for a live transcription session.
#[allow(clippy::cast_possible_truncation)]
fn watch_for_stall(app: &AppHandle, signals: &Signals) {
    while !signals.stop_requested() && !signals.finished.load(Ordering::Relaxed) {
        thread::sleep(WATCHDOG_POLL);
        let last = signals.last_data_ms.load(Ordering::Relaxed);
        if last == 0 {
            continue; // no data has arrived yet; still buffering
        }
        let elapsed = signals.epoch.elapsed().as_millis() as u64 - last;
        if elapsed >= STALL_TIMEOUT.as_millis() as u64
            && !signals.stalled.swap(true, Ordering::Relaxed)
        {
            tracing::warn!(stall_ms = elapsed, "stream stalled, awaiting more PCM");
            let _ = DiagnosticsEvent {
                message: "[pipeline] no audio received, reconnecting".into(),
            }
            .emit(app);
            let _ = set_state(app, SessionStatus::Reconnecting);
        }
    }
}

fn enqueue(app: &AppHandle, queue: &ChunkQueue, window: AudioWindow) {
    let replaced = queue.push(window);
    if replaced > 0 {
        tracing::debug!(replaced, "replaced stale rolling inference window");
        let _ = DiagnosticsEvent {
            message: format!("[pipeline] replaced {replaced} stale rolling window(s)"),
        }
        .emit(app);
    }
}

fn set_state(app: &AppHandle, status: SessionStatus) -> Result<(), String> {
    let managed = app.state::<AppState>();
    managed
        .session
        .lock()
        .map_err(|_| "session lock poisoned")?
        .status = status;
    managed.send_feed(AppFeed::State { state: status });
    SessionStateEvent { state: status }
        .emit(app)
        .map_err(|error| error.to_string())
}
