//! The vocabulary of the disguise.
//!
//! A reader thinks in characters, chapters and minutes. An agent session talks
//! about tokens, context windows and elapsed time. This module is the exchange
//! rate between the two, in one place so every surface agrees:
//!
//! - characters → **tokens**
//! - reading progress → **context window used**
//! - time since launch → the session clock
//!
//! The numbers stay honest: they are derived from the real text, not invented.

use std::time::Duration;

/// Estimated tokens for a string.
///
/// A tokenizer-free approximation, calibrated to how modern BPE vocabularies
/// behave: CJK runs land near three tokens per four characters, Latin text near
/// one per four. Close enough that the readout moves the way a real one would.
pub fn tokens_of(text: &str) -> usize {
    let mut cjk = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        if ch.is_whitespace() {
            continue;
        }
        if is_cjk(ch) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    let estimate = (cjk as f32 * 0.75) + (other as f32 / 4.0);
    if estimate > 0.0 {
        estimate.round().max(1.0) as usize
    } else {
        0
    }
}

/// Tokens for a character count, when the text itself is not at hand.
///
/// Book-scale counts come from chapter sizes, so assume the book's own mix:
/// Chinese prose dominates the corpus this reader is built for.
pub fn tokens_from_chars(chars: usize) -> usize {
    ((chars as f32) * 0.75).round() as usize
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0x20000..=0x3FFFD)
}

/// Compact token count: `812`, `12.4k`, `1.20M`.
pub fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.2}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

/// Session clock: `0:42`, `18:07`, `2:15:31`.
pub fn format_duration(elapsed: Duration) -> String {
    let total = elapsed.as_secs();
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Reading progress presented as context-window pressure.
///
/// The book is the window: finishing it fills the context. Rounded so a
/// just-started book never reads as a suspicious 0.0%.
pub fn context_percent(progress: f32) -> f32 {
    (progress.clamp(0.0, 1.0) * 100.0 * 10.0).round() / 10.0
}

/// Segments of the context bar: read, current chapter, still free.
///
/// Returned as fractions that sum to 1, so a renderer can lay them out without
/// knowing anything about books.
pub fn context_split(read: usize, chapter: usize, total: usize) -> (f32, f32, f32) {
    if total == 0 {
        return (0.0, 0.0, 1.0);
    }
    let read_share = (read as f32 / total as f32).clamp(0.0, 1.0);
    let chapter_share = (chapter as f32 / total as f32).clamp(0.0, 1.0 - read_share);
    let free = (1.0 - read_share - chapter_share).max(0.0);
    (read_share, chapter_share, free)
}

/// Tokens per second, for the status line's throughput readout.
pub fn tokens_per_second(chars_per_second: f32) -> f32 {
    (chars_per_second * 0.75).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_costs_more_tokens_per_character_than_english() {
        let chinese = tokens_of("注意力的形状");
        let english = tokens_of("the shape of attention");
        assert!(
            chinese >= 4,
            "six CJK chars should be ~4-6 tokens: {chinese}"
        );
        assert!(
            english < chinese * 2,
            "22 latin chars should stay in the same ballpark: {english}"
        );
        assert!(english >= 4, "should not undercount latin text: {english}");
    }

    #[test]
    fn whitespace_is_free_and_empty_text_is_zero() {
        assert_eq!(tokens_of(""), 0);
        assert_eq!(tokens_of("   \n\t "), 0);
        assert_eq!(
            tokens_of("注意 力"),
            tokens_of("注意力"),
            "spaces should not add tokens"
        );
    }

    #[test]
    fn token_counts_grow_with_the_text() {
        let short = tokens_of("一段话。");
        let long = tokens_of(&"一段话。".repeat(20));
        assert!(long > short * 10, "{short} → {long} should scale");
    }

    #[test]
    fn formatting_is_compact_at_every_scale() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(812), "812");
        assert_eq!(format_tokens(12_400), "12.4k");
        assert_eq!(format_tokens(1_200_000), "1.20M");
    }

    #[test]
    fn the_session_clock_reads_like_a_clock() {
        assert_eq!(format_duration(Duration::from_secs(42)), "0:42");
        assert_eq!(format_duration(Duration::from_secs(1_087)), "18:07");
        assert_eq!(format_duration(Duration::from_secs(8_131)), "2:15:31");
    }

    #[test]
    fn context_percent_is_clamped_and_rounded() {
        assert_eq!(context_percent(0.0), 0.0);
        assert_eq!(context_percent(0.3014), 30.1);
        assert_eq!(context_percent(1.0), 100.0);
        assert_eq!(
            context_percent(2.5),
            100.0,
            "progress cannot exceed the book"
        );
        assert_eq!(context_percent(-1.0), 0.0);
    }

    #[test]
    fn context_segments_always_sum_to_the_whole() {
        for (read, chapter, total) in [
            (0, 0, 0),
            (0, 500, 1_000),
            (900, 500, 1_000),
            (250, 250, 1_000),
        ] {
            let (a, b, c) = context_split(read, chapter, total);
            assert!(
                (a + b + c - 1.0).abs() < 1e-5,
                "segments for ({read}, {chapter}, {total}) sum to {}",
                a + b + c
            );
            assert!(a >= 0.0 && b >= 0.0 && c >= 0.0, "no negative segments");
        }
    }

    #[test]
    fn an_overlong_chapter_cannot_overflow_the_bar() {
        let (read, chapter, free) = context_split(900, 5_000, 1_000);
        assert!(chapter <= 1.0 - read + 1e-6, "chapter should be clipped");
        assert_eq!(free, 0.0);
    }
}

/// Characters per word, by script.
///
/// English words average about 5.1 characters with their space. Chinese has no
/// spaces at all, and publishers quoting a translation's length convert at
/// roughly 1.6 characters per English word.
const LATIN_CHARS_PER_WORD: f64 = 5.1;
const CJK_CHARS_PER_WORD: f64 = 1.6;

/// Is this one of the scripts that writes without spaces?
fn is_ideographic(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30ff}'      // hiragana, katakana
        | '\u{3400}'..='\u{4dbf}'    // CJK extension A
        | '\u{4e00}'..='\u{9fff}'    // CJK unified
        | '\u{f900}'..='\u{faff}'    // compatibility ideographs
        | '\u{ac00}'..='\u{d7af}'    // hangul syllables
    )
}

/// Words in a piece of text, counted the way each script counts them.
///
/// Latin runs are counted at word boundaries; ideographic characters are
/// converted at [`CJK_CHARS_PER_WORD`], because counting each 字 as a word would
/// triple a Chinese novel's apparent length.
pub fn words_in(text: &str) -> usize {
    let mut cjk = 0usize;
    let mut latin = String::new();
    for c in text.chars() {
        if is_ideographic(c) {
            cjk += 1;
        } else {
            latin.push(c);
        }
    }
    let latin_words = latin
        .split(|c: char| c.is_whitespace() || (c.is_ascii_punctuation() && c != '\''))
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count();
    latin_words + (cjk as f64 / CJK_CHARS_PER_WORD).round() as usize
}

/// Words implied by a bare character count, for records that predate counting
/// them. Assumes prose in a spaced script, which is the safer guess: it never
/// inflates a number.
pub fn words_from_char_count(chars: usize) -> usize {
    (chars as f64 / LATIN_CHARS_PER_WORD).round() as usize
}

/// How much text there is, in the unit the reader's language uses: 字 for
/// Chinese, words for English.
///
/// Both are rough measures of the same thing, but the unit has to be one the
/// reader thinks in. An English interface used to say "8.8w chars", which is a
/// Chinese unit (万) wearing an English label.
pub fn amount(chars: usize, words: usize) -> String {
    if crate::i18n::current() == crate::i18n::Lang::Zh {
        if chars >= 10_000 {
            format!("{:.1}万字", chars as f64 / 10_000.0)
        } else {
            format!("{chars}字")
        }
    } else if words >= 1_000_000 {
        format!("{:.1}M words", words as f64 / 1_000_000.0)
    } else if words >= 1_000 {
        format!("{:.1}k words", words as f64 / 1_000.0)
    } else {
        format!("{words} words")
    }
}

/// The same, for a piece of text in hand.
pub fn amount_of(text: &str) -> String {
    amount(text.chars().count(), words_in(text))
}

#[cfg(test)]
mod amount_tests {
    use super::*;
    use crate::i18n::{self, Lang};

    /// The tests share one global language, so they take turns.
    fn exclusive() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn latin_words_come_from_word_boundaries() {
        assert_eq!(words_in("The first time I noticed"), 5);
        assert_eq!(words_in("don't stop — it's fine"), 4);
        assert_eq!(words_in("   "), 0);
    }

    /// A Chinese paragraph has no spaces, so counting each 字 as a word would
    /// call this sentence twenty words. It is closer to twelve.
    #[test]
    fn chinese_characters_convert_rather_than_counting_one_each() {
        let text = "从那儿出发，向东走三天，你便会抵达迪奥米拉";
        let chars = text.chars().filter(|c| !"，。".contains(*c)).count();
        let words = words_in(text);
        assert!(
            words < chars && words > chars / 3,
            "{words} words for {chars} characters is not a plausible conversion"
        );
    }

    #[test]
    fn mixed_scripts_add_up() {
        assert_eq!(words_in("readio 读书"), 1 + 1);
    }

    #[test]
    fn each_language_gets_its_own_unit() {
        let _guard = exclusive();
        let before = i18n::current();

        i18n::set(Lang::Zh);
        assert_eq!(amount(88_000, 55_000), "8.8万字");
        assert_eq!(amount(560, 350), "560字");

        i18n::set(Lang::En);
        assert_eq!(amount(88_000, 55_000), "55.0k words");
        assert_eq!(amount(560, 350), "350 words");

        i18n::set(before);
    }

    /// The bug this replaced: an English interface announcing a novel as
    /// "8.8w chars" — a Chinese unit with an English label.
    #[test]
    fn english_never_shows_the_chinese_unit() {
        let _guard = exclusive();
        let before = i18n::current();
        i18n::set(Lang::En);
        let text = amount(88_000, 55_000);
        i18n::set(before);
        assert!(!text.contains('w') || text.contains("words"));
        assert!(!text.contains('字'));
    }
}
