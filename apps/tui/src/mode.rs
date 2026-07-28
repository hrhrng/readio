//! The three ways readio moves through a book.
//!
//! A reader is doing one of three things: turning the pages themselves, letting
//! the text come at a set pace, or listening. Those used to be two independent
//! switches — `/auto` and `/tts` — which meant four states, one of which
//! ("listening, but the passages stop coming") made no sense. One mode with
//! three values cannot get into that state.
//!
//! The costume matters too: a coding agent shows its mode as a chip near the
//! prompt and cycles it with `shift+tab`, so readio does the same.

use crate::i18n::t;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Nothing advances by itself: `⏎` loads the next passage, and so does `↓`
    /// once the reader is at the bottom of what has arrived.
    #[default]
    Manual,
    /// Passages keep coming at the configured reveal speed.
    Auto,
    /// Read-aloud, which brings its own scrolling: the voice sets the pace and
    /// the highlight follows it.
    Speak,
}

impl Mode {
    /// The next mode `shift+tab` should offer.
    pub fn next(self) -> Self {
        match self {
            Mode::Manual => Mode::Auto,
            Mode::Auto => Mode::Speak,
            Mode::Speak => Mode::Manual,
        }
    }

    /// What the two underlying switches say, read back as a mode.
    pub fn of(auto: bool, speaking: bool) -> Self {
        match (speaking, auto) {
            (true, _) => Mode::Speak,
            (false, true) => Mode::Auto,
            (false, false) => Mode::Manual,
        }
    }

    /// Whether the text advances without being asked.
    pub fn scrolls(self) -> bool {
        matches!(self, Mode::Auto | Mode::Speak)
    }

    /// Whether the voice is what sets the pace.
    pub fn speaks(self) -> bool {
        self == Mode::Speak
    }

    /// Position in the cycle, for the app's "already explained this" record.
    pub fn index(self) -> usize {
        match self {
            Mode::Manual => 0,
            Mode::Auto => 1,
            Mode::Speak => 2,
        }
    }

    /// The spelling `/mode` writes and reads.
    pub fn code(self) -> &'static str {
        match self {
            Mode::Manual => "manual",
            Mode::Auto => "auto",
            Mode::Speak => "tts",
        }
    }

    /// Every spelling `/mode` accepts, generous on purpose: the reader should not
    /// have to remember whether it is `tts`, `speak` or `voice`.
    pub fn parse(arg: &str) -> Option<Self> {
        match arg.trim().to_ascii_lowercase().as_str() {
            "manual" | "m" | "step" | "hand" | "off" | "手动" => Some(Mode::Manual),
            "auto" | "a" | "scroll" | "自动" | "自动滚动" => Some(Mode::Auto),
            "tts" | "t" | "speak" | "voice" | "read" | "aloud" | "朗读" => Some(Mode::Speak),
            _ => None,
        }
    }

    /// The name used in sentences.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Manual => t("mode.manual"),
            Mode::Auto => t("mode.auto"),
            Mode::Speak => t("mode.tts"),
        }
    }

    /// The marker on the chip: one arrow for a mode that waits, two for a mode
    /// that runs on its own — the shape a coding agent uses for the same idea.
    pub fn glyph(self) -> &'static str {
        match self {
            Mode::Manual => crate::theme::PLAY,
            // Two arrows: it keeps going without being asked.
            Mode::Auto | Mode::Speak => "⏵⏵",
        }
    }

    /// The one word on the chip. Read-aloud gets a word rather than a musical
    /// note: a note in the corner of the screen is a media player, and this is
    /// supposed to look like an agent.
    pub fn chip_word(self) -> &'static str {
        match self {
            Mode::Manual => t("mode.chip_manual"),
            Mode::Auto => t("mode.chip_auto"),
            Mode::Speak => t("mode.chip_tts"),
        }
    }
}

/// The chip shown in the status row: an arrow for how much the reader has to do,
/// and one word for which mode that is.
///
/// Short by design — it shares a row with the reading hints, and a wide chip is
/// what makes those truncate. The pace is not here: it rides with the model name
/// as the reasoning effort, which is where a coding agent puts it.
pub fn chip(mode: Mode) -> String {
    format!("{} {}", mode.glyph(), mode.chip_word())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_tab_visits_every_mode_and_comes_back() {
        let mut mode = Mode::Manual;
        let mut seen = vec![mode];
        for _ in 0..3 {
            mode = mode.next();
            seen.push(mode);
        }
        assert_eq!(
            seen,
            vec![Mode::Manual, Mode::Auto, Mode::Speak, Mode::Manual]
        );
    }

    #[test]
    fn a_mode_is_just_a_reading_of_the_two_switches() {
        assert_eq!(Mode::of(false, false), Mode::Manual);
        assert_eq!(Mode::of(true, false), Mode::Auto);
        assert_eq!(Mode::of(true, true), Mode::Speak);
        // Speech implies scrolling, so the impossible fourth state reads as the
        // mode the reader meant.
        assert_eq!(Mode::of(false, true), Mode::Speak);
    }

    #[test]
    fn the_names_a_reader_might_type_all_land() {
        for spelling in ["manual", "STEP", " off ", "手动"] {
            assert_eq!(Mode::parse(spelling), Some(Mode::Manual), "{spelling}");
        }
        for spelling in ["auto", "scroll", "自动滚动"] {
            assert_eq!(Mode::parse(spelling), Some(Mode::Auto), "{spelling}");
        }
        for spelling in ["tts", "speak", "voice", "朗读"] {
            assert_eq!(Mode::parse(spelling), Some(Mode::Speak), "{spelling}");
        }
        assert_eq!(Mode::parse("faster"), None);
    }

    #[test]
    fn the_chip_stays_short_enough_to_share_a_row() {
        // The language is global, so this takes the same lock every other test
        // that switches it takes — and puts it back afterwards. Without that, a
        // test asserting on English output fails whenever this one happens to be
        // running beside it.
        let _guard = crate::i18n::exclusive();
        let before = crate::i18n::current();
        for lang in [crate::i18n::Lang::En, crate::i18n::Lang::Zh] {
            crate::i18n::set(lang);
            for mode in [Mode::Manual, Mode::Auto, Mode::Speak] {
                let chip = chip(mode);
                assert!(
                    crate::wrap::display_width(&chip) <= 12,
                    "{chip:?} is too wide for the status row"
                );
                assert!(!chip.contains('♪'), "no musical notes in an agent's chrome");
                // The chip is a piece of the mode's own name, never a second word
                // for the same thing: a status row reading "voice" beside a
                // sentence about "read-aloud" makes a reader go looking for two
                // features. This catches a rename that lands in only one place.
                let name = mode.name().to_lowercase();
                let short = chip.split_whitespace().next_back().unwrap_or("");
                assert!(
                    !short.is_empty() && name.contains(short),
                    "chip {short:?} is not part of the mode name {name:?}"
                );
            }
        }
        crate::i18n::set(before);
    }
}
