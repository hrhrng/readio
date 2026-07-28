//! Palette and chrome glyphs.
//!
//! The colour choices follow the same semantic slots a coding agent's TUI
//! needs — one accent per speaker role, one per tool state — so a block only
//! has to say *what it is* and the theme decides how it looks.

use ratatui::style::Color;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

pub struct Theme {
    pub bg_base: Color,
    pub bg_soft: Color,
    pub bg_code: Color,

    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_faint: Color,

    pub accent_user: Color,
    pub accent_agent: Color,
    pub accent_thinking: Color,
    pub accent_tool: Color,
    pub accent_system: Color,
    pub accent_running: Color,
    pub accent_success: Color,
    pub accent_error: Color,
    pub accent_warning: Color,
    pub accent_plan: Color,
    pub accent_model: Color,

    pub border: Color,
    pub border_focus: Color,

    /// Wash under the sentence being read aloud.
    pub bg_speaking: Color,
    /// Stronger wash under the word being sounded right now.
    pub bg_speaking_word: Color,
    /// Foreground for that sentence, a step brighter than body text.
    pub fg_speaking: Color,

    /// Row under the cursor in the slash-command menu.
    pub bg_selected: Color,
    /// Latin words and numbers sitting inside Chinese prose. A coding agent
    /// tints identifiers; a book full of `Invisible Cities, 1972` gets the same
    /// treatment, and the page reads the way a tool's output reads.
    pub fg_latin: Color,
}

pub const THEME: Theme = Theme {
    bg_base: rgb(22, 22, 24),
    bg_soft: rgb(30, 30, 34),
    bg_code: rgb(28, 28, 32),

    text_primary: rgb(214, 216, 224),
    text_secondary: rgb(158, 162, 176),
    text_muted: rgb(118, 122, 136),
    text_faint: rgb(84, 88, 100),

    accent_user: rgb(176, 180, 194),
    accent_agent: rgb(196, 152, 245),
    accent_thinking: rgb(148, 128, 190),
    accent_tool: rgb(104, 110, 128),
    accent_system: rgb(122, 162, 247),
    accent_running: rgb(196, 152, 245),
    accent_success: rgb(126, 208, 158),
    accent_error: rgb(232, 118, 118),
    accent_warning: rgb(226, 186, 120),
    accent_plan: rgb(255, 219, 141),
    accent_model: rgb(116, 199, 196),

    border: rgb(52, 54, 62),
    border_focus: rgb(88, 92, 104),

    bg_speaking: rgb(44, 40, 62),
    bg_speaking_word: rgb(92, 74, 148),
    fg_speaking: rgb(242, 240, 248),

    bg_selected: rgb(40, 40, 48),
    fg_latin: rgb(122, 194, 214),
};

pub fn theme() -> &'static Theme {
    &THEME
}

// ── chrome glyphs ────────────────────────────────────────────────────────────

/// Braille spinner, one column per frame.
pub const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

/// Quiet dot spinner used for low-priority work (progress saves, prefetch).
pub const DOT_SPINNER: [&str; 4] = ["⋅", ":", "⸬", "⁙"];

pub const BULLET_DONE: &str = "●";
pub const BULLET_OPEN: &str = "○";
pub const CHECK: &str = "✓";
pub const CROSS: &str = "✗";
pub const ARROW: &str = "❯";
/// One arrow: something is running, or a mode that runs on its own. Two of them
/// mean "and it keeps going", the way a coding agent marks its unattended mode.
pub const PLAY: &str = "⏵";
pub const QUOTE_BAR: &str = "❙";
/// Marks an illustration's caption.
pub const IMAGE: &str = "⛶";
/// Marks something the reader should notice and can act on.
pub const WARNING: &str = "⚠";
/// Read-aloud is on but held back — the output device is not allowed. A plain
/// single-width glyph, because a combining slash over anything lands wherever the
/// terminal feels like putting it.
///
/// There is deliberately no glyph for "read-aloud is on": a musical note in the
/// chrome would announce a media player, and the chip says `⏵⏵ voice` instead.
pub const NOTE_MUTED: &str = "⊘";
pub const ELLIPSIS: &str = "…";

pub fn spinner_frame(tick: u64) -> &'static str {
    SPINNER[(tick as usize / 2) % SPINNER.len()]
}

pub fn dot_frame(tick: u64) -> &'static str {
    DOT_SPINNER[(tick as usize / 4) % DOT_SPINNER.len()]
}
