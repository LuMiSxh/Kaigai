use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use whisper_rs::{WhisperVadContext, WhisperVadContextParams};

use crate::settings::AppSettings;

const MODEL_FILE: &str = "ggml-silero-v6.2.0.bin";
const VAD_FRAME_SAMPLES: usize = 512;
/// Silero carries recurrent state within one call. whisper.cpp 1.8 resets that
/// state between calls, so each 100ms decision is evaluated with one second of
/// acoustic history and we use the probability of the newest 32ms frame.
const CONTEXT_SAMPLES: usize = VAD_FRAME_SAMPLES * 32;

pub struct SpeechDetector {
    context: WhisperVadContext,
    history: Vec<f32>,
    threshold: f32,
}

impl SpeechDetector {
    pub fn load(app: &AppHandle, settings: &AppSettings) -> Result<Self, String> {
        let model = model_path(app)?;
        Self::from_path(&model, sensitivity_threshold(&settings.vad_sensitivity))
    }

    pub(crate) fn from_path(model: &Path, threshold: f32) -> Result<Self, String> {
        let model = model
            .to_str()
            .ok_or("Silero VAD model path is not valid UTF-8")?;
        let context = WhisperVadContext::new(model, WhisperVadContextParams::default())
            .map_err(|error| format!("failed to load Silero VAD model: {error}"))?;
        Ok(Self {
            context,
            history: Vec::with_capacity(CONTEXT_SAMPLES),
            threshold,
        })
    }

    pub fn is_speech(&mut self, samples: &[f32]) -> Result<bool, String> {
        self.history.extend(samples.iter().copied());
        if self.history.len() > CONTEXT_SAMPLES {
            let excess = self.history.len() - CONTEXT_SAMPLES;
            self.history.drain(..excess);
        }
        // Never let whisper.cpp zero-pad the newest VAD frame: a partial last
        // frame would make a real voice look artificially quiet. Drop only the
        // oldest remainder so the newest sample stays aligned.
        let usable = self.history.len() / VAD_FRAME_SAMPLES * VAD_FRAME_SAMPLES;
        if usable == 0 {
            return Ok(false);
        }
        let aligned = &self.history[self.history.len() - usable..];
        self.context
            .detect_speech(aligned)
            .map_err(|error| format!("Silero VAD inference failed: {error}"))?;
        Ok(self
            .context
            .probabilities()
            .last()
            .is_some_and(|probability| *probability >= self.threshold))
    }
}

pub(crate) fn sensitivity_threshold(sensitivity: &str) -> f32 {
    match sensitivity {
        "high" => 0.35,
        "strict" => 0.65,
        _ => 0.50,
    }
}

fn model_path(app: &AppHandle) -> Result<PathBuf, String> {
    let resources = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?;
    [
        resources.join("models").join(MODEL_FILE),
        resources.join("resources").join("models").join(MODEL_FILE),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .ok_or_else(|| format!("bundled Silero VAD model is missing: {MODEL_FILE}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_user_facing_sensitivity_to_probability_thresholds() {
        assert!(sensitivity_threshold("high") < sensitivity_threshold("balanced"));
        assert!(sensitivity_threshold("strict") > sensitivity_threshold("balanced"));
    }
}
