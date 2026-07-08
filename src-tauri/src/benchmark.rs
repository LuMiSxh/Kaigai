use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};

use serde::{Deserialize, Serialize};

use crate::{pipeline::whisper::WhisperEngine, settings::AppSettings};

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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub windows: Vec<WindowRun>,
}

pub struct BenchmarkEngine {
    engine: WhisperEngine,
    load_ms: u128,
}

impl BenchmarkEngine {
    pub fn load(model_path: &Path, task: &str) -> Result<Self, String> {
        let settings = AppSettings {
            model_path: model_path.to_string_lossy().into_owned(),
            source_language: "ja".into(),
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
            windows: Vec::new(),
        })
    }

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
            windows,
        })
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
