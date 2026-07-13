use std::{
    path::Path,
    sync::{
        Arc, Once,
        atomic::{AtomicBool, Ordering},
    },
};

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::settings::AppSettings;

/// whisper.cpp reports timestamps in centiseconds; multiply to get milliseconds.
const WHISPER_TS_UNIT_MS: u64 = 10;
/// whisper.cpp doesn't apply its own no-speech threshold in the short-window
/// path, so apply it ourselves before text reaches the stabilizer.
const NO_SPEECH_THRESHOLD: f32 = 0.60;

static LOGGING_HOOKS: Once = Once::new();

/// Stock `YouTube`-outro phrases Whisper hallucinates on silence/non-speech
/// audio. Matched exactly (ignoring case/punctuation) so real speech that
/// merely contains these words is kept.
const HALLUCINATIONS: &[&str] = &[
    "thank you for watching",
    "thanks for watching",
    "thank you for watching the video",
    "thank you so much for watching",
    "thank you for watching and see you next time",
    "see you next time",
    "please subscribe",
    "please subscribe to my channel",
    "like and subscribe",
    "please like and subscribe",
    "please give me a thumbs up",
    "give this video a thumbs up",
    "please like the video",
    "subscribe to my channel",
    "ご視聴ありがとうございました",
    "ご視聴ありがとうございます",
    "最後までご視聴いただきありがとうございます",
    "チャンネル登録お願いします",
];

/// Strong subtitle-artifact snippets may be embedded in an otherwise longer
/// segment. Remove the artifact part instead of only rejecting exact matches.
const REMOVABLE_ARTIFACTS: &[&str] = &[
    "thank you for watching",
    "thanks for watching",
    "thank you so much for watching",
    "see you next time",
    "please subscribe to my channel",
    "please subscribe",
    "like and subscribe",
    "please like and subscribe",
    "subscribe to my channel",
    "translated by releska",
    "translated by",
    "ご視聴ありがとうございました",
    "ご視聴ありがとうございます",
    "チャンネル登録お願いします",
];

/// Phrases that can be legitimate once, but usually indicate a decoder loop
/// when repeated inside a short streaming window.
const LOOP_PHRASES: &[&str] = &[
    "i'm sorry",
    "i am sorry",
    "i'm going to eat",
    "i am going to eat",
    "i'm not sure if i can do it",
    "i am not sure if i can do it",
];

/// Short English outputs that are often legitimate translations, but are also
/// Whisper's most common failure mode on tiny/noisy windows. Unlike the stock
/// phrases above, these need a source-language verification before removal.
const AMBIGUOUS_TRANSLATIONS: &[&str] = &["bye", "goodbye", "thank you", "thanks", "see you"];

/// Reduce text to lowercase alphanumerics so spacing and punctuation don't
/// affect the hallucination comparison (works for Latin and CJK alike).
fn squash(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_hallucination(text: &str) -> bool {
    let squashed = squash(text);
    if squashed.is_empty() {
        return true;
    }
    HALLUCINATIONS.iter().any(|phrase| {
        let phrase = squash(phrase);
        squashed == phrase
            || (squashed.len() > phrase.len()
                && squashed.len().is_multiple_of(phrase.len())
                && squashed
                    .as_bytes()
                    .chunks(phrase.len())
                    .all(|chunk| chunk == phrase.as_bytes()))
    })
}

fn needs_source_verification(text: &str) -> bool {
    let normalized = squash(text);
    normalized.contains("nexttime")
        || AMBIGUOUS_TRANSLATIONS
            .iter()
            .any(|phrase| squash(phrase) == normalized)
}

fn source_supports_translation(source: &str, translation: &str) -> bool {
    let source = squash(source);
    let translation = squash(translation);
    if source.is_empty() || is_hallucination(&source) {
        return false;
    }

    if translation.contains("thank") {
        return ["ありがとう", "有難う", "感謝"]
            .iter()
            .any(|marker| source.contains(marker));
    }
    if translation.contains("bye")
        || translation.contains("seeyou")
        || translation.contains("nexttime")
    {
        return [
            "バイバイ",
            "さようなら",
            "じゃあね",
            "またね",
            "また会",
            "また見",
            "次回",
            "今度",
        ]
        .iter()
        .any(|marker| source.contains(marker));
    }
    if translation.contains("subscribe") {
        return source.contains("登録");
    }
    if translation.contains("watch") {
        return source.contains("視聴") || source.contains("見て");
    }
    false
}

pub struct WhisperSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub no_speech_probability: f32,
}

pub struct WhisperEngine {
    state: WhisperState,
    language: String,
    translate: bool,
    cancel: Arc<AtomicBool>,
}

impl WhisperEngine {
    pub fn load(settings: &AppSettings, cancel: Arc<AtomicBool>) -> Result<Option<Self>, String> {
        let model_path = settings.model_path.trim();
        if model_path.is_empty() {
            return Ok(None);
        }
        if !Path::new(model_path).is_file() {
            return Err(format!("Whisper model not found: {model_path}"));
        }

        // Global, process-wide and idempotent: install exactly once.
        LOGGING_HOOKS.call_once(whisper_rs::install_logging_hooks);
        let mut context_params = WhisperContextParameters::default();
        context_params.flash_attn(true);
        let context = WhisperContext::new_with_params(model_path, context_params)
            .map_err(|error| format!("failed to load Whisper model: {error}"))?;
        tracing::info!(
            model = %context.model_type_readable_str_lossy().unwrap_or_default(),
            flash_attention = true,
            system = whisper_rs::print_system_info(),
            "Whisper backend initialized"
        );
        // The state owns its own reference to the context, so it outlives the
        // local `context` binding and can be reused across every chunk.
        let state = context
            .create_state()
            .map_err(|error| format!("failed to create Whisper state: {error}"))?;
        Ok(Some(Self {
            state,
            language: settings.source_language.clone(),
            translate: settings.task == "translate",
            cancel,
        }))
    }

    pub fn transcribe(&mut self, audio: &[f32]) -> Result<Vec<WhisperSegment>, String> {
        let mut segments = self.decode(audio, self.translate)?;
        clean_segments(&mut segments);
        segments
            .retain(|segment| output_is_supported(&segment.text, segment.no_speech_probability));
        if !self.translate {
            return Ok(segments);
        }

        if segments
            .iter()
            .any(|segment| needs_source_verification(&segment.text))
        {
            let source = self
                .decode(audio, false)?
                .into_iter()
                .filter_map(|mut segment| {
                    segment.text = clean_output_text(&segment.text)?;
                    Some(segment)
                })
                .filter(|segment| output_is_supported(&segment.text, segment.no_speech_probability))
                .map(|segment| segment.text)
                .collect::<Vec<_>>()
                .join(" ");
            segments.retain(|segment| {
                !needs_source_verification(&segment.text)
                    || source_supports_translation(&source, &segment.text)
            });
        }
        Ok(segments)
    }

    fn decode(&mut self, audio: &[f32], translate: bool) -> Result<Vec<WhisperSegment>, String> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(&self.language));
        params.set_translate(translate);
        params.set_no_context(true);
        // One short window, rendered as one caption — skip multi-segment
        // bookkeeping and cap runaway repetition loops.
        params.set_single_segment(true);
        params.set_max_tokens(128);
        params.set_temperature(0.0);
        params.set_temperature_inc(0.0);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        // Suppress non-speech tokens so near-silent audio is less likely to
        // collapse into hallucinated stock phrases.
        params.set_suppress_nst(true);
        let cancel = self.cancel.clone();
        let abort: Box<dyn FnMut() -> bool> = Box::new(move || cancel.load(Ordering::Relaxed));
        params
            .set_abort_callback_safe::<Option<Box<dyn FnMut() -> bool>>, Box<dyn FnMut() -> bool>>(
                Some(abort),
            );

        self.state
            .full(params, audio)
            .map_err(|error| format!("Whisper inference failed: {error}"))?;

        if self.language == "auto" {
            let detected = whisper_rs::get_lang_str(self.state.full_lang_id_from_state())
                .unwrap_or("unknown");
            tracing::debug!(detected, translate, "auto-detected window language");
        }

        Ok(self
            .state
            .as_iter()
            .filter_map(|segment| {
                let text = segment.to_string().trim().to_owned();
                if text.is_empty() {
                    return None;
                }
                Some(WhisperSegment {
                    start_ms: segment.start_timestamp().max(0).cast_unsigned() * WHISPER_TS_UNIT_MS,
                    end_ms: segment.end_timestamp().max(0).cast_unsigned() * WHISPER_TS_UNIT_MS,
                    text,
                    no_speech_probability: segment.no_speech_probability(),
                })
            })
            .collect())
    }
}

fn output_is_supported(text: &str, no_speech_probability: f32) -> bool {
    no_speech_probability < NO_SPEECH_THRESHOLD
        && !is_hallucination(text)
        && !is_repetition_loop(text)
}

fn clean_segments(segments: &mut Vec<WhisperSegment>) {
    for segment in &mut *segments {
        if let Some(text) = clean_output_text(&segment.text) {
            segment.text = text;
        } else {
            segment.text.clear();
        }
    }
}

fn clean_output_text(text: &str) -> Option<String> {
    let mut text = strip_translator_credit(text.trim());
    for artifact in REMOVABLE_ARTIFACTS {
        text = strip_case_insensitive(&text, artifact);
    }
    text = collapse_repeated_units(&text);
    text = collapse_repeated_words(&text);
    text = collapse_repeated_characters(&text);
    let text = normalize_spaces(&trim_lonely_separators(&text));
    (!squash(&text).is_empty()).then_some(text)
}

fn strip_translator_credit(text: &str) -> String {
    let normalized = text.to_lowercase();
    if !normalized.starts_with("translated by ") {
        return text.into();
    }
    text.split_whitespace()
        .skip(3)
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_case_insensitive(text: &str, needle: &str) -> String {
    let mut output = text.to_owned();
    loop {
        let lowercase = output.to_lowercase();
        let Some(start) = lowercase.find(needle) else {
            return output;
        };
        let end = start + needle.len();
        output.replace_range(start..end, "");
    }
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn trim_lonely_separators(text: &str) -> String {
    text.trim_start_matches(|character: char| {
        character.is_whitespace()
            || matches!(
                character,
                '.' | ',' | ':' | ';' | '-' | '–' | '—' | '。' | '、' | '，'
            )
    })
    .into()
}

fn collapse_repeated_units(text: &str) -> String {
    let characters = text.chars().collect::<Vec<_>>();
    let mut output = String::new();
    let mut index = 0;
    while index < characters.len() {
        let mut collapsed = false;
        for unit_len in (2..=6).rev() {
            if index + unit_len * 4 > characters.len() {
                continue;
            }
            let unit = &characters[index..index + unit_len];
            if !unit.iter().all(|character| is_japanese_text(*character)) {
                continue;
            }
            let mut repeats = 1;
            while index + unit_len * (repeats + 1) <= characters.len()
                && &characters[index + unit_len * repeats..index + unit_len * (repeats + 1)] == unit
            {
                repeats += 1;
            }
            if repeats >= 4 {
                output.extend(unit.iter());
                output.extend(unit.iter());
                index += unit_len * repeats;
                collapsed = true;
                break;
            }
        }
        if !collapsed {
            output.push(characters[index]);
            index += 1;
        }
    }
    output
}

fn collapse_repeated_words(text: &str) -> String {
    let words = text.split_whitespace().collect::<Vec<_>>();
    if words.len() < 6 {
        return text.into();
    }
    let mut output = Vec::new();
    let mut index = 0;
    while index < words.len() {
        let mut repeats = 1;
        while index + repeats < words.len()
            && squash(words[index + repeats]) == squash(words[index])
            && !squash(words[index]).is_empty()
        {
            repeats += 1;
        }
        let keep = if repeats >= 5 { 2 } else { repeats };
        for _ in 0..keep {
            output.push(words[index]);
        }
        index += repeats;
    }
    output.join(" ")
}

fn collapse_repeated_characters(text: &str) -> String {
    let mut output = String::new();
    let mut previous = None;
    let mut count = 0;
    for character in text.chars() {
        if Some(character) == previous {
            count += 1;
        } else {
            previous = Some(character);
            count = 1;
        }
        if count <= 4 || character.is_whitespace() || !character.is_alphanumeric() {
            output.push(character);
        }
    }
    output
}

fn is_japanese_text(character: char) -> bool {
    matches!(
        character,
        '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}'
    )
}

fn is_repetition_loop(text: &str) -> bool {
    let normalized = text.to_lowercase();
    LOOP_PHRASES.iter().any(|phrase| {
        let count = normalized.matches(phrase).count();
        count >= 3 && squash(text).len() <= squash(phrase).len() * count + 24
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::settings::AppSettings;

    #[test]
    fn flags_known_hallucinations_regardless_of_punctuation() {
        assert!(is_hallucination("Thank you for watching!"));
        assert!(is_hallucination("  thanks for watching. "));
        assert!(is_hallucination("ご視聴ありがとうございました。"));
        assert!(is_hallucination(
            "Thank you for watching! Thank you for watching!"
        ));
        assert!(is_hallucination("Please subscribe to my channel!"));
        assert!(is_hallucination("Please give me a thumbs up!"));
        assert!(is_hallucination("See you next time."));
        assert!(is_hallucination(""));
    }

    #[test]
    fn keeps_real_speech() {
        assert!(!is_hallucination("Thank you for the gift you sent today"));
        assert!(!is_hallucination("今日は配信に来てくれてありがとう"));
        assert!(!is_hallucination("Hello everyone"));
    }

    #[test]
    fn rejects_text_when_whisper_predicts_no_speech() {
        assert!(!output_is_supported("invented sentence", 0.92));
        assert!(output_is_supported("actual speech", 0.12));
    }

    #[test]
    fn strips_embedded_subtitle_artifacts() {
        assert_eq!(
            clean_output_text("Thank you for watching. Actual translation here."),
            Some("Actual translation here.".into())
        );
        assert_eq!(
            clean_output_text("Translated by Releska I wonder if I'm here"),
            Some("I wonder if I'm here".into())
        );
        assert_eq!(clean_output_text("Please subscribe."), None);
    }

    #[test]
    fn rejects_dominant_repetition_loops() {
        assert!(!output_is_supported(
            "I'm sorry. I'm sorry. I'm sorry. I'm sorry.",
            0.1
        ));
        assert!(output_is_supported(
            "I'm sorry about the timing, but thank you for the gift.",
            0.1
        ));
    }

    #[test]
    fn collapses_source_repetition_artifacts() {
        assert_eq!(
            clean_output_text("スカスカスカスカスカスカになる"),
            Some("スカスカになる".into())
        );
        assert_eq!(
            clean_output_text("ううううううううう"),
            Some("うううう".into())
        );
        assert_eq!(
            clean_output_text("money money money money money money is here"),
            Some("money money is here".into())
        );
    }

    #[test]
    fn verifies_ambiguous_outros_against_japanese_source() {
        assert!(needs_source_verification("Bye!"));
        assert!(needs_source_verification("Thank you."));
        assert!(needs_source_verification("Okay, next time!"));
        assert!(!source_supports_translation("また!", "Bye!"));
        assert!(!source_supports_translation("いいね", "Okay, next time!"));
        assert!(source_supports_translation("ありがとうね", "Thank you."));
        assert!(source_supports_translation("またね", "See you."));
    }

    /// Smoke test for the inference path. Ignored by default because it needs a
    /// real model; run with `KAIGAI_TEST_MODEL=/path/to/ggml-tiny.bin cargo
    /// test -- --ignored loads_model_and_transcribes_silence`.
    #[test]
    #[ignore = "requires a ggml model via KAIGAI_TEST_MODEL"]
    fn loads_model_and_transcribes_silence() {
        let model_path =
            std::env::var("KAIGAI_TEST_MODEL").expect("set KAIGAI_TEST_MODEL to a .bin path");
        let settings = AppSettings {
            model_path,
            task: "transcribe".into(),
            ..AppSettings::default()
        };
        let mut engine = WhisperEngine::load(&settings, Arc::new(AtomicBool::new(false)))
            .expect("load succeeds")
            .expect("engine present when model_path is set");
        // One second of silence must run cleanly without panicking.
        let segments = engine
            .transcribe(&vec![0.0_f32; 16_000])
            .expect("inference succeeds");
        let _ = segments.len();
    }

    /// Diagnostic for a real 16-bit mono 16 kHz WAV. Run with
    /// `KAIGAI_TEST_MODEL=... KAIGAI_TEST_AUDIO=... cargo test
    /// diagnoses_audio_windows -- --ignored --nocapture`.
    #[test]
    #[ignore = "requires a ggml model and WAV via environment variables"]
    fn diagnoses_audio_windows() {
        use super::super::audio_window::RollingWindow;
        use super::super::vad::SpeechDetector;

        const SAMPLE_RATE: usize = 16_000;
        let model_path = std::env::var("KAIGAI_TEST_MODEL").expect("model path");
        let audio_path = std::env::var("KAIGAI_TEST_AUDIO").expect("audio path");
        let task = std::env::var("KAIGAI_TEST_TASK").unwrap_or_else(|_| "translate".into());
        let end_silence_ms = std::env::var("KAIGAI_TEST_END_SILENCE_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(250);
        let bytes = std::fs::read(audio_path).expect("read WAV");
        let data_offset = bytes
            .windows(4)
            .position(|window| window == b"data")
            .map(|offset| offset + 8)
            .expect("WAV data chunk");
        let audio = bytes[data_offset..]
            .chunks_exact(2)
            .map(|sample| f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32_768.0)
            .collect::<Vec<_>>();
        let settings = AppSettings {
            model_path,
            source_language: "ja".into(),
            task,
            end_silence_ms,
            ..AppSettings::default()
        };
        let mut engine = WhisperEngine::load(&settings, Arc::new(AtomicBool::new(false)))
            .expect("load model")
            .expect("engine");
        let mut rolling = RollingWindow::new(&settings);
        let mut detector = std::env::var("KAIGAI_TEST_VAD_MODEL")
            .ok()
            .map(|path| SpeechDetector::from_path(&PathBuf::from(path), 0.5).expect("load VAD"));

        for samples in audio.chunks(SAMPLE_RATE / 10) {
            let windows = if let Some(detector) = &mut detector {
                let speaking = detector.is_speech(samples).expect("VAD inference");
                rolling.push_with_activity(samples, speaking)
            } else {
                rolling.push(samples)
            };
            for window in windows {
                let segments = engine.transcribe(&window.samples).expect("transcribe");
                for segment in segments {
                    println!(
                        "{} {:>5}ms {:?}",
                        window.reason,
                        window.samples.len() * 1_000 / SAMPLE_RATE,
                        segment.text,
                    );
                }
            }
        }
    }
}
