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

    /// Product identity shared with the Web dark theme.
    pub accent_brand: Color,
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
    bg_base: rgb(28, 31, 29),
    bg_soft: rgb(37, 41, 37),
    bg_code: rgb(32, 36, 33),

    text_primary: rgb(244, 247, 244),
    text_secondary: rgb(192, 200, 193),
    text_muted: rgb(156, 166, 158),
    // Faint still has to be readable: this is the status-line hint colour, not
    // a decorative border. It clears 4.5:1 against `bg_base`.
    text_faint: rgb(127, 137, 129),

    accent_brand: rgb(138, 214, 169),
    accent_user: rgb(192, 200, 193),
    accent_agent: rgb(138, 214, 169),
    accent_thinking: rgb(122, 166, 139),
    accent_tool: rgb(156, 166, 158),
    accent_system: rgb(122, 162, 247),
    accent_running: rgb(138, 214, 169),
    accent_success: rgb(145, 215, 163),
    accent_error: rgb(255, 180, 171),
    accent_warning: rgb(239, 196, 119),
    accent_plan: rgb(239, 196, 119),
    accent_model: rgb(155, 205, 228),

    border: rgb(58, 64, 59),
    border_focus: rgb(91, 101, 93),

    bg_speaking: rgb(45, 67, 53),
    bg_speaking_word: rgb(73, 111, 87),
    fg_speaking: rgb(244, 247, 244),

    bg_selected: rgb(48, 54, 48),
    fg_latin: rgb(155, 205, 228),
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
/// The cross-platform lockup: the command prompt plus a stable cursor.
pub const BRAND_MARK: &str = "❯_";
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
/// chrome would announce a media player, and the chip says `⏵⏵ aloud` instead.
pub const NOTE_MUTED: &str = "⊘";
pub const ELLIPSIS: &str = "…";

pub fn spinner_frame(tick: u64) -> &'static str {
    SPINNER[(tick as usize / 2) % SPINNER.len()]
}

pub fn dot_frame(tick: u64) -> &'static str {
    DOT_SPINNER[(tick as usize / 4) % DOT_SPINNER.len()]
}
