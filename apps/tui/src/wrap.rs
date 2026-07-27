//! Display-width aware wrapping.
//!
//! `textwrap` is built around whitespace-separated words, which is wrong for
//! Chinese: a 400-character paragraph with no spaces would never break. This
//! wrapper measures East-Asian width, breaks anywhere between CJK graphemes,
//! prefers the last space inside Latin runs, and refuses to orphan a closing
//! punctuation mark at the start of a line.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Characters that must not begin a line (CJK line-break rules, abridged).
const NO_LINE_START: &str = "，。、；：？！）】》」』”’%,.;:?!)]}>…～";
/// Characters that must not end a line.
const NO_LINE_END: &str = "（【《「『“‘([{<";

pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

fn is_cjk(g: &str) -> bool {
    g.chars().next().is_some_and(|c| {
        matches!(c as u32,
            0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)
    })
}

/// One wrapped line, plus the byte range of the source text it came from.
///
/// The range is what makes it possible to highlight "the sentence being read
/// aloud": the speech layer knows byte offsets into the passage, and the
/// renderer needs to know which columns those offsets landed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// Wrap `text` to `width` columns. Never returns an empty vector.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    wrap_indexed(text, width)
        .into_iter()
        .map(|segment| segment.text)
        .collect()
}

/// Wrap `text`, reporting where each line came from in the input.
pub fn wrap_indexed(text: &str, width: usize) -> Vec<Segment> {
    let width = width.max(4);
    let mut out: Vec<Segment> = Vec::new();
    // Byte offset of the current source line within `text`.
    let mut base = 0usize;

    for raw_line in text.split('\n') {
        if raw_line.trim().is_empty() {
            out.push(Segment {
                text: String::new(),
                start: base,
                end: base + raw_line.len(),
            });
            base += raw_line.len() + 1;
            continue;
        }

        let mut line = String::new();
        let mut line_w = 0usize;
        // Where the current output line starts, as a byte offset in `raw_line`.
        let mut line_start = 0usize;
        // Last breakable space: (byte offset in `line`, byte offset in `raw_line`).
        let mut last_space: Option<(usize, usize)> = None;
        // Start offset of the grapheme pushed most recently.
        let mut prev_start: Option<usize> = None;

        let emit = |out: &mut Vec<Segment>, text: String, start: usize, end: usize| {
            out.push(Segment {
                text,
                start: base + start,
                end: base + end,
            });
        };

        for (idx, g) in raw_line.grapheme_indices(true) {
            let gw = display_width(g);

            if line_w + gw > width && line_w > 0 {
                let prev_ends_badly = line
                    .graphemes(true)
                    .next_back()
                    .is_some_and(|p| NO_LINE_END.contains(p));

                if NO_LINE_START.contains(g) {
                    // Carry the preceding grapheme down together with the mark:
                    // the punctuation never starts a line and the margin still
                    // holds. Only possible when something stays behind.
                    let tail = line.grapheme_indices(true).next_back();
                    if let (Some((cut, prev)), Some(prev_raw)) = (tail, prev_start)
                        && cut > 0
                    {
                        let carried = prev.to_string();
                        line.truncate(cut);
                        emit(&mut out, std::mem::take(&mut line), line_start, prev_raw);
                        line = carried;
                        line.push_str(g);
                        line_w = display_width(&line);
                        line_start = prev_raw;
                        prev_start = Some(idx);
                        last_space = None;
                        continue;
                    }
                }

                // Latin text: rewind to the last space so words stay intact.
                let split_at = if prev_ends_badly {
                    None
                } else {
                    last_space.filter(|_| !is_cjk(g))
                };
                match split_at {
                    Some((cut, raw_cut)) => {
                        let rest = line[cut + 1..].to_string();
                        line.truncate(cut);
                        emit(&mut out, std::mem::take(&mut line), line_start, raw_cut);
                        line = rest;
                        line_w = display_width(&line);
                        // The break consumed the space itself.
                        line_start = raw_cut + 1;
                    }
                    None => {
                        emit(&mut out, std::mem::take(&mut line), line_start, idx);
                        line_w = 0;
                        line_start = idx;
                    }
                }
                last_space = None;
            }

            if g == " " && line_w > 0 {
                last_space = Some((line.len(), idx));
            }
            prev_start = Some(idx);
            line.push_str(g);
            line_w += gw;
        }
        emit(&mut out, line, line_start, raw_line.len());
        base += raw_line.len() + 1;
    }

    if out.is_empty() {
        out.push(Segment {
            text: String::new(),
            start: 0,
            end: 0,
        });
    }
    out
}

/// Truncate to `width` columns, appending an ellipsis when it does not fit.
pub fn truncate(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    if width <= 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for g in text.graphemes(true) {
        let gw = display_width(g);
        if w + gw > width - 1 {
            break;
        }
        out.push_str(g);
        w += gw;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No line may exceed the target width — the check `textwrap` fails on CJK.
    fn assert_fits(lines: &[String], width: usize) {
        for line in lines {
            assert!(
                display_width(line) <= width,
                "line {line:?} is {} columns, limit {width}",
                display_width(line)
            );
        }
    }

    #[test]
    fn breaks_chinese_without_spaces() {
        let text = "注意力不是水桶里的水，而是被挖出来的河道，形状决定了它往哪里流。";
        let lines = wrap(text, 20);
        assert!(lines.len() > 1, "should wrap: {lines:?}");
        assert_fits(&lines, 20);
        let joined: String = lines.concat();
        assert_eq!(joined, text, "no characters may be lost");
    }

    #[test]
    fn keeps_latin_words_intact() {
        let lines = wrap("the quick brown fox jumps over the lazy dog", 12);
        assert_fits(&lines, 12);
        for line in &lines {
            assert!(
                !line.trim().is_empty() && !line.contains("  "),
                "unexpected spacing in {line:?}"
            );
        }
        assert!(
            lines.iter().all(|l| !l.starts_with(' ')),
            "no leading spaces: {lines:?}"
        );
    }

    #[test]
    fn never_starts_a_line_with_closing_punctuation() {
        // Width chosen so a naive wrapper would push the comma to a new line.
        for width in 6..24 {
            let lines = wrap("他说，界面不是中立的，它一边呈现，一边塑造。", width);
            for line in &lines {
                let first = line.chars().next();
                assert!(
                    !matches!(first, Some('，') | Some('。')),
                    "width {width}: line {line:?} starts with punctuation"
                );
            }
        }
    }

    #[test]
    fn preserves_blank_lines_between_paragraphs() {
        let lines = wrap("第一段。\n\n第二段。", 20);
        assert_eq!(lines, vec!["第一段。", "", "第二段。"]);
    }

    #[test]
    fn truncate_measures_columns_not_bytes() {
        assert_eq!(truncate("短", 10), "短");
        let cut = truncate("这是一个很长的中文标题需要截断", 10);
        assert!(display_width(&cut) <= 10, "{cut} too wide");
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn handles_degenerate_widths() {
        assert!(!wrap("文字", 0).is_empty());
        assert_eq!(truncate("文字", 1), "…");
        assert_eq!(wrap("", 10), vec![String::new()]);
    }

    /// Every wrapped line must point back at the text it came from, or the
    /// read-aloud highlight would land on the wrong words.
    #[test]
    fn segments_map_back_onto_the_source() {
        let text = "注意力不是水桶里的水，而是被挖出来的河道。\n\n第二段在这里，也要能对上。";
        for width in [8, 13, 20, 40] {
            for segment in wrap_indexed(text, width) {
                assert!(
                    segment.start <= segment.end && segment.end <= text.len(),
                    "width {width}: bad range {segment:?}"
                );
                let source = &text[segment.start..segment.end];
                assert!(
                    source.contains(segment.text.trim()) || segment.text.trim().is_empty(),
                    "width {width}: line {:?} is not inside its source slice {source:?}",
                    segment.text
                );
            }
        }
    }

    #[test]
    fn segment_ranges_advance_and_cover_the_text() {
        let text = "the quick brown fox jumps over the lazy dog";
        let segments = wrap_indexed(text, 12);
        let mut previous_end: usize = 0;
        for segment in &segments {
            assert!(
                segment.start >= previous_end.saturating_sub(1),
                "ranges should move forward: {segment:?} after {previous_end}"
            );
            previous_end = segment.end;
        }
        assert_eq!(
            segments.last().map(|s| s.end),
            Some(text.len()),
            "the last line should reach the end of the text"
        );
    }

    #[test]
    fn wrap_and_wrap_indexed_agree() {
        let text = "混合 mixed 内容，用来检查两个入口是否一致。\n第二行。";
        for width in [6, 11, 25] {
            let plain = wrap(text, width);
            let indexed: Vec<String> = wrap_indexed(text, width)
                .into_iter()
                .map(|s| s.text)
                .collect();
            assert_eq!(plain, indexed, "width {width}");
        }
    }
}
