use std::time::Instant;

use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use super::{
    Signals,
    audio_window::samples_to_ms,
    quality::CaptionQualityGate,
    queue::{ChunkQueue, QueuedWindow},
    stabilizer::{Stabilizer, squash},
    whisper::{WhisperEngine, WhisperSegment},
};
use crate::{
    events::{MetricsEvent, SubtitleEvent, SubtitlePartialEvent},
    state::{AppFeed, AppState},
};

// u128 -> u64 truncation is a non-issue below 2^52ms (~142,000 years); these
// are display-metric millis, not real durations.
//
// Long on purpose: one linear pipeline stage (no engine -> metrics only, else
// transcribe -> quality gate -> finalize/partial). Splitting it up would trade
// a readable sequence for helpers that all share most of these parameters.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::too_many_lines
)]
pub fn process(
    app: &AppHandle,
    whisper: &mut Option<WhisperEngine>,
    queued: QueuedWindow,
    queue: &ChunkQueue,
    signals: &Signals,
    stabilizer: &mut Stabilizer,
    quality_gate: &mut CaptionQualityGate,
) -> Result<(), String> {
    let window = queued.window;
    let chunk_ms = samples_to_ms(window.samples.len());
    let audio_ms = samples_to_ms(window.start_sample);
    let queue_delay_ms = queued.queued_at.elapsed().as_millis() as u64;
    let capture_lag_ms = signals.capture_lag_ms(window.start_sample + window.samples.len());

    let Some(engine) = whisper else {
        return emit_metrics(
            app,
            &MetricsEvent {
                audio_ms,
                chunk_ms,
                inference_ms: None,
                realtime_factor: None,
                reason: window.reason.into(),
                queue_depth: queue.depth(),
                queue_delay_ms,
                pcm_gap_ms: window.pcm_gap_ms,
                capture_lag_ms,
            },
        );
    };

    let started = Instant::now();
    let segments = engine.transcribe(&window.samples)?;
    let inference_ms = started.elapsed().as_millis() as u64;
    let realtime_factor = inference_ms as f64 / chunk_ms.max(1) as f64;
    tracing::debug!(
        chunk_ms,
        speech_ms = window.speech_ms,
        inference_ms,
        realtime_factor,
        reason = window.reason,
        final_window = window.final_window,
        queue_delay_ms,
        pcm_gap_ms = window.pcm_gap_ms,
        capture_lag_ms,
        segments = segments.len(),
        "inference window completed"
    );
    emit_metrics(
        app,
        &MetricsEvent {
            audio_ms,
            chunk_ms,
            inference_ms: Some(inference_ms),
            realtime_factor: Some(realtime_factor),
            reason: window.reason.into(),
            queue_depth: queue.depth(),
            queue_delay_ms,
            pcm_gap_ms: window.pcm_gap_ms,
            capture_lag_ms,
        },
    )?;

    let text = join_unique_segments(&segments);
    let quality = quality_gate.evaluate_inference_text(
        &text,
        window.final_window,
        window.speech_ms,
        segments.len(),
    );
    if !quality.allow {
        tracing::debug!(
            reason = quality.reason,
            final_window = window.final_window,
            speech_ms = window.speech_ms,
            segments = segments.len(),
            "caption quality gate rejected inference output"
        );
        return Ok(());
    }

    if window.final_window {
        let final_text = stabilizer.finalize(&text);
        if final_text.is_empty() {
            return Ok(()); // exact repeat of the previous line — skip it
        }
        let start_ms =
            audio_ms.saturating_add(segments.first().map_or(0, |segment| segment.start_ms));
        let end_ms =
            audio_ms.saturating_add(segments.last().map_or(chunk_ms, |segment| segment.end_ms));
        app.state::<AppState>().send_feed(AppFeed::Subtitle {
            start_ms,
            end_ms,
            text: final_text.clone(),
        });
        quality_gate.accept_final();
        SubtitleEvent {
            start_ms,
            end_ms,
            text: final_text,
            inference_ms,
        }
        .emit(app)
        .map_err(|error| error.to_string())?;
    } else {
        let hypothesis = stabilizer.update(&text);
        let partial_quality =
            quality_gate.evaluate_partial(&hypothesis.stable, &hypothesis.unstable);
        if !partial_quality.allow {
            tracing::debug!(
                reason = partial_quality.reason,
                stable_empty = hypothesis.stable.is_empty(),
                unstable_empty = hypothesis.unstable.is_empty(),
                "caption quality gate skipped partial update"
            );
            return Ok(());
        }
        app.state::<AppState>().send_feed(AppFeed::Partial {
            stable_text: hypothesis.stable.clone(),
            unstable_text: hypothesis.unstable.clone(),
        });
        emit_partial(app, stabilizer, &hypothesis.stable, &hypothesis.unstable)?;
    }
    Ok(())
}

fn emit_metrics(app: &AppHandle, event: &MetricsEvent) -> Result<(), String> {
    event.emit(app).map_err(|error| error.to_string())
}

/// Drop adjacent duplicate segments — Whisper repetition-loops on long
/// windows re-emit the same phrase across several segments.
fn join_unique_segments(segments: &[WhisperSegment]) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for segment in segments {
        let part = segment.text.trim();
        if part.is_empty() {
            continue;
        }
        if parts
            .last()
            .is_some_and(|previous| squash(previous) == squash(part))
        {
            continue;
        }
        parts.push(part);
    }
    parts.join(" ")
}

fn emit_partial(
    app: &AppHandle,
    stabilizer: &Stabilizer,
    stable_text: &str,
    unstable_text: &str,
) -> Result<(), String> {
    SubtitlePartialEvent {
        stable_text: stable_text.into(),
        unstable_text: unstable_text.into(),
        revision: stabilizer.revision(),
    }
    .emit(app)
    .map_err(|error| error.to_string())
}
