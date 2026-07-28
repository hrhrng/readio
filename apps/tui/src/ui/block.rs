//! Scrollback block types and their rendering.
//!
//! Each block knows *what* it is; the theme decides colour and the block
//! decides layout. Every block renders into owned `Line<'static>` values so the
//! scrollback can cache them between frames.

use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};

use crate::i18n::{t, tf};
use crate::metrics;
use crate::theme::{self, theme};
use crate::wrap::{Segment, display_width, truncate, wrap, wrap_indexed};

/// Fold state of a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Collapsed,
    Truncated,
    Expanded,
}

#[derive(Debug, Clone)]
pub struct Ctx {
    pub width: u16,
    pub tick: u64,
    pub running: bool,
    pub mode: Mode,
}

impl Ctx {
    /// Columns available after the two-column gutter.
    pub fn body_width(&self) -> usize {
        (self.width as usize).saturating_sub(2).max(8)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Read,
    Grep,
    ListDir,
    Locate,
    Write,
}

impl Verb {
    fn label(self) -> &'static str {
        match self {
            Verb::Read => "Read",
            Verb::Grep => "Grep",
            Verb::ListDir => "ListDir",
            Verb::Locate => "Locate",
            Verb::Write => "Write",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolStatus {
    Running,
    Ok { ms: u64 },
    Failed { message: String },
}

#[derive(Debug, Clone)]
pub struct Tool {
    pub verb: Verb,
    pub target: String,
    pub detail: Option<String>,
    /// Dim preview lines shown under the header.
    pub body: Vec<String>,
    pub status: ToolStatus,
}

impl Tool {
    pub fn new(verb: Verb, target: impl Into<String>) -> Self {
        Self {
            verb,
            target: target.into(),
            detail: None,
            body: Vec::new(),
            status: ToolStatus::Running,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn body(mut self, body: Vec<String>) -> Self {
        self.body = body;
        self
    }

    /// Mark the call as failed; the header turns red and shows `message`.
    pub fn failed(mut self, message: impl Into<String>) -> Self {
        self.status = ToolStatus::Failed {
            message: message.into(),
        };
        self
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    TurnComplete {
        ms: u64,
        chars: usize,
    },
    Interrupted,
    ChapterComplete {
        title: String,
        chars: usize,
    },
    BookComplete,
    Failed {
        message: String,
    },
    /// Something the reader should notice and can act on: a headline, then the
    /// detail and the commands that resolve it.
    Warning {
        title: String,
        lines: Vec<String>,
    },
}

/// Where the voice is in a passage: the sentence, and the word inside it.
///
/// Two levels rather than one because they answer different questions: the
/// sentence tells you where to look, the inner span tells you exactly where. Read
/// aloud uses it for the voice's position, letting your eye run slightly ahead;
/// search uses it for the term it found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Highlight {
    /// Byte range washed lightly: the sentence in question.
    pub sentence: (usize, usize),
    /// Byte range washed deeply: the word being sounded, or the term searched for.
    pub word: Option<(usize, usize)>,
}

impl Highlight {
    /// A sentence, with nothing singled out inside it.
    pub fn new(sentence: (usize, usize)) -> Self {
        Self {
            sentence,
            word: None,
        }
    }

    /// A sentence with one span picked out inside it.
    pub fn focused(sentence: (usize, usize), word: (usize, usize)) -> Self {
        Self {
            sentence,
            word: Some(word),
        }
    }
}

/// Shown inside the frame when an illustration cannot be drawn.
pub const PLACEHOLDER_LABEL: &str = "⛶";

/// Lines a picture block spends before its first row of pixels: one blank
/// separator and one caption.
pub const IMAGE_HEADER_LINES: u16 = 2;

/// Where an illustration's pixels belong inside a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePlan {
    pub path: std::path::PathBuf,
    pub cols: u16,
    pub rows: u16,
    /// Lines of this block that come before the picture.
    pub skip_lines: u16,
}

#[derive(Debug, Clone)]
pub struct PlanItem {
    /// Position in the whole book, not in this window: a plan showing chapters
    /// 30 to 38 has to say 30 to 38.
    pub number: usize,
    pub title: String,
    pub chars: usize,
    pub done: bool,
    pub current: bool,
}

/// The reading position, dressed as an agent's context-window readout.
#[derive(Debug, Clone)]
pub struct ContextInfo {
    pub book: String,
    pub chapter: String,
    pub chapter_index: usize,
    pub chapter_total: usize,
    pub progress: f32,
    pub chars_read: usize,
    pub chars_total: usize,
    /// Size of the chapter in progress, drawn as the live segment of the bar.
    pub chapter_chars: usize,
    pub session_chars: usize,
    pub cps: f32,
    pub elapsed: std::time::Duration,
    /// Speech engine, when reading aloud.
    pub engine: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LibraryRow {
    pub index: usize,
    pub title: String,
    pub author: Option<String>,
    pub chars: usize,
    pub progress: f32,
    pub mode: &'static str,
    /// Currently open book.
    pub current: bool,
    /// The file behind the entry is gone.
    pub missing: bool,
}

#[derive(Debug, Clone)]
pub enum Block {
    User(String),
    Thinking {
        text: String,
        elapsed_ms: Option<u64>,
    },
    Tool(Tool),
    /// Streamed book content. Understands `## `, `> ` and fenced code prefixes.
    Passage {
        text: String,
        /// Byte ranges the book emphasised, in this passage's coordinates. Set
        /// when the block is created: the text arrives a phrase at a time, but
        /// what is emphasised in it is known from the start.
        emphasis: Vec<crate::book::Emphasis>,
        /// What the voice is on, when speech is on.
        highlight: Option<Highlight>,
    },
    System(String),
    Event(Event),
    Plan {
        title: String,
        items: Vec<PlanItem>,
        /// Chapters left out of the window, reported rather than dropped
        /// silently. A book cut at its table of contents can have seventy of
        /// them, and a to-do list that long is furniture.
        hidden: usize,
    },
    Context(ContextInfo),
    /// An illustration from the book, drawn with half-block characters.
    ///
    /// The block only reserves rows and draws the caption; the pixels are
    /// painted by the scrollback afterwards, because they need cell access
    /// rather than lines of text.
    Image {
        path: std::path::PathBuf,
        alt: String,
        /// Source pixel size, read once when the block is made. `None` means
        /// the file could not be read, and a framed placeholder stands in.
        dims: Option<(u32, u32)>,
        /// Tallest the picture may be drawn, from `config.yaml`.
        max_rows: u16,
    },
    /// The imported-books listing.
    Library {
        rows: Vec<LibraryRow>,
        hint: String,
    },
}

impl Block {
    pub fn passage(emphasis: Vec<crate::book::Emphasis>) -> Self {
        Block::Passage {
            text: String::new(),
            emphasis,
            highlight: None,
        }
    }

    /// An illustration. Reads the file header once, here, so every later frame
    /// can lay the block out without touching the disk.
    pub fn image(path: std::path::PathBuf, alt: impl Into<String>, max_rows: u16) -> Self {
        let dims = crate::ui::image::dimensions(&path).ok();
        Block::Image {
            path,
            alt: alt.into(),
            dims,
            max_rows,
        }
    }

    /// Where the pixels go, given the available width: the picture's size in
    /// cells and how many of this block's lines come before it.
    ///
    /// `None` when the file is unreadable — the placeholder is drawn as text.
    pub fn image_plan(&self, width: u16) -> Option<ImagePlan> {
        let Block::Image {
            path,
            dims,
            max_rows,
            ..
        } = self
        else {
            return None;
        };
        let (w, h) = (*dims)?;
        let body = width.saturating_sub(2).max(1);
        let fit = crate::ui::image::fit_in(w, h, body, *max_rows);
        if fit.is_empty() {
            return None;
        }
        Some(ImagePlan {
            path: path.clone(),
            cols: fit.cols,
            rows: fit.rows,
            skip_lines: IMAGE_HEADER_LINES,
        })
    }

    /// A passage that is already complete, used by `/tts test`.
    pub fn passage_text(text: &str) -> Self {
        Block::Passage {
            text: text.to_string(),
            emphasis: Vec::new(),
            highlight: None,
        }
    }

    pub fn thinking() -> Self {
        Block::Thinking {
            text: String::new(),
            elapsed_ms: None,
        }
    }

    /// Append streamed text. Returns false when the block cannot take text.
    pub fn push_chunk(&mut self, chunk: &str) -> bool {
        match self {
            Block::Passage { text, .. } | Block::Thinking { text, .. } => {
                text.push_str(chunk);
                true
            }
            _ => false,
        }
    }

    pub fn char_len(&self) -> usize {
        match self {
            Block::Passage { text, .. } | Block::Thinking { text, .. } => text.chars().count(),
            _ => 0,
        }
    }

    /// Whether this block's appearance depends on the animation tick.
    pub fn animated(&self, running: bool) -> bool {
        running
            || matches!(
                self,
                Block::Tool(Tool {
                    status: ToolStatus::Running,
                    ..
                })
            )
    }

    pub fn set_elapsed(&mut self, ms: u64) {
        if let Block::Thinking { elapsed_ms, .. } = self {
            *elapsed_ms = Some(ms);
        }
    }

    /// Mark which bytes are being spoken right now.
    pub fn set_highlight(&mut self, state: Option<Highlight>) -> bool {
        match self {
            Block::Passage { highlight, .. } => {
                let changed = *highlight != state;
                *highlight = state;
                changed
            }
            _ => false,
        }
    }

    pub fn finish_tool(&mut self, ms: u64) {
        if let Block::Tool(tool) = self
            && tool.status == ToolStatus::Running
        {
            tool.status = ToolStatus::Ok { ms };
        }
    }

    pub fn render(&self, ctx: &Ctx) -> Vec<Line<'static>> {
        match self {
            Block::User(text) => render_user(text, ctx),
            Block::Thinking { text, elapsed_ms } => render_thinking(text, *elapsed_ms, ctx),
            Block::Tool(tool) => render_tool(tool, ctx),
            Block::Passage {
                text,
                emphasis,
                highlight,
            } => render_passage(text, emphasis, *highlight, ctx),
            Block::System(text) => render_system(text, ctx),
            Block::Event(event) => render_event(event, ctx),
            Block::Plan {
                title,
                items,
                hidden,
            } => render_plan(title, items, *hidden, ctx),
            Block::Context(info) => render_context(info, ctx),
            Block::Image {
                path,
                alt,
                dims,
                max_rows,
            } => render_image(path, alt, *dims, *max_rows, ctx),
            Block::Library { rows, hint } => render_library(rows, hint, ctx),
        }
    }
}

// ── renderers ────────────────────────────────────────────────────────────────

fn gutter(glyph: &str, color: ratatui::style::Color) -> Span<'static> {
    Span::styled(format!("{glyph} "), Style::default().fg(color))
}

fn indent() -> Span<'static> {
    Span::raw("  ")
}

fn render_user(text: &str, ctx: &Ctx) -> Vec<Line<'static>> {
    let th = theme();
    let mut out = vec![Line::from("")];
    for (i, line) in wrap(text, ctx.body_width()).into_iter().enumerate() {
        let mut spans = Vec::new();
        if i == 0 {
            spans.push(gutter(theme::ARROW, th.accent_user));
        } else {
            spans.push(indent());
        }
        spans.push(Span::styled(line, Style::default().fg(th.text_primary)));
        out.push(Line::from(spans));
    }
    out
}

fn render_thinking(text: &str, elapsed_ms: Option<u64>, ctx: &Ctx) -> Vec<Line<'static>> {
    let th = theme();
    let header_style = Style::default()
        .fg(th.accent_thinking)
        .add_modifier(Modifier::ITALIC);

    let header = if ctx.running {
        Line::from(vec![
            gutter(theme::spinner_frame(ctx.tick), th.accent_running),
            Span::styled(t("block.thinking").to_string(), header_style),
            Span::styled(
                theme::ELLIPSIS.to_string(),
                Style::default().fg(th.text_faint),
            ),
        ])
    } else {
        let label = match elapsed_ms {
            Some(ms) => tf(
                "block.thought_for",
                &[&format!("{:.1}", ms as f64 / 1000.0)],
            ),
            None => t("block.thought").to_string(),
        };
        Line::from(vec![
            gutter(theme::QUOTE_BAR, th.accent_thinking),
            Span::styled(label, Style::default().fg(th.text_muted)),
        ])
    };

    let mut out = vec![Line::from(""), header];
    if ctx.mode == Mode::Collapsed && !ctx.running {
        return out;
    }

    let body_style = Style::default()
        .fg(th.text_muted)
        .add_modifier(Modifier::ITALIC);
    let lines = wrap(text.trim(), ctx.body_width().saturating_sub(2));
    // While thinking, keep only the tail visible — the same trick the real
    // thing uses to stop reasoning from pushing content off screen.
    let visible: Vec<String> = if ctx.mode == Mode::Expanded {
        lines
    } else {
        let keep = 3;
        if lines.len() > keep {
            lines[lines.len() - keep..].to_vec()
        } else {
            lines
        }
    };
    for line in visible {
        if line.trim().is_empty() {
            continue;
        }
        out.push(Line::from(vec![
            Span::styled(
                format!("{} ", theme::QUOTE_BAR),
                Style::default().fg(th.accent_thinking),
            ),
            Span::styled(line, body_style),
        ]));
    }
    out
}

fn render_tool(tool: &Tool, ctx: &Ctx) -> Vec<Line<'static>> {
    let th = theme();
    let (bullet, bullet_color) = match &tool.status {
        ToolStatus::Running => (
            theme::spinner_frame(ctx.tick).to_string(),
            th.accent_running,
        ),
        ToolStatus::Ok { .. } => (theme::BULLET_DONE.to_string(), th.accent_tool),
        ToolStatus::Failed { .. } => (theme::CROSS.to_string(), th.accent_error),
    };

    let mut header = vec![
        gutter(&bullet, bullet_color),
        Span::styled(
            tool.verb.label().to_string(),
            Style::default().fg(th.text_secondary).bold(),
        ),
        Span::raw(" "),
    ];
    let fixed = display_width(tool.verb.label()) + 3;
    let detail_w = tool
        .detail
        .as_deref()
        .map(|d| display_width(d) + 2)
        .unwrap_or(0);
    let target_w = (ctx.width as usize)
        .saturating_sub(fixed + detail_w + 10)
        .max(12);
    header.push(Span::styled(
        truncate(&tool.target, target_w),
        Style::default().fg(th.text_primary),
    ));
    if let Some(detail) = &tool.detail {
        header.push(Span::styled(
            format!("  {detail}"),
            Style::default().fg(th.text_faint),
        ));
    }
    match &tool.status {
        ToolStatus::Ok { ms } if *ms > 0 => header.push(Span::styled(
            format!("  ·  {:.1}s", *ms as f64 / 1000.0),
            Style::default().fg(th.text_faint),
        )),
        ToolStatus::Failed { message } => header.push(Span::styled(
            format!("  ·  {message}"),
            Style::default().fg(th.accent_error),
        )),
        _ => {}
    }

    let mut out = vec![Line::from(""), Line::from(header)];
    if ctx.mode == Mode::Collapsed || tool.body.is_empty() {
        return out;
    }

    let limit = if ctx.mode == Mode::Expanded {
        tool.body.len()
    } else {
        6
    };
    let body_style = Style::default().fg(th.text_faint);
    for raw in tool.body.iter().take(limit) {
        for line in wrap(raw, ctx.body_width().saturating_sub(2)) {
            out.push(Line::from(vec![indent(), Span::styled(line, body_style)]));
        }
    }
    if tool.body.len() > limit {
        out.push(Line::from(vec![
            indent(),
            Span::styled(
                format!("{} +{} more", theme::ELLIPSIS, tool.body.len() - limit),
                Style::default().fg(th.text_faint),
            ),
        ]));
    }
    out
}

/// Book content. A tiny markdown dialect: `## ` heading, `> ` quote, ``` fence.
///
/// Two overlays ride on top of the text, both expressed as byte ranges into it:
/// what the book emphasised, and what the voice is saying. Keeping them as
/// ranges rather than as markup is what lets the text stay a plain string for
/// wrapping, pacing, search and speech.
fn render_passage(
    text: &str,
    emphasis: &[crate::book::Emphasis],
    highlight: Option<Highlight>,
    ctx: &Ctx,
) -> Vec<Line<'static>> {
    let th = theme();
    let width = ctx.body_width();
    let mut out = vec![Line::from("")];
    let mut in_code = false;
    // Byte offset of the current source line inside `text`.
    let mut base = 0usize;

    for raw in text.split('\n') {
        let advance = raw.len() + 1;
        let trimmed = raw.trim_start();
        let indent_len = raw.len() - trimmed.len();

        if trimmed.starts_with("```") {
            in_code = !in_code;
            base += advance;
            continue;
        }
        if in_code {
            out.push(Line::from(vec![
                indent(),
                Span::styled(
                    truncate(raw, width),
                    Style::default().fg(th.text_secondary).bg(th.bg_code),
                ),
            ]));
            base += advance;
            continue;
        }
        if raw.trim().is_empty() {
            out.push(Line::from(""));
            base += advance;
            continue;
        }

        // Marker width matters twice: it is not drawn, and the highlight offsets
        // have to stay aligned with the source text.
        let (marker, body_width, style, quote) = if trimmed.starts_with("## ") {
            (
                3,
                width,
                Style::default()
                    .fg(th.accent_agent)
                    .add_modifier(Modifier::BOLD),
                false,
            )
        } else if trimmed.starts_with("> ") {
            (
                2,
                width.saturating_sub(2),
                Style::default()
                    .fg(th.text_secondary)
                    .add_modifier(Modifier::ITALIC),
                true,
            )
        } else {
            (0, width, Style::default().fg(th.text_primary), false)
        };

        let origin = base + indent_len + marker;
        let body = &raw[indent_len + marker..];
        // Light wash for the sentence, strong wash plus bold for the word: two
        // steps that stay distinguishable even on a low-contrast terminal.
        let lit = Style::default().fg(th.fg_speaking).bg(th.bg_speaking);
        let hot = Style::default()
            .fg(th.fg_speaking)
            .bg(th.bg_speaking_word)
            .add_modifier(Modifier::BOLD);

        for segment in wrap_indexed(body, body_width) {
            let lead = if quote {
                Span::styled(
                    format!("{} ", theme::QUOTE_BAR),
                    Style::default().fg(th.accent_plan),
                )
            } else {
                indent()
            };
            let mut spans = vec![lead];
            spans.extend(styled_spans(
                &segment, origin, highlight, emphasis, style, lit, hot,
            ));
            out.push(Line::from(spans));
        }
        base += advance;
    }

    // Streaming cursor, so the tail never looks frozen.
    if ctx.running
        && ctx.tick % 8 < 5
        && let Some(last) = out.last_mut()
    {
        last.spans.push(Span::styled(
            "\u{258c}".to_string(),
            Style::default().fg(th.accent_agent),
        ));
    }
    out
}

/// Split one wrapped line into spans: plain text, what the book emphasised, the
/// sentence being read, and the word inside it.
///
/// `origin` is where this line's source begins in the passage, because
/// [`wrap_indexed`] reports offsets relative to the slice it was handed. The
/// line is cut at every boundary that falls inside it and each piece takes the
/// style of the innermost range containing it, which keeps the word highlight
/// correct even when it straddles a wrap.
///
/// The two overlays compose rather than compete: emphasis is a modifier, the
/// speech highlight is a colour, so an italicised phrase being read aloud is
/// both italic and washed.
fn styled_spans(
    segment: &Segment,
    origin: usize,
    highlight: Option<Highlight>,
    emphasis: &[crate::book::Emphasis],
    plain: Style,
    lit: Style,
    hot: Style,
) -> Vec<Span<'static>> {
    let (line_from, line_to) = (origin + segment.start, origin + segment.end);
    // Everything below is in offsets inside the text actually drawn: a wrap can
    // eat a space, so the rendered line is not the source slice byte for byte.
    let local = |offset: usize| floor_boundary(&segment.text, offset.saturating_sub(line_from));

    // Emphasis that touches this line at all, in local coordinates.
    let leaning: Vec<(usize, usize, bool)> = emphasis
        .iter()
        .filter(|span| span.end as usize > line_from && (span.start as usize) < line_to)
        .map(|span| {
            (
                local(span.start as usize),
                local((span.end as usize).max(span.start as usize)),
                span.strong,
            )
        })
        .filter(|(start, end, _)| end > start)
        .collect();

    let speech = highlight.filter(|h| h.sentence.1 > line_from && h.sentence.0 < line_to);
    if leaning.is_empty() && speech.is_none() {
        return vec![Span::styled(segment.text.clone(), plain)];
    }

    let sentence = speech.map(|h| (local(h.sentence.0), local(h.sentence.1.max(h.sentence.0))));
    let word = speech
        .and_then(|h| h.word)
        .filter(|(a, b)| *b > line_from && *a < line_to)
        .map(|(a, b)| (local(a), local(b.max(a))));

    let mut cuts = vec![0usize, segment.text.len()];
    if let Some((a, b)) = sentence {
        cuts.push(a);
        cuts.push(b);
    }
    if let Some((a, b)) = word {
        cuts.push(a);
        cuts.push(b);
    }
    for (start, end, _) in &leaning {
        cuts.push(*start);
        cuts.push(*end);
    }
    cuts.retain(|cut| *cut <= segment.text.len());
    cuts.sort_unstable();
    cuts.dedup();

    let mut spans = Vec::new();
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if end <= start {
            continue;
        }
        let mut style = if word.is_some_and(|(a, b)| start >= a && end <= b) {
            hot
        } else if sentence.is_some_and(|(a, b)| start >= a && end <= b) {
            lit
        } else {
            plain
        };
        if let Some((_, _, strong)) = leaning.iter().find(|(a, b, _)| start >= *a && end <= *b) {
            style = style.add_modifier(if *strong {
                Modifier::BOLD
            } else {
                Modifier::ITALIC
            });
        }
        spans.push(Span::styled(segment.text[start..end].to_string(), style));
    }
    if spans.is_empty() {
        spans.push(Span::styled(segment.text.clone(), plain));
    }
    spans
}

/// Nearest char boundary at or below `index`.
fn floor_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn render_system(text: &str, ctx: &Ctx) -> Vec<Line<'static>> {
    let th = theme();
    let mut out = vec![Line::from("")];
    for (i, line) in wrap(text, ctx.body_width()).into_iter().enumerate() {
        let lead = if i == 0 {
            gutter(theme::BULLET_OPEN, th.accent_system)
        } else {
            indent()
        };
        out.push(Line::from(vec![
            lead,
            Span::styled(line, Style::default().fg(th.text_secondary)),
        ]));
    }
    out
}

fn render_event(event: &Event, _ctx: &Ctx) -> Vec<Line<'static>> {
    let th = theme();
    let (glyph, color, text) = match event {
        Event::TurnComplete { ms, chars } => (
            theme::CHECK,
            th.text_faint,
            tf(
                "block.turn_done",
                &[
                    &metrics::format_tokens(metrics::tokens_from_chars(*chars)),
                    &format!("{:.1}", *ms as f64 / 1000.0),
                ],
            ),
        ),
        Event::Interrupted => (
            theme::CROSS,
            th.accent_warning,
            t("block.interrupted").to_string(),
        ),
        Event::ChapterComplete { title, chars } => (
            theme::CHECK,
            th.accent_success,
            tf(
                "block.chapter_done",
                &[
                    title,
                    &metrics::format_tokens(metrics::tokens_from_chars(*chars)),
                ],
            ),
        ),
        Event::BookComplete => (
            theme::CHECK,
            th.accent_success,
            t("block.book_done").to_string(),
        ),
        Event::Failed { message } => (theme::CROSS, th.accent_error, message.clone()),
        Event::Warning { title, lines } => {
            let mut out = vec![
                Line::from(""),
                Line::from(vec![
                    gutter(theme::WARNING, th.accent_warning),
                    Span::styled(
                        title.clone(),
                        Style::default()
                            .fg(th.accent_warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
            ];
            for line in lines {
                for wrapped in wrap(line, _ctx.body_width()) {
                    out.push(Line::from(vec![
                        indent(),
                        Span::styled(wrapped, Style::default().fg(th.text_secondary)),
                    ]));
                }
            }
            return out;
        }
    };
    vec![
        Line::from(""),
        Line::from(vec![
            gutter(glyph, color),
            Span::styled(text, Style::default().fg(color)),
        ]),
    ]
}

fn render_plan(title: &str, items: &[PlanItem], hidden: usize, ctx: &Ctx) -> Vec<Line<'static>> {
    let th = theme();
    let mut out = vec![
        Line::from(""),
        Line::from(vec![
            gutter("◆", th.accent_plan),
            Span::styled(
                title.to_string(),
                Style::default().fg(th.accent_plan).bold(),
            ),
        ]),
    ];
    for item in items.iter() {
        let (glyph, style) = if item.current {
            (
                theme::ARROW,
                Style::default()
                    .fg(th.text_primary)
                    .add_modifier(Modifier::BOLD),
            )
        } else if item.done {
            (theme::CHECK, Style::default().fg(th.text_muted))
        } else {
            (theme::BULLET_OPEN, Style::default().fg(th.text_faint))
        };
        let label = format!("{:>2}. {}", item.number, item.title);
        let meta = format!(
            "  {} tok",
            metrics::format_tokens(metrics::tokens_from_chars(item.chars))
        );
        let room = ctx
            .body_width()
            .saturating_sub(display_width(&meta) + 4)
            .max(10);
        out.push(Line::from(vec![
            indent(),
            Span::styled(
                format!("{glyph} "),
                Style::default().fg(if item.current {
                    th.accent_plan
                } else {
                    th.text_faint
                }),
            ),
            Span::styled(truncate(&label, room), style),
            Span::styled(meta, Style::default().fg(th.text_faint)),
        ]));
    }
    if hidden > 0 {
        out.push(Line::from(vec![
            indent(),
            Span::styled(
                format!("  {} +{hidden} more", theme::ELLIPSIS),
                Style::default().fg(th.text_faint),
            ),
        ]));
    }
    out
}

/// The imported-books listing: index, title, size, progress, hold mode.
/// Caption plus blank rows for the picture. The pixels land on the blank rows
/// in a second pass; a file we cannot read gets a framed placeholder instead,
/// which is also just text.
fn render_image(
    path: &std::path::Path,
    alt: &str,
    dims: Option<(u32, u32)>,
    max_rows: u16,
    ctx: &Ctx,
) -> Vec<Line<'static>> {
    let th = theme();
    let body = ctx.body_width();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".to_string());
    let label = if alt.trim().is_empty() { &name } else { alt };

    let caption = match dims {
        Some((w, h)) => tf(
            "block.image",
            &[&truncate(label, body.saturating_sub(18)), &w, &h],
        ),
        None => tf(
            "block.image_failed",
            &[&truncate(label, body.saturating_sub(8))],
        ),
    };

    let mut out = vec![
        Line::from(""),
        Line::from(vec![
            gutter(theme::IMAGE, th.accent_tool),
            Span::styled(caption, Style::default().fg(th.text_secondary)),
        ]),
    ];

    match dims {
        Some((w, h)) => {
            let fit = crate::ui::image::fit_in(w, h, body.min(u16::MAX as usize) as u16, max_rows);
            for _ in 0..fit.rows.max(1) {
                out.push(Line::from(""));
            }
        }
        // Three rows is enough for a frame with a label in it.
        None => {
            for _ in 0..3 {
                out.push(Line::from(""));
            }
        }
    }
    out
}

fn render_library(rows: &[LibraryRow], hint: &str, ctx: &Ctx) -> Vec<Line<'static>> {
    let th = theme();
    let mut out = vec![
        Line::from(""),
        Line::from(vec![
            gutter("◈", th.accent_model),
            Span::styled(
                tf("lib.title", &[&rows.len()]),
                Style::default().fg(th.accent_model).bold(),
            ),
        ]),
    ];

    if rows.is_empty() {
        out.push(Line::from(vec![
            indent(),
            Span::styled(
                t("lib.empty").to_string(),
                Style::default().fg(th.text_muted),
            ),
        ]));
    }

    for row in rows {
        let (glyph, glyph_color) = if row.missing {
            (theme::CROSS, th.accent_error)
        } else if row.current {
            (theme::ARROW, th.accent_plan)
        } else if row.progress >= 0.999 {
            (theme::CHECK, th.text_muted)
        } else {
            (theme::BULLET_OPEN, th.text_faint)
        };

        let meta = format!(
            "  {:>8}  {:>4}  {}",
            format!(
                "{} tok",
                metrics::format_tokens(metrics::tokens_from_chars(row.chars))
            ),
            format!("{:.0}%", row.progress * 100.0),
            row.mode
        );
        let mut label = format!("{:>2}. {}", row.index, row.title);
        if let Some(author) = &row.author {
            label.push_str(&format!("  {author}"));
        }
        let room = ctx
            .body_width()
            .saturating_sub(display_width(&meta) + 4)
            .max(10);
        // Pad to a fixed column so the size / progress / mode columns line up
        // across rows whose titles differ in width.
        let mut label = truncate(&label, room);
        let pad = room.saturating_sub(display_width(&label));
        label.push_str(&" ".repeat(pad));

        let title_style = if row.missing {
            Style::default().fg(th.text_faint)
        } else if row.current {
            Style::default()
                .fg(th.text_primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(th.text_secondary)
        };

        out.push(Line::from(vec![
            indent(),
            Span::styled(format!("{glyph} "), Style::default().fg(glyph_color)),
            Span::styled(label, title_style),
            Span::styled(meta, Style::default().fg(th.text_faint)),
        ]));
    }

    if !hint.is_empty() {
        out.push(Line::from(""));
        for line in wrap(hint, ctx.body_width()) {
            out.push(Line::from(vec![
                indent(),
                Span::styled(line, Style::default().fg(th.text_muted)),
            ]));
        }
    }
    out
}

fn render_context(info: &ContextInfo, ctx: &Ctx) -> Vec<Line<'static>> {
    let th = theme();
    let width = ctx.body_width().min(48);

    // Three segments: what has been read, the chapter in flight, what is free.
    let (read, chapter, _) =
        metrics::context_split(info.chars_read, info.chapter_chars, info.chars_total.max(1));
    let read_cells = ((read * width as f32).round() as usize).min(width);
    let chapter_cells = ((chapter * width as f32).round() as usize).min(width - read_cells);
    let free_cells = width - read_cells - chapter_cells;

    let total_tokens = metrics::tokens_from_chars(info.chars_total);
    let read_tokens = metrics::tokens_from_chars(info.chars_read);
    let chapter_tokens = metrics::tokens_from_chars(info.chapter_chars);

    let mut out = vec![
        Line::from(""),
        Line::from(vec![
            gutter("◈", th.accent_model),
            Span::styled(
                t("block.context_title").to_string(),
                Style::default().fg(th.accent_model).bold(),
            ),
            Span::styled(
                format!("  ctx {:.1}%", metrics::context_percent(info.progress)),
                Style::default().fg(th.text_faint),
            ),
        ]),
        Line::from(vec![
            indent(),
            Span::styled("█".repeat(read_cells), Style::default().fg(th.accent_model)),
            Span::styled(
                "█".repeat(chapter_cells),
                Style::default().fg(th.accent_plan),
            ),
            Span::styled("░".repeat(free_cells), Style::default().fg(th.text_faint)),
        ]),
    ];

    let mut rows = vec![
        (
            t("block.ctx_window"),
            format!(
                "{}  ·  {} tok",
                info.book,
                metrics::format_tokens(total_tokens)
            ),
        ),
        (
            t("block.ctx_used"),
            format!(
                "{} tok  ·  {:.1}%",
                metrics::format_tokens(read_tokens),
                metrics::context_percent(info.progress)
            ),
        ),
        (
            t("block.ctx_chapter"),
            format!(
                "{}  {}  ·  {} tok",
                tf(
                    "chrome.chapter",
                    &[&(info.chapter_index + 1), &info.chapter_total.max(1)]
                ),
                info.chapter,
                metrics::format_tokens(chapter_tokens)
            ),
        ),
        (
            t("block.ctx_session"),
            format!(
                "{} tok  ·  {}  ·  {:.0} tok/s",
                metrics::format_tokens(metrics::tokens_from_chars(info.session_chars)),
                metrics::format_duration(info.elapsed),
                metrics::tokens_per_second(info.cps)
            ),
        ),
    ];
    if let Some(engine) = &info.engine {
        rows.push((t("block.ctx_speech"), engine.clone()));
    }

    let label_width = rows
        .iter()
        .map(|(label, _)| display_width(label))
        .max()
        .unwrap_or(8)
        + 2;
    for (label, value) in rows {
        let pad = label_width.saturating_sub(display_width(label));
        out.push(Line::from(vec![
            indent(),
            Span::styled(
                format!("{label}{}", " ".repeat(pad)),
                Style::default().fg(th.text_faint),
            ),
            Span::styled(
                truncate(&value, ctx.body_width().saturating_sub(label_width + 3)),
                Style::default().fg(th.text_secondary),
            ),
        ]));
    }
    out
}
