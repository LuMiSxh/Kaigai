use crate::settings::AppSettings;

#[derive(Debug, PartialEq, Eq)]
pub struct Hypothesis {
    /// Words confirmed across consecutive hypotheses — safe to display.
    pub stable: String,
    /// The still-changing tail. Shown only when transcribing; hidden for
    /// translation, where re-ordering would make it churn.
    pub unstable: String,
}

/// Streaming word stabilizer. A word is committed once it has appeared at the
/// same position in two consecutive hypotheses, so the rendered line grows
/// instead of rewriting itself every window.
pub struct Stabilizer {
    /// Words committed for the current line; never rewritten once locked.
    locked: Vec<String>,
    /// The previous hypothesis's tail (seen once, awaiting confirmation).
    pending: Vec<String>,
    /// Squashed text of the last finalized line, to drop exact repeats caused
    /// by overlap re-transcription.
    last_final: String,
    /// Original units of the previous final line, used to remove the overlap
    /// intentionally retained between adjacent audio windows.
    last_final_units: Vec<String>,
    revision: u64,
    expose_tail: bool,
}

impl Stabilizer {
    pub fn new(settings: &AppSettings) -> Self {
        Self {
            locked: Vec::new(),
            pending: Vec::new(),
            last_final: String::new(),
            last_final_units: Vec::new(),
            revision: 0,
            // Translation re-orders as context grows, so an unconfirmed tail
            // would flicker; only same-language transcription shows it live.
            expose_tail: settings.task != "translate",
        }
    }

    pub fn update(&mut self, text: &str) -> Hypothesis {
        self.revision = self.revision.wrapping_add(1);
        let tokens = tokenize(text);

        // If a previously committed word changed (re-ordering), back off to the
        // agreement point rather than show stale text.
        let agree = common_prefix_len(&self.locked, &tokens);
        if agree < self.locked.len() {
            self.locked.truncate(agree);
        }

        // Commit the tail words that match the previous hypothesis (seen twice).
        let rest = &tokens[self.locked.len().min(tokens.len())..];
        let confirm = common_prefix_len(&self.pending, rest);
        self.locked.extend_from_slice(&rest[..confirm]);
        self.pending = tokens[self.locked.len().min(tokens.len())..].to_vec();

        let stable = render(&self.locked);
        let unstable = if self.expose_tail {
            render(&self.pending)
        } else {
            String::new()
        };
        Hypothesis { stable, unstable }
    }

    /// Finalize the current line. Returns an empty string when this line is an
    /// exact repeat of the previous one (so the caller can skip emitting it).
    pub fn finalize(&mut self, text: &str) -> String {
        self.revision = self.revision.wrapping_add(1);
        self.locked.clear();
        self.pending.clear();
        let units = tokenize(text);
        let line = render(&units);
        let key = squash(&line);
        if key.is_empty() || key == self.last_final {
            return String::new();
        }
        self.last_final = key;

        // Adjacent windows deliberately overlap. Remove the longest meaningful
        // suffix/prefix match so that overlap does not repeat at line breaks.
        let overlap = meaningful_overlap(&self.last_final_units, &units);
        self.last_final_units.clone_from(&units);
        render(&units[overlap..])
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let mut units = Vec::new();
    let mut current = String::new();
    let mut whitespace = String::new();

    for character in text.trim().chars() {
        if character.is_whitespace() {
            flush(&mut units, &mut current);
            if whitespace.is_empty() {
                whitespace.push(' ');
            }
        } else if is_cjk(character) {
            flush(&mut units, &mut current);
            let mut unit = std::mem::take(&mut whitespace);
            unit.push(character);
            units.push(unit);
        } else if character.is_alphanumeric() || matches!(character, '\'' | '’') {
            if current.is_empty() {
                current = std::mem::take(&mut whitespace);
            }
            current.push(character);
        } else if current.is_empty() {
            let prefix = std::mem::take(&mut whitespace);
            if let Some(previous) = units.last_mut() {
                previous.push_str(&prefix);
                previous.push(character);
            } else {
                current = prefix;
                current.push(character);
            }
        } else {
            current.push(character);
            flush(&mut units, &mut current);
        }
    }
    flush(&mut units, &mut current);
    units
}

fn flush(units: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() {
        units.push(std::mem::take(current));
    }
}

fn render(units: &[String]) -> String {
    units.concat().trim().to_owned()
}

fn meaningful_overlap(previous: &[String], current: &[String]) -> usize {
    let mut best = 0;
    for length in 1..=previous.len().min(current.len()) {
        if previous[previous.len() - length..]
            .iter()
            .zip(&current[..length])
            .all(|(left, right)| squash(left) == squash(right))
            && squash(&render(&current[..length])).chars().count() >= 2
        {
            best = length;
        }
    }
    best
}

/// Treat CJK characters as independently stabilizable units. Whisper's
/// Japanese output usually has no spaces, so whitespace tokenization made the
/// whole sentence flicker as one giant token.
fn is_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x3040..=0x30ff // Hiragana + Katakana
            | 0x3400..=0x4dbf // CJK Extension A
            | 0x4e00..=0x9fff // CJK Unified Ideographs
            | 0xac00..=0xd7af // Hangul syllables
            | 0xf900..=0xfaff // CJK compatibility ideographs
    )
}

/// Lowercase alphanumerics only, for punctuation/spacing-insensitive comparison.
pub(super) fn squash(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn common_prefix_len(left: &[String], right: &[String]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(left, right)| left == right)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transcribing() -> Stabilizer {
        Stabilizer::new(&AppSettings {
            task: "transcribe".into(),
            ..AppSettings::default()
        })
    }

    #[test]
    fn commits_words_only_after_two_consistent_hypotheses() {
        let mut stabilizer = transcribing();
        let first = stabilizer.update("hello world");
        assert_eq!(first.stable, "");
        assert_eq!(first.unstable, "hello world");

        let second = stabilizer.update("hello world today");
        assert_eq!(second.stable, "hello world");
        assert_eq!(second.unstable, "today");
    }

    #[test]
    fn backs_off_when_a_committed_word_changes() {
        let mut stabilizer = transcribing();
        stabilizer.update("the cat sat");
        stabilizer.update("the cat sat"); // commits "the cat sat"
        let revised = stabilizer.update("the cat ran home");
        assert_eq!(revised.stable, "the cat");
    }

    #[test]
    fn translation_hides_the_unconfirmed_tail() {
        let mut stabilizer = Stabilizer::new(&AppSettings::default()); // translate
        let hypothesis = stabilizer.update("subject verb object");
        assert_eq!(hypothesis.stable, "");
        assert_eq!(hypothesis.unstable, "");
    }

    #[test]
    fn skips_a_finalized_line_that_repeats_the_previous() {
        let mut stabilizer = transcribing();
        assert_eq!(stabilizer.finalize("hello there"), "hello there");
        assert_eq!(stabilizer.finalize("Hello there."), "");
        assert_eq!(stabilizer.finalize("something new"), "something new");
    }

    #[test]
    fn finalization_resets_state_and_normalizes() {
        let mut stabilizer = transcribing();
        stabilizer.update("one two");
        assert_eq!(stabilizer.finalize("one   two  three"), "one two three");
        assert_eq!(stabilizer.update("fresh start").stable, "");
    }

    #[test]
    fn stabilizes_japanese_without_inserting_spaces() {
        let mut stabilizer = transcribing();
        let first = stabilizer.update("今日は晴れ");
        assert_eq!(first.unstable, "今日は晴れ");
        let second = stabilizer.update("今日は晴れです");
        assert_eq!(second.stable, "今日は晴れ");
        assert_eq!(second.unstable, "です");
    }

    #[test]
    fn removes_audio_overlap_from_the_next_final_line() {
        let mut stabilizer = transcribing();
        assert_eq!(stabilizer.finalize("hello there"), "hello there");
        assert_eq!(stabilizer.finalize("there, next line"), "next line");
        assert_eq!(stabilizer.finalize("next line continues"), "continues");
    }
}
