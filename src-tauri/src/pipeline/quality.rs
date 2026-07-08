use crate::settings::AppSettings;

use super::stabilizer::squash;

const MIN_ROLLING_TRANSLATION_SPEECH_MS: u64 = 900;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityDecision {
    pub allow: bool,
    pub reason: &'static str,
}

impl QualityDecision {
    const fn allow(reason: &'static str) -> Self {
        Self {
            allow: true,
            reason,
        }
    }

    const fn reject(reason: &'static str) -> Self {
        Self {
            allow: false,
            reason,
        }
    }
}

/// Product-facing guardrail for live captions.
///
/// Whisper filtering removes known hallucination strings before this point.
/// This gate deals with live UX: don't surface weak rolling translation drafts,
/// empty partial updates, or duplicate partials that only churn the overlay.
pub struct CaptionQualityGate {
    translate: bool,
    last_partial_key: String,
}

impl CaptionQualityGate {
    pub fn new(settings: &AppSettings) -> Self {
        Self {
            translate: settings.task == "translate",
            last_partial_key: String::new(),
        }
    }

    pub fn evaluate_inference_text(
        &self,
        text: &str,
        final_window: bool,
        speech_ms: u64,
        segments: usize,
    ) -> QualityDecision {
        if segments == 0 || squash(text).is_empty() {
            return QualityDecision::reject("empty");
        }
        if is_dominant_repeated_word_run(text) {
            return QualityDecision::reject("dominant-repetition");
        }
        if !final_window
            && self.translate
            && speech_ms < MIN_ROLLING_TRANSLATION_SPEECH_MS
            && is_weak_short_translation(text)
        {
            return QualityDecision::reject("weak-rolling-translation");
        }
        QualityDecision::allow("accepted")
    }

    pub fn evaluate_partial(&mut self, stable_text: &str, unstable_text: &str) -> QualityDecision {
        let key = squash(&format!("{stable_text} {unstable_text}"));
        if key.is_empty() {
            return QualityDecision::reject("empty-partial");
        }
        if key == self.last_partial_key {
            return QualityDecision::reject("duplicate-partial");
        }
        self.last_partial_key = key;
        QualityDecision::allow("accepted-partial")
    }

    pub fn accept_final(&mut self) {
        self.last_partial_key.clear();
    }
}

fn is_weak_short_translation(text: &str) -> bool {
    let words = text
        .split_whitespace()
        .filter(|word| !squash(word).is_empty())
        .count();
    let characters = squash(text).chars().count();
    words <= 2 && characters <= 14
}

fn is_dominant_repeated_word_run(text: &str) -> bool {
    let words = text
        .split_whitespace()
        .map(squash)
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    if words.len() < 6 {
        return false;
    }

    let mut index = 0;
    while index < words.len() {
        let mut repeats = 1;
        while index + repeats < words.len() && words[index + repeats] == words[index] {
            repeats += 1;
        }
        if repeats >= 5 && repeats * 2 >= words.len() {
            return true;
        }
        index += repeats;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translate_gate() -> CaptionQualityGate {
        CaptionQualityGate::new(&AppSettings::default())
    }

    fn transcribe_gate() -> CaptionQualityGate {
        CaptionQualityGate::new(&AppSettings {
            task: "transcribe".into(),
            ..AppSettings::default()
        })
    }

    #[test]
    fn rejects_weak_low_speech_rolling_translation() {
        let gate = translate_gate();
        let decision = gate.evaluate_inference_text("I see.", false, 320, 1);
        assert_eq!(
            decision,
            QualityDecision::reject("weak-rolling-translation")
        );
    }

    #[test]
    fn keeps_short_final_translation() {
        let gate = translate_gate();
        let decision = gate.evaluate_inference_text("I see.", true, 320, 1);
        assert_eq!(decision, QualityDecision::allow("accepted"));
    }

    #[test]
    fn keeps_transcription_rolling_tail() {
        let gate = transcribe_gate();
        let decision = gate.evaluate_inference_text("うん", false, 320, 1);
        assert_eq!(decision, QualityDecision::allow("accepted"));
    }

    #[test]
    fn rejects_empty_and_duplicate_partials() {
        let mut gate = translate_gate();
        assert_eq!(
            gate.evaluate_partial("", ""),
            QualityDecision::reject("empty-partial")
        );
        assert_eq!(
            gate.evaluate_partial("hello", ""),
            QualityDecision::allow("accepted-partial")
        );
        assert_eq!(
            gate.evaluate_partial("hello", ""),
            QualityDecision::reject("duplicate-partial")
        );
    }

    #[test]
    fn rejects_dominant_word_runs() {
        let gate = translate_gate();
        let decision = gate.evaluate_inference_text(
            "money money money money money money is here",
            true,
            2_000,
            1,
        );
        assert_eq!(decision, QualityDecision::reject("dominant-repetition"));
    }
}
