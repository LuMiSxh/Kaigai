use std::collections::VecDeque;

use crate::settings::AppSettings;

pub const SAMPLE_RATE: usize = 16_000;

const FRAME_SAMPLES: usize = SAMPLE_RATE / 50;
#[cfg(test)]
const SPEECH_RMS_THRESHOLD: f32 = 0.008;
/// Require sustained VAD activity before opening an utterance — a stray 100ms
/// positive over music used to open one and then decode mostly silence.
const SPEECH_START_MS: u32 = 240;
/// Cadence for re-decoding the growing window after the first hypothesis.
/// 1.5s keeps captions updating without re-running Whisper every second.
const PARTIAL_STEP_MS: u32 = 1_500;

pub struct AudioWindow {
    pub start_sample: usize,
    pub reason: &'static str,
    pub final_window: bool,
    pub pcm_gap_ms: u64,
    pub speech_ms: u64,
    pub samples: Vec<f32>,
}

pub struct RollingWindow {
    maximum_samples: usize,
    minimum_samples: usize,
    end_silence_samples: usize,
    overlap_samples: usize,
    step_samples: usize,
    speech_start_samples: usize,
    fixed: bool,
    emit_rolling: bool,
    buffer: VecDeque<f32>,
    buffer_start_sample: usize,
    total_samples: usize,
    last_emit_sample: usize,
    speech_started: bool,
    /// Audio collected since speech actually began for the current utterance.
    /// Retained overlap and pre-roll must not satisfy the minimum duration.
    utterance_samples: usize,
    /// Samples Silero classified as speech in the current utterance.
    speech_samples: usize,
    /// Consecutive speech waiting to pass the start debounce.
    pending_speech_samples: usize,
    trailing_silence: usize,
}

impl RollingWindow {
    pub fn new(settings: &AppSettings) -> Self {
        Self {
            maximum_samples: ms_to_samples(settings.maximum_chunk_ms),
            minimum_samples: ms_to_samples(settings.minimum_chunk_ms),
            end_silence_samples: ms_to_samples(settings.end_silence_ms),
            overlap_samples: ms_to_samples(settings.overlap_ms),
            step_samples: ms_to_samples(PARTIAL_STEP_MS),
            speech_start_samples: ms_to_samples(SPEECH_START_MS),
            fixed: settings.chunk_mode == "fixed",
            emit_rolling: settings.task != "translate" || settings.caption_mode != "stable",
            buffer: VecDeque::with_capacity(ms_to_samples(settings.maximum_chunk_ms)),
            buffer_start_sample: 0,
            total_samples: 0,
            last_emit_sample: 0,
            speech_started: false,
            utterance_samples: 0,
            speech_samples: 0,
            pending_speech_samples: 0,
            trailing_silence: 0,
        }
    }

    #[cfg(test)]
    pub fn push(&mut self, samples: &[f32]) -> Vec<AudioWindow> {
        self.push_with_activity(samples, rms(samples) >= SPEECH_RMS_THRESHOLD)
    }

    pub fn push_with_activity(&mut self, samples: &[f32], speaking: bool) -> Vec<AudioWindow> {
        let mut windows = Vec::new();
        for frame in samples.chunks(FRAME_SAMPLES) {
            self.push_frame(frame, speaking);
            if !self.speech_started {
                continue;
            }

            let reached_maximum = self.buffer.len() >= self.maximum_samples;
            let ended_on_silence = !self.fixed && self.trailing_silence >= self.end_silence_samples;
            // Adaptive mode must also close long, uninterrupted speech. Without
            // this branch the buffer slid forever and its changing prefix made
            // stable hypotheses regress.
            let final_window = reached_maximum || ended_on_silence;
            if !final_window && !self.ready() {
                continue;
            }
            if !final_window && !self.emit_rolling {
                continue;
            }
            let due = self.last_emit_sample == 0
                || self.total_samples.saturating_sub(self.last_emit_sample) >= self.step_samples;
            if final_window || due {
                windows.push(self.snapshot(
                    if final_window {
                        if reached_maximum {
                            "maximum"
                        } else {
                            "silence"
                        }
                    } else {
                        "rolling"
                    },
                    final_window,
                ));
                self.last_emit_sample = self.total_samples;
            }
            if final_window {
                self.finish_utterance();
            }
        }
        windows
    }

    pub fn flush(&mut self) -> Option<AudioWindow> {
        self.speech_started
            .then(|| self.snapshot("end-of-stream", true))
    }

    fn push_frame(&mut self, frame: &[f32], speaking: bool) {
        if self.buffer.is_empty() {
            self.buffer_start_sample = self.total_samples;
        }
        self.total_samples += frame.len();

        let mut started_now = false;
        if speaking && !self.speech_started {
            self.pending_speech_samples += frame.len();
        } else if !speaking && !self.speech_started {
            self.pending_speech_samples = 0;
        }

        if !self.speech_started && self.pending_speech_samples >= self.speech_start_samples {
            // Keep a small pre-roll so initial consonants are not clipped, but
            // discard older silence/music rather than feeding a full stale
            // buffer into the next inference window.
            while self.buffer.len() > self.overlap_samples {
                self.buffer.pop_front();
                self.buffer_start_sample += 1;
            }
            self.speech_started = true;
            self.utterance_samples = self.pending_speech_samples;
            self.speech_samples = self.pending_speech_samples;
            self.pending_speech_samples = 0;
            started_now = true;
        }
        if self.speech_started && !started_now {
            self.utterance_samples += frame.len();
            if speaking {
                self.speech_samples += frame.len();
            }
        }
        self.trailing_silence = if self.speech_started {
            if speaking {
                0
            } else {
                self.trailing_silence + frame.len()
            }
        } else {
            0
        };
        self.buffer.extend(frame);

        while self.buffer.len() > self.maximum_samples {
            self.buffer.pop_front();
            self.buffer_start_sample += 1;
        }
    }

    fn ready(&self) -> bool {
        self.speech_started && self.utterance_samples >= self.minimum_samples
    }

    fn snapshot(&self, reason: &'static str, final_window: bool) -> AudioWindow {
        AudioWindow {
            start_sample: self.buffer_start_sample,
            reason,
            final_window,
            pcm_gap_ms: 0,
            speech_ms: samples_to_ms(self.speech_samples),
            samples: self.buffer.iter().copied().collect(),
        }
    }

    fn finish_utterance(&mut self) {
        let keep = self.overlap_samples.min(self.buffer.len());
        while self.buffer.len() > keep {
            self.buffer.pop_front();
            self.buffer_start_sample += 1;
        }
        self.speech_started = false;
        self.utterance_samples = 0;
        self.speech_samples = 0;
        self.pending_speech_samples = 0;
        self.trailing_silence = 0;
    }
}

pub fn decode_pcm_into(bytes: &[u8], out: &mut Vec<f32>) {
    out.clear();
    out.reserve(bytes.len() / 2);
    out.extend(
        bytes
            .chunks_exact(2)
            .map(|sample| f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32_768.0),
    );
}

pub fn samples_to_ms(samples: usize) -> u64 {
    (samples as u64 * 1_000) / SAMPLE_RATE as u64
}

fn ms_to_samples(milliseconds: u32) -> usize {
    milliseconds as usize * SAMPLE_RATE / 1_000
}

// `samples.len()` only loses precision against f32's 23-bit mantissa above
// ~16M samples (~1000s of 16kHz audio) — far beyond a single analysis window.
#[allow(clippy::cast_precision_loss)]
#[cfg(test)]
fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_rolling_then_final_window() {
        let settings = AppSettings {
            maximum_chunk_ms: 6_000,
            end_silence_ms: 200,
            caption_mode: "live".into(),
            ..AppSettings::default()
        };
        let mut window = RollingWindow::new(&settings);

        let rolling = window.push(&vec![0.5; ms_to_samples(1_100)]);
        assert_eq!(rolling.len(), 1);
        assert!(!rolling[0].final_window);

        let final_windows = window.push(&vec![0.0; ms_to_samples(200)]);
        assert_eq!(final_windows.len(), 1);
        assert!(final_windows[0].final_window);
    }

    #[test]
    fn stable_translation_emits_only_final_windows() {
        let settings = AppSettings {
            maximum_chunk_ms: 6_000,
            end_silence_ms: 200,
            ..AppSettings::default()
        };
        let mut window = RollingWindow::new(&settings);

        assert!(window.push(&vec![0.5; ms_to_samples(1_100)]).is_empty());
        let final_windows = window.push(&vec![0.0; ms_to_samples(200)]);

        assert_eq!(final_windows.len(), 1);
        assert!(final_windows[0].final_window);
    }

    #[test]
    fn bounds_the_rolling_audio() {
        let settings = AppSettings {
            maximum_chunk_ms: 2_000,
            ..AppSettings::default()
        };
        let mut window = RollingWindow::new(&settings);
        let windows = window.push(&vec![0.5; ms_to_samples(4_000)]);
        assert!(
            windows
                .iter()
                .all(|window| window.samples.len() <= ms_to_samples(2_000))
        );
        assert!(windows.last().unwrap().start_sample > 0);
    }

    #[test]
    fn ignores_silence_before_speech() {
        let mut window = RollingWindow::new(&AppSettings::default());
        assert!(window.push(&vec![0.0; ms_to_samples(2_000)]).is_empty());
        assert!(window.flush().is_none());
    }

    #[test]
    fn ignores_a_short_vad_blip_followed_by_silence() {
        let mut window = RollingWindow::new(&AppSettings::default());
        assert!(window.push(&vec![0.5; ms_to_samples(200)]).is_empty());
        assert!(window.push(&vec![0.0; ms_to_samples(2_000)]).is_empty());
        assert!(window.flush().is_none());
    }

    #[test]
    fn finalizes_short_real_speech_at_the_pause() {
        let settings = AppSettings {
            minimum_chunk_ms: 1_000,
            end_silence_ms: 250,
            ..AppSettings::default()
        };
        let mut window = RollingWindow::new(&settings);
        assert!(window.push(&vec![0.5; ms_to_samples(400)]).is_empty());
        let final_windows = window.push(&vec![0.0; ms_to_samples(250)]);
        assert_eq!(final_windows.len(), 1);
        assert!(final_windows[0].final_window);
        assert_eq!(final_windows[0].reason, "silence");
        assert_eq!(final_windows[0].speech_ms, 400);
    }

    #[test]
    fn retained_overlap_does_not_satisfy_the_next_minimum() {
        let settings = AppSettings {
            minimum_chunk_ms: 1_000,
            maximum_chunk_ms: 6_000,
            end_silence_ms: 200,
            overlap_ms: 600,
            caption_mode: "live".into(),
            ..AppSettings::default()
        };
        let mut window = RollingWindow::new(&settings);
        let first = window.push(&vec![0.5; ms_to_samples(1_000)]);
        assert!(!first.is_empty());
        let final_window = window.push(&vec![0.0; ms_to_samples(200)]);
        assert!(final_window.last().unwrap().final_window);

        // Only 400ms of new audio follows. The retained 600ms overlap must not
        // turn it into another one-second inference window.
        assert!(window.push(&vec![0.5; ms_to_samples(200)]).is_empty());
        assert!(window.push(&vec![0.0; ms_to_samples(200)]).is_empty());
    }

    #[test]
    fn fixed_mode_finalizes_at_the_maximum_window() {
        let settings = AppSettings {
            chunk_mode: "fixed".into(),
            maximum_chunk_ms: 2_000,
            ..AppSettings::default()
        };
        let mut window = RollingWindow::new(&settings);
        let windows = window.push(&vec![0.5; ms_to_samples(2_000)]);
        assert!(windows.last().unwrap().final_window);
        assert_eq!(windows.last().unwrap().reason, "maximum");
    }

    #[test]
    fn adaptive_mode_finalizes_continuous_speech_at_the_maximum() {
        let settings = AppSettings {
            chunk_mode: "adaptive".into(),
            maximum_chunk_ms: 2_000,
            ..AppSettings::default()
        };
        let mut window = RollingWindow::new(&settings);
        let windows = window.push(&vec![0.5; ms_to_samples(2_000)]);
        assert!(windows.last().unwrap().final_window);
        assert_eq!(windows.last().unwrap().reason, "maximum");
    }

    #[test]
    fn decode_pcm_ignores_trailing_byte() {
        let mut decoded = Vec::new();
        decode_pcm_into(&[0x00, 0x80, 0x01], &mut decoded);
        assert_eq!(decoded, vec![-1.0]);
    }
}
