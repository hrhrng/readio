//! The prompt: a small grapheme-aware line editor with history.
//!
//! Deliberately not a full textarea — a reading session needs one line, a
//! cursor that lands where you expect in CJK text, and command history.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block as UiBlock, BorderType, Borders, Paragraph, Widget};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme::{self, theme};
use crate::wrap::{display_width, wrap};

pub struct Prompt {
    buf: String,
    /// Byte offset of the caret inside `buf`; always on a grapheme boundary.
    cursor: usize,
    history: Vec<String>,
    /// Position while browsing history; `None` means editing a fresh line.
    browse: Option<usize>,
    stash: String,
    pub placeholder: String,
}

impl Default for Prompt {
    fn default() -> Self {
        Self::new()
    }
}

impl Prompt {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            cursor: 0,
            history: Vec::new(),
            browse: None,
            stash: String::new(),
            placeholder: "回车继续阅读，或提问 / 输入命令".to_string(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.buf.trim().is_empty()
    }

    pub fn insert_str(&mut self, text: &str) {
        let cleaned: String = text.replace(['\n', '\r', '\t'], " ");
        self.buf.insert_str(self.cursor, &cleaned);
        self.cursor += cleaned.len();
        self.browse = None;
    }

    pub fn insert_char(&mut self, c: char) {
        self.buf.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.browse = None;
    }

    pub fn backspace(&mut self) {
        if let Some(prev) = self.prev_boundary() {
            self.buf.replace_range(prev..self.cursor, "");
            self.cursor = prev;
        }
    }

    pub fn delete(&mut self) {
        if let Some(next) = self.next_boundary() {
            self.buf.replace_range(self.cursor..next, "");
        }
    }

    pub fn left(&mut self) {
        if let Some(prev) = self.prev_boundary() {
            self.cursor = prev;
        }
    }

    pub fn right(&mut self) {
        if let Some(next) = self.next_boundary() {
            self.cursor = next;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.buf.len();
    }

    /// Ctrl+U — clear everything before the caret.
    pub fn kill_to_start(&mut self) {
        self.buf.replace_range(..self.cursor, "");
        self.cursor = 0;
    }

    /// Ctrl+W — delete the word (or CJK run) before the caret.
    pub fn kill_word(&mut self) {
        let head = &self.buf[..self.cursor];
        let trimmed = head.trim_end();
        let start = trimmed
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        self.buf.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
        self.browse = None;
    }

    /// Take the current line, recording it in history.
    pub fn take(&mut self) -> String {
        let text = std::mem::take(&mut self.buf);
        self.cursor = 0;
        self.browse = None;
        let trimmed = text.trim().to_string();
        if !trimmed.is_empty() && self.history.last() != Some(&trimmed) {
            self.history.push(trimmed.clone());
        }
        trimmed
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.browse {
            None => {
                self.stash = self.buf.clone();
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.browse = Some(next);
        self.buf = self.history[next].clone();
        self.cursor = self.buf.len();
    }

    pub fn history_next(&mut self) {
        match self.browse {
            None => {}
            Some(i) if i + 1 >= self.history.len() => {
                self.browse = None;
                self.buf = std::mem::take(&mut self.stash);
                self.cursor = self.buf.len();
            }
            Some(i) => {
                self.browse = Some(i + 1);
                self.buf = self.history[i + 1].clone();
                self.cursor = self.buf.len();
            }
        }
    }

    fn prev_boundary(&self) -> Option<usize> {
        self.buf[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)
    }

    fn next_boundary(&self) -> Option<usize> {
        self.buf[self.cursor..]
            .grapheme_indices(true)
            .next()
            .map(|(_, g)| self.cursor + g.len())
    }

    /// Rows needed, including both border lines.
    pub fn height(&self, width: u16) -> u16 {
        let inner = (width as usize).saturating_sub(4).max(8);
        let rows = wrap(&self.buf, inner).len().clamp(1, 6);
        rows as u16 + 2
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, busy: bool) {
        let th = theme();
        let border_color = if busy { th.border } else { th.border_focus };
        let frame = UiBlock::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color));
        let inner = frame.inner(area);
        frame.render(area, buf);

        let width = inner.width.saturating_sub(2).max(8) as usize;
        let arrow = Span::styled(
            format!("{} ", theme::ARROW),
            Style::default().fg(if busy { th.text_faint } else { th.accent_agent }),
        );

        if self.buf.is_empty() {
            let hint = Span::styled(self.placeholder.clone(), Style::default().fg(th.text_faint));
            Paragraph::new(Line::from(vec![arrow, hint])).render(inner, buf);
            self.draw_caret(inner, buf, 0, 0);
            return;
        }

        let rows = wrap(&self.buf, width);
        let lines: Vec<Line<'static>> = rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let lead = if i == 0 {
                    arrow.clone()
                } else {
                    Span::raw("  ")
                };
                Line::from(vec![
                    lead,
                    Span::styled(row.clone(), Style::default().fg(th.text_primary)),
                ])
            })
            .collect();
        Paragraph::new(lines).render(inner, buf);

        let (row, col) = self.caret_position(&rows);
        self.draw_caret(inner, buf, row, col);
    }

    /// Map the byte cursor onto the wrapped rows.
    fn caret_position(&self, rows: &[String]) -> (usize, usize) {
        let before = &self.buf[..self.cursor];
        let mut remaining = before.graphemes(true).count();
        for (row_idx, row) in rows.iter().enumerate() {
            let row_graphemes = row.graphemes(true).count();
            if remaining <= row_graphemes && row_idx + 1 == rows.len() {
                return (row_idx, display_width(&take_graphemes(row, remaining)));
            }
            if remaining < row_graphemes {
                return (row_idx, display_width(&take_graphemes(row, remaining)));
            }
            remaining -= row_graphemes;
            // Wrapping may have eaten a space; keep the caret from drifting.
            if remaining > 0 && before.len() > row.len() {
                remaining = remaining.saturating_sub(0);
            }
        }
        let last = rows.len().saturating_sub(1);
        (
            last,
            display_width(rows.last().map(String::as_str).unwrap_or("")),
        )
    }

    fn draw_caret(&self, inner: Rect, buf: &mut Buffer, row: usize, col: usize) {
        let x = inner.x + 2 + col as u16;
        let y = inner.y + row as u16;
        if x >= inner.right() || y >= inner.bottom() {
            return;
        }
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_style(
                Style::default()
                    .fg(theme().bg_base)
                    .bg(theme().accent_agent)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
}

fn take_graphemes(s: &str, n: usize) -> String {
    s.graphemes(true).take(n).collect()
}
