use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};

use serde::{Deserialize, Serialize};

use crate::{
    pipeline::{
        audio_window::{AudioWindow, RollingWindow, SAMPLE_RATE, samples_to_ms},
        inference::join_unique_segments,
        quality::CaptionQualityGate,
        stabilizer::Stabilizer,
        vad::{SpeechDetector, sensitivity_threshold},
        whisper::WhisperEngine,
    },
    settings::AppSettings,
};

mod paths;
mod report;

pub use paths::{
    default_manifest_path, default_model_dir, default_output_path, isolated_model_path,
};
pub use report::{RunReport, SummaryRow, write_report};

pub const BENCH_SAMPLE_RATE_HZ: u32 = 16_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusManifest {
    pub sample_rate_hz: u32,
    pub clips: Vec<CorpusClip>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusClip {
    pub id: String,
    pub category: String,
    pub length_profile: String,
    pub speech_density: String,
    pub speaker_profile: String,
    pub speaker_count: u32,
    pub overlap_risk: String,
    pub speaker: String,
    pub title: String,
    pub url: String,
    pub start: String,
    pub duration_seconds: f64,
    pub audio_path: PathBuf,
    #[serde(default)]
    pub source_text: Option<String>,
    #[serde(default)]
    pub reference_text: Option<String>,
    #[serde(default)]
    pub source_dataset: Option<String>,
    #[serde(default)]
    pub noise_profile: Option<String>,
    #[serde(default)]
    pub snr_db: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowRun {
    pub index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub inference_ms: u128,
    pub segments: usize,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emitted_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_window: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speech_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_decision: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipRun {
    pub model: String,
    pub backend: String,
    pub task: String,
    pub decode_mode: String,
    pub clip_id: String,
    pub category: String,
    pub length_profile: String,
    pub speech_density: String,
    pub speaker_profile: String,
    pub speaker_count: u32,
    pub overlap_risk: String,
    pub speaker: String,
    pub duration_ms: u64,
    pub window_ms: Option<u64>,
    pub overlap_ms: Option<u64>,
    pub window_count: usize,
    pub load_ms: u128,
    pub inference_ms: u128,
    pub realtime_factor: f64,
    pub segments: usize,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_dataset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub noise_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snr_db: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub windows: Vec<WindowRun>,
}

pub struct BenchmarkEngine {
    engine: WhisperEngine,
    load_ms: u128,
}

impl BenchmarkEngine {
    /// Loads a benchmark model with an explicit Whisper language token.
    ///
    /// # Errors
    /// Returns an error when the model cannot be loaded.
    pub fn load_with_language(
        model_path: &Path,
        task: &str,
        language: &str,
    ) -> Result<Self, String> {
        let settings = AppSettings {
            model_path: model_path.to_string_lossy().into_owned(),
            source_language: language.into(),
            task: task.into(),
            ..AppSettings::default()
        };
        let started = Instant::now();
        let engine = WhisperEngine::load(&settings, Arc::new(AtomicBool::new(false)))?
            .ok_or("benchmark model path produced no Whisper engine")?;
        Ok(Self {
            engine,
            load_ms: started.elapsed().as_millis(),
        })
    }

    /// Transcribes a whole clip in one call.
    ///
    /// # Errors
    /// Returns an error when Whisper inference fails.
    #[allow(clippy::cast_precision_loss)]
    pub fn run_clip(
        &mut self,
        model: &str,
        backend: &str,
        task: &str,
        clip: &CorpusClip,
        audio: &[f32],
    ) -> Result<ClipRun, String> {
        let started = Instant::now();
        let segments = self.engine.transcribe(audio)?;
        let inference_ms = started.elapsed().as_millis();
        let duration_ms = (audio.len() as u64 * 1_000) / u64::from(BENCH_SAMPLE_RATE_HZ);
        Ok(ClipRun {
            model: model.into(),
            backend: backend.into(),
            task: task.into(),
            decode_mode: "whole".into(),
            clip_id: clip.id.clone(),
            category: clip.category.clone(),
            length_profile: clip.length_profile.clone(),
            speech_density: clip.speech_density.clone(),
            speaker_profile: clip.speaker_profile.clone(),
            speaker_count: clip.speaker_count,
            overlap_risk: clip.overlap_risk.clone(),
            speaker: clip.speaker.clone(),
            duration_ms,
            window_ms: None,
            overlap_ms: None,
            window_count: 1,
            load_ms: self.load_ms,
            inference_ms,
            realtime_factor: inference_ms as f64 / duration_ms.max(1) as f64,
            segments: segments.len(),
            text: segments
                .into_iter()
                .map(|segment| segment.text)
                .collect::<Vec<_>>()
                .join(" "),
            source_text: clip.source_text.clone(),
            reference_text: clip.reference_text.clone(),
            source_dataset: clip.source_dataset.clone(),
            noise_profile: clip.noise_profile.clone(),
            snr_db: clip.snr_db,
            windows: Vec::new(),
        })
    }

    /// Transcribes fixed overlapping windows without VAD.
    ///
    /// # Errors
    /// Returns an error when Whisper inference fails.
    #[allow(clippy::cast_precision_loss, clippy::too_many_arguments)]
    pub fn run_clip_streaming(
        &mut self,
        model: &str,
        backend: &str,
        task: &str,
        clip: &CorpusClip,
        audio: &[f32],
        window_ms: u64,
        overlap_ms: u64,
    ) -> Result<ClipRun, String> {
        let window_samples = ms_to_samples(window_ms).max(1);
        let overlap_samples = ms_to_samples(overlap_ms).min(window_samples.saturating_sub(1));
        let step_samples = window_samples - overlap_samples;
        let duration_ms = (audio.len() as u64 * 1_000) / u64::from(BENCH_SAMPLE_RATE_HZ);
        let mut offset = 0;
        let mut inference_ms = 0;
        let mut segment_count = 0;
        let mut texts = Vec::new();
        let mut window_count = 0;
        let mut windows = Vec::new();

        while offset < audio.len() {
            let end = (offset + window_samples).min(audio.len());
            let started = Instant::now();
            let segments = self.engine.transcribe(&audio[offset..end])?;
            let window_inference_ms = started.elapsed().as_millis();
            inference_ms += window_inference_ms;
            segment_count += segments.len();
            let window_segments = segments.len();
            let text = segments
                .into_iter()
                .map(|segment| segment.text)
                .collect::<Vec<_>>()
                .join(" ");
            texts.push(text.clone());
            windows.push(WindowRun {
                index: window_count,
                start_ms: (offset as u64 * 1_000) / u64::from(BENCH_SAMPLE_RATE_HZ),
                end_ms: (end as u64 * 1_000) / u64::from(BENCH_SAMPLE_RATE_HZ),
                inference_ms: window_inference_ms,
                segments: window_segments,
                text,
                emitted_text: None,
                final_window: None,
                reason: None,
                speech_ms: None,
                quality_decision: None,
            });
            window_count += 1;
            if end == audio.len() {
                break;
            }
            offset += step_samples;
        }

        Ok(ClipRun {
            model: model.into(),
            backend: backend.into(),
            task: task.into(),
            decode_mode: "streaming".into(),
            clip_id: clip.id.clone(),
            category: clip.category.clone(),
            length_profile: clip.length_profile.clone(),
            speech_density: clip.speech_density.clone(),
            speaker_profile: clip.speaker_profile.clone(),
            speaker_count: clip.speaker_count,
            overlap_risk: clip.overlap_risk.clone(),
            speaker: clip.speaker.clone(),
            duration_ms,
            window_ms: Some(window_ms),
            overlap_ms: Some(overlap_ms),
            window_count,
            load_ms: self.load_ms,
            inference_ms,
            realtime_factor: inference_ms as f64 / duration_ms.max(1) as f64,
            segments: segment_count,
            text: join_stable_text(texts),
            source_text: clip.source_text.clone(),
            reference_text: clip.reference_text.clone(),
            source_dataset: clip.source_dataset.clone(),
            noise_profile: clip.noise_profile.clone(),
            snr_db: clip.snr_db,
            windows,
        })
    }

    /// Runs a clip through the production caption pipeline.
    ///
    /// # Errors
    /// Returns an error when VAD or Whisper inference fails.
    #[allow(clippy::cast_precision_loss)]
    pub fn run_clip_pipeline(
        &mut self,
        labels: &RunLabels<'_>,
        clip: &CorpusClip,
        audio: &[f32],
        vad_model_path: &Path,
        config: &PipelineConfig,
    ) -> Result<ClipRun, String> {
        let settings = config.app_settings(labels.task);
        let mut detector = SpeechDetector::from_path(
            vad_model_path,
            sensitivity_threshold(&settings.vad_sensitivity),
        )?;
        let mut rolling = RollingWindow::new(&settings);
        let mut stabilizer = Stabilizer::new(&settings);
        let mut quality_gate = CaptionQualityGate::new(&settings);
        let mut windows = Vec::new();
        let mut emitted = Vec::new();
        let mut inference_ms = 0;
        let mut segment_count = 0;

        for frame in audio.chunks(SAMPLE_RATE / 10) {
            let speaking = detector.is_speech(frame)?;
            for window in rolling.push_with_activity(frame, speaking) {
                self.run_pipeline_window(
                    &window,
                    &mut stabilizer,
                    &mut quality_gate,
                    &mut windows,
                    &mut emitted,
                    &mut inference_ms,
                    &mut segment_count,
                )?;
            }
        }
        if let Some(window) = rolling.flush() {
            self.run_pipeline_window(
                &window,
                &mut stabilizer,
                &mut quality_gate,
                &mut windows,
                &mut emitted,
                &mut inference_ms,
                &mut segment_count,
            )?;
        }

        let duration_ms = samples_to_ms(audio.len());
        Ok(ClipRun {
            model: labels.model.into(),
            backend: labels.backend.into(),
            task: labels.task.into(),
            decode_mode: "pipeline".into(),
            clip_id: clip.id.clone(),
            category: clip.category.clone(),
            length_profile: clip.length_profile.clone(),
            speech_density: clip.speech_density.clone(),
            speaker_profile: clip.speaker_profile.clone(),
            speaker_count: clip.speaker_count,
            overlap_risk: clip.overlap_risk.clone(),
            speaker: clip.speaker.clone(),
            duration_ms,
            window_ms: Some(u64::from(settings.maximum_chunk_ms)),
            overlap_ms: Some(u64::from(settings.overlap_ms)),
            window_count: windows.len(),
            load_ms: self.load_ms,
            inference_ms,
            realtime_factor: inference_ms as f64 / duration_ms.max(1) as f64,
            segments: segment_count,
            text: join_stable_text(emitted),
            source_text: clip.source_text.clone(),
            reference_text: clip.reference_text.clone(),
            source_dataset: clip.source_dataset.clone(),
            noise_profile: clip.noise_profile.clone(),
            snr_db: clip.snr_db,
            windows,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn run_pipeline_window(
        &mut self,
        window: &AudioWindow,
        stabilizer: &mut Stabilizer,
        quality_gate: &mut CaptionQualityGate,
        windows: &mut Vec<WindowRun>,
        emitted: &mut Vec<String>,
        total_inference_ms: &mut u128,
        total_segments: &mut usize,
    ) -> Result<(), String> {
        let started = Instant::now();
        let segments = self.engine.transcribe(&window.samples)?;
        let inference_ms = started.elapsed().as_millis();
        *total_inference_ms += inference_ms;
        *total_segments += segments.len();
        let text = join_unique_segments(&segments);
        let decision = quality_gate.evaluate_inference_text(
            &text,
            window.final_window,
            window.speech_ms,
            segments.len(),
        );
        let mut emitted_text = None;
        let quality_decision = if !decision.allow {
            decision.reason
        } else if window.final_window {
            let final_text = stabilizer.finalize(&text);
            if final_text.is_empty() {
                "stabilizer-duplicate"
            } else {
                emitted.push(final_text.clone());
                emitted_text = Some(final_text);
                quality_gate.accept_final();
                "emitted-final"
            }
        } else {
            let hypothesis = stabilizer.update(&text);
            quality_gate
                .evaluate_partial(&hypothesis.stable, &hypothesis.unstable)
                .reason
        };
        windows.push(WindowRun {
            index: windows.len(),
            start_ms: samples_to_ms(window.start_sample),
            end_ms: samples_to_ms(window.start_sample + window.samples.len()),
            inference_ms,
            segments: segments.len(),
            text,
            emitted_text,
            final_window: Some(window.final_window),
            reason: Some(window.reason.into()),
            speech_ms: Some(window.speech_ms),
            quality_decision: Some(quality_decision.into()),
        });
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RunLabels<'a> {
    pub model: &'a str,
    pub backend: &'a str,
    pub task: &'a str,
}

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub vad_sensitivity: String,
    pub minimum_chunk_ms: u32,
    pub maximum_chunk_ms: u32,
    pub end_silence_ms: u32,
    pub overlap_ms: u32,
}

impl PipelineConfig {
    fn app_settings(&self, task: &str) -> AppSettings {
        AppSettings {
            task: task.into(),
            caption_mode: "stable".into(),
            chunk_mode: "adaptive".into(),
            vad_sensitivity: self.vad_sensitivity.clone(),
            minimum_chunk_ms: self.minimum_chunk_ms,
            maximum_chunk_ms: self.maximum_chunk_ms,
            end_silence_ms: self.end_silence_ms,
            overlap_ms: self.overlap_ms,
            ..AppSettings::default()
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        let settings = AppSettings::default();
        Self {
            vad_sensitivity: settings.vad_sensitivity,
            minimum_chunk_ms: settings.minimum_chunk_ms,
            maximum_chunk_ms: settings.maximum_chunk_ms,
            end_silence_ms: settings.end_silence_ms,
            overlap_ms: settings.overlap_ms,
        }
    }
}

fn ms_to_samples(ms: u64) -> usize {
    ((ms * u64::from(BENCH_SAMPLE_RATE_HZ)) / 1_000) as usize
}

fn join_stable_text(texts: Vec<String>) -> String {
    let mut output = Vec::new();
    for text in texts {
        if output.last() == Some(&text) {
            continue;
        }
        output.push(text);
    }
    output.join(" ")
}

/// Reads a 16 kHz benchmark manifest.
///
/// # Errors
/// Returns an error for unreadable, invalid, or mismatched input.
pub fn read_manifest(path: &Path) -> Result<CorpusManifest, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let manifest: CorpusManifest =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if manifest.sample_rate_hz != BENCH_SAMPLE_RATE_HZ {
        return Err(format!(
            "expected {BENCH_SAMPLE_RATE_HZ} Hz corpus, got {} Hz",
            manifest.sample_rate_hz
        ));
    }
    Ok(manifest)
}

/// Reads mono 16 kHz PCM into normalized samples.
///
/// # Errors
/// Returns an error for unreadable or unsupported WAV files.
pub fn read_wav_mono_16k(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|error| error.to_string())?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != BENCH_SAMPLE_RATE_HZ
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err(format!(
            "expected mono {BENCH_SAMPLE_RATE_HZ} Hz 16-bit PCM WAV, got {} channels, {} Hz, {} bits, {:?}",
            spec.channels, spec.sample_rate, spec.bits_per_sample, spec.sample_format
        ));
    }

    reader
        .samples::<i16>()
        .map(|sample| {
            sample
                .map(|value| f32::from(value) / 32_768.0)
                .map_err(|error| error.to_string())
        })
        .collect()
}
