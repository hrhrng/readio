//! Splitting a passage into utterances for the speech engine.
//!
//! Speech wants contextual windows: complete paragraphs where they fit, and
//! complete sentences where they do not. Each utterance keeps the byte range it
//! came from, so when its audio starts the reader can highlight those words.

/// One thing to say, and where it sits in the passage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utterance {
    /// Text handed to the engine, with markup removed.
    pub text: String,
    /// Byte range inside the passage the text came from.
    pub range: (usize, usize),
}

/// Sentence-final punctuation, Chinese and ASCII.
const HARD_STOP: &str = "。！？…";
/// Preferred synthesis-window ceiling. A complete sentence may exceed it: a
/// semantic boundary is worth more than an arbitrary size guarantee.
const MAX_CHARS: usize = 90;
/// Below this, prefer to keep gluing clauses together. Chinese sentences are
/// short — 「界面不是中立的。」 is eight characters — so the floor has to be low
/// enough not to swallow them, while still refusing to synthesize 「好。」 alone.
const MIN_CHARS: usize = 6;

/// Split a passage into utterances.
///
/// Understands the passage dialect used by the reader: `## ` headings, `> `
/// quotes, and fenced code. Code fences are skipped — reading source aloud is
/// noise — and the markers themselves are never spoken.
pub fn split(passage: &str) -> Vec<Utterance> {
    pack_windows(passage, paragraphs(passage))
}

/// Fine-grained boundaries used by transport navigation.
///
/// Synthesis deliberately packs these into larger contextual windows; player
/// navigation must retain the smaller semantic units so `←` and `→` do not
/// unexpectedly jump an entire paragraph.
pub fn sentences(passage: &str) -> Vec<Utterance> {
    paragraphs(passage)
        .into_iter()
        .flat_map(|paragraph| paragraph.clauses)
        .collect()
}

fn paragraphs(passage: &str) -> Vec<Paragraph> {
    let mut paragraphs: Vec<Paragraph> = Vec::new();
    let mut in_code = false;
    let mut barrier_before = false;
    // Byte offset of the current line within `passage`.
    let mut base = 0usize;

    for line in passage.split('\n') {
        let len = line.len();
        let advance = len + 1;
        let trimmed = line.trim_start();
        let indent = len - trimmed.len();

        if trimmed.starts_with("```") {
            in_code = !in_code;
            // Context must not jump across omitted source: the two sides of a
            // code sample are unrelated even when both happen to be short.
            barrier_before = true;
            base += advance;
            continue;
        }
        if in_code || trimmed.trim().is_empty() {
            base += advance;
            continue;
        }

        // Skip the marker so the engine does not say "hash hash".
        let marker = if trimmed.starts_with("## ") {
            3
        } else if trimmed.starts_with("> ") {
            2
        } else {
            0
        };
        let start = base + indent + marker;
        let body = &line[indent + marker..];
        let clauses = split_line(body, start);
        if !clauses.is_empty() {
            paragraphs.push(Paragraph {
                clauses,
                barrier_before: std::mem::take(&mut barrier_before),
            });
        }
        base += advance;
    }
    paragraphs
}

struct Paragraph {
    clauses: Vec<Utterance>,
    barrier_before: bool,
}

/// Recursive text splitting, stopping at the sentence level:
///
/// 1. Keep a paragraph whole when it fits the preferred window.
/// 2. Otherwise pack its complete sentences up to that window.
/// 3. Never hard-cut a sentence merely to satisfy the preferred size.
///
/// The resulting paragraph-sized pieces are then greedily packed with their
/// neighbours. This is the useful part of a RAG text splitter for TTS: rich
/// context without sacrificing the linguistic boundaries the voice needs.
fn pack_windows(passage: &str, paragraphs: Vec<Paragraph>) -> Vec<Utterance> {
    let mut windows = Vec::new();
    let mut current: Option<Utterance> = None;
    for paragraph in paragraphs {
        let pieces = split_paragraph(passage, paragraph.clauses);
        for (index, piece) in pieces.into_iter().enumerate() {
            let Some(open) = current.take() else {
                current = Some(piece);
                continue;
            };
            let candidate = join(open.clone(), piece.clone(), passage);
            if (paragraph.barrier_before && index == 0)
                || candidate.text.chars().count() > MAX_CHARS
            {
                windows.push(open);
                current = Some(piece);
            } else {
                current = Some(candidate);
            }
        }
    }
    if let Some(open) = current {
        windows.push(open);
    }
    windows
}

fn split_paragraph(passage: &str, clauses: Vec<Utterance>) -> Vec<Utterance> {
    let Some(first) = clauses.first().cloned() else {
        return Vec::new();
    };
    let whole = clauses
        .iter()
        .skip(1)
        .cloned()
        .fold(first, |open, clause| join(open, clause, passage));
    if whole.text.chars().count() <= MAX_CHARS || clauses.len() == 1 {
        return vec![whole];
    }

    let mut pieces = Vec::new();
    let mut current: Option<Utterance> = None;
    for clause in clauses {
        let Some(open) = current.take() else {
            current = Some(clause);
            continue;
        };
        let candidate = join(open.clone(), clause.clone(), passage);
        if candidate.text.chars().count() > MAX_CHARS {
            pieces.push(open);
            current = Some(clause);
        } else {
            current = Some(candidate);
        }
    }
    if let Some(open) = current {
        pieces.push(open);
    }
    pieces
}

fn join(mut first: Utterance, second: Utterance, passage: &str) -> Utterance {
    let source_gap = passage
        .get(first.range.1..second.range.0)
        .unwrap_or_default();
    let last = first.text.chars().last();
    let separator = if last.is_some_and(|ch| HARD_STOP.contains(ch)) {
        ""
    } else if last.is_some_and(|ch| matches!(ch, '.' | '!' | '?')) {
        " "
    } else if source_gap.is_empty() {
        ""
    } else if first.text.chars().any(crate::metrics::is_ideographic) {
        "。"
    } else {
        ". "
    };
    first.text.push_str(separator);
    first.text.push_str(second.text.trim_start());
    first.range.1 = second.range.1;
    first
}

/// Split one line of prose into utterances, offset by `origin`.
fn split_line(line: &str, origin: usize) -> Vec<Utterance> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut current_start = 0usize;
    let mut chars = 0usize;

    let push = |text: &mut String, start: usize, end: usize, out: &mut Vec<Utterance>| {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            out.push(Utterance {
                text: trimmed.to_string(),
                range: (origin + start, origin + end),
            });
        }
        text.clear();
    };

    for (index, ch) in line.char_indices() {
        if current.is_empty() {
            current_start = index;
        }
        current.push(ch);
        chars += 1;

        let end = index + ch.len_utf8();
        let hard = HARD_STOP.contains(ch)
            || (matches!(ch, '.' | '!' | '?')
                && line[end..].chars().next().is_none_or(|n| n == ' ')
                && !period_abbreviation(&current));
        if hard && chars >= MIN_CHARS {
            push(&mut current, current_start, end, &mut out);
            chars = 0;
        }
    }
    let end = line.len();
    push(&mut current, current_start, end, &mut out);
    out
}

fn period_abbreviation(text: &str) -> bool {
    let token = text
        .trim_end_matches('.')
        .split_whitespace()
        .next_back()
        .unwrap_or_default()
        .trim_matches(|ch: char| !ch.is_alphanumeric())
        .to_lowercase();
    matches!(
        token.as_str(),
        "e.g"
            | "i.e"
            | "etc"
            | "trans"
            | "ed"
            | "eds"
            | "p"
            | "pp"
            | "vol"
            | "no"
            | "mr"
            | "mrs"
            | "ms"
            | "dr"
            | "prof"
            | "ph"
    ) || (token.len() == 1 && token.chars().all(|ch| ch.is_ascii_alphabetic()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(passage: &str) -> Vec<String> {
        split(passage).into_iter().map(|u| u.text).collect()
    }

    #[test]
    fn an_oversized_paragraph_splits_only_on_sentence_endings() {
        let passage = "第一句先交代这座城市为何建在河流旁边。第二句描述黄昏时广场上的钟声和人群。第三句转向窗边那位始终沉默的老人。第四句补充远山在雨后显露出来的轮廓。第五句才揭示旅行者一直等待的答案。第六句让船夫把这个答案带往下一座城市。";
        let said = texts(passage);
        assert!(said.len() > 1, "oversized paragraph stayed whole: {said:?}");
        assert!(
            said.iter().all(|window| window.ends_with('。')),
            "a window ended inside a sentence: {said:?}"
        );
        assert_eq!(said.concat(), passage, "text was lost or duplicated");
    }

    #[test]
    fn punctuation_is_a_preferred_boundary_not_a_synthesis_window() {
        let passage = "第一句话提供背景。第二句话继续展开。第三句话收住这一小段。";
        assert_eq!(
            texts(passage),
            vec![passage],
            "each full stop became a separate synthesis request"
        );
    }

    #[test]
    fn packed_english_sentences_keep_their_word_boundary() {
        let passage = "This is one sentence. And here is another one.";
        assert_eq!(
            texts(passage),
            vec![passage],
            "packing removed the space between English sentences"
        );
    }

    #[test]
    fn a_complete_paragraph_is_not_split_to_fill_the_previous_window() {
        let passage = "第一段先交代这座城市的背景，也说明旅行者为何会在黄昏时抵达这里，最后留下一个尚未回答的问题。\n\n第二段第一句描述广场上的人群和钟声。第二句转向窗边沉默的老人。第三句补充远处河流反射出来的微光。第四句才揭示他们正在等待什么。";
        assert_eq!(
            texts(passage),
            vec![
                "第一段先交代这座城市的背景，也说明旅行者为何会在黄昏时抵达这里，最后留下一个尚未回答的问题。",
                "第二段第一句描述广场上的人群和钟声。第二句转向窗边沉默的老人。第三句补充远处河流反射出来的微光。第四句才揭示他们正在等待什么。",
            ],
            "a paragraph that fits the preferred window was split merely to fill the previous one"
        );
    }

    #[test]
    fn ranges_point_back_at_the_passage() {
        let passage = "第一句。第二句稍微长一点，也应该对得上。";
        for utterance in split(passage) {
            let slice = &passage[utterance.range.0..utterance.range.1];
            assert_eq!(
                slice.trim(),
                utterance.text,
                "range {:?} does not match the text",
                utterance.range
            );
        }
    }

    #[test]
    fn strips_markup_but_keeps_the_words() {
        let said = texts("## 二 阅读的进度条\n\n> 界面不是中立的。\n\n正文在这里。");
        assert_eq!(
            said,
            vec!["二 阅读的进度条。界面不是中立的。正文在这里。"],
            "markup leaked into speech or its words were lost"
        );
    }

    /// A heading or year is context for what follows, not a useful TTS clip on
    /// its own. MOSS can turn a bare `1983` into several seconds of unstable
    /// speech, which is exactly what happened at an EPUB bibliography boundary.
    #[test]
    fn short_lines_are_joined_to_the_sentence_that_follows() {
        let said = texts("## 五\n\n选择马可波罗的故事，有什么意蕴呢？");
        assert_eq!(
            said,
            vec!["五。选择马可波罗的故事，有什么意蕴呢？"],
            "a one-character heading became its own synthesis job"
        );
    }

    #[test]
    fn bibliographic_abbreviations_do_not_become_tiny_clips() {
        let said = texts(
            "参考文献\n\nBaudrillard, Jean\n\n1983\n\nSimulations. trans. by Paul Foss etc. New York: Semiotext(e).",
        );
        assert!(
            said.iter()
                .all(|text| !matches!(text.as_str(), "1983" | "trans." | "etc.")),
            "bibliographic fragments leaked out as standalone clips: {said:?}"
        );
        let joined = said.join(" ");
        for expected in [
            "参考文献",
            "Baudrillard, Jean",
            "1983",
            "Simulations",
            "trans.",
            "Paul Foss",
            "etc.",
            "New York",
        ] {
            assert!(joined.contains(expected), "{expected:?} was lost: {said:?}");
        }
        assert!(
            said.iter().all(|window| {
                let chars = window.chars().count();
                chars <= MAX_CHARS || !window.contains(['。', '.', '!', '?'])
            }),
            "a multi-sentence window exceeded the preferred ceiling: {said:?}"
        );
    }

    #[test]
    fn code_is_not_read_aloud() {
        let said = texts("前面。\n\n```\nfn main() {}\n```\n\n后面。");
        assert_eq!(said, vec!["前面。", "后面。"], "code should be skipped");
    }

    #[test]
    fn a_sentence_is_never_hard_split_to_meet_the_window_limit() {
        let passage = "注意力".repeat(120);
        let said = texts(&passage);
        assert_eq!(
            said,
            vec![passage],
            "a sentence was cut at an arbitrary character boundary"
        );
    }

    #[test]
    fn short_clauses_are_not_shattered() {
        // "好。" alone is too short to be worth its own synthesis call.
        let said = texts("好。真的。这句稍微长一些，可以独立成句。");
        assert!(
            said.len() <= 2,
            "tiny fragments should be glued together: {said:?}"
        );
    }

    #[test]
    fn nothing_to_say_is_not_an_error() {
        assert!(texts("").is_empty());
        assert!(texts("\n\n   \n").is_empty());
        assert!(texts("```\ncode only\n```").is_empty());
    }
}

/// One step of the reading cursor: a word in a Latin script, a single character
/// in Chinese or Japanese.
///
/// Chinese has no spaces to break on, and a character is the unit a reader's eye
/// actually moves over, so per-character is both the only option and the right
/// one. Latin words are kept whole because a highlight crawling through
/// `attention` letter by letter reads as a stutter, not as speech.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unit {
    /// Byte range inside whatever text was passed in.
    pub range: (usize, usize),
    /// Characters in the unit, used to share out the clip's duration.
    pub chars: usize,
}

/// Whether a character is a letter written without word spacing (CJK, kana,
/// Hangul). Punctuation in those ranges — 。、！ — is deliberately excluded, so
/// it attaches to the character before it instead of taking a step of its own.
fn is_ideographic(c: char) -> bool {
    c.is_alphanumeric()
        && matches!(c as u32,
        0x2E80..=0x9FFF      // CJK radicals through unified ideographs
        | 0xA960..=0xA97F    // Hangul jamo extended
        | 0xAC00..=0xD7AF    // Hangul syllables
        | 0xF900..=0xFAFF    // CJK compatibility ideographs
        | 0xFF00..=0xFF60    // fullwidth forms
        | 0x1_0000..=0x1_FFFF
        )
}

/// Split text into cursor units, in order and covering every non-space run.
///
/// Whitespace and punctuation are attached to the unit before them rather than
/// becoming units of their own: a highlight that pauses on a comma looks like a
/// bug, and the comma's time is better spent on the word it follows.
pub fn units(text: &str) -> Vec<Unit> {
    use unicode_segmentation::UnicodeSegmentation;

    let mut out: Vec<Unit> = Vec::new();
    for (offset, word) in text.split_word_bound_indices() {
        if word.chars().all(char::is_whitespace) {
            continue;
        }
        // Punctuation-only run: glue it onto the previous unit.
        if word
            .chars()
            .all(|c| !c.is_alphanumeric() && !is_ideographic(c))
            && let Some(last) = out.last_mut()
        {
            last.range.1 = offset + word.len();
            continue;
        }
        if word.chars().next().is_some_and(is_ideographic) {
            // No spaces to break on: one unit per character.
            for (index, c) in word.char_indices() {
                out.push(Unit {
                    range: (offset + index, offset + index + c.len_utf8()),
                    chars: 1,
                });
            }
            continue;
        }
        out.push(Unit {
            range: (offset, offset + word.len()),
            chars: word.chars().count(),
        });
    }
    out
}

/// The unit being sounded after `progress` of a clip has played, where
/// `progress` runs 0.0 → 1.0.
///
/// Time is shared out by character count rather than by unit count, so a long
/// English word holds the highlight longer than `a` does — which is what the
/// audio does too.
pub fn unit_at(units: &[Unit], progress: f32) -> Option<Unit> {
    if units.is_empty() {
        return None;
    }
    let total: usize = units.iter().map(|u| u.chars).sum();
    if total == 0 {
        return units.first().copied();
    }
    let target = (progress.clamp(0.0, 1.0) * total as f32).min(total as f32 - 0.001);
    let mut seen = 0f32;
    for unit in units {
        seen += unit.chars as f32;
        if target < seen {
            return Some(*unit);
        }
    }
    units.last().copied()
}

#[cfg(test)]
mod cursor_tests {
    use super::*;

    #[test]
    fn chinese_advances_one_character_at_a_time() {
        let text = "界面不是中立的。";
        let units = units(text);
        assert_eq!(
            units.len(),
            7,
            "seven characters, the stop glued on: {units:?}"
        );
        assert_eq!(&text[units[0].range.0..units[0].range.1], "界");
        let last = units.last().expect("a last unit");
        assert_eq!(
            &text[last.range.0..last.range.1],
            "的。",
            "the full stop rides along with the character before it"
        );
        assert!(
            units.iter().all(|u| u.chars == 1),
            "each character carries one character of time"
        );
    }

    #[test]
    fn latin_advances_word_by_word() {
        let text = "An interface is never neutral.";
        let units = units(text);
        let words: Vec<&str> = units.iter().map(|u| &text[u.range.0..u.range.1]).collect();
        assert_eq!(
            words,
            vec!["An", "interface", "is", "never", "neutral."],
            "punctuation joins the word it follows"
        );
        assert_eq!(units[1].chars, 9, "time is shared out by characters");
    }

    #[test]
    fn mixed_scripts_split_at_the_boundary() {
        let text = "用 ratatui 画界面";
        let words: Vec<&str> = units(text)
            .iter()
            .map(|u| &text[u.range.0..u.range.1])
            .collect();
        assert_eq!(words, vec!["用", "ratatui", "画", "界", "面"]);
    }

    #[test]
    fn the_cursor_walks_from_the_first_unit_to_the_last() {
        let text = "界面不是中立的。";
        let units = units(text);
        let first = unit_at(&units, 0.0).expect("start");
        let last = unit_at(&units, 1.0).expect("end");
        assert_eq!(first, units[0], "progress 0 is the first unit");
        assert_eq!(
            last,
            *units.last().expect("last"),
            "progress 1 is the last unit, not past the end"
        );

        // Monotonic: the cursor never moves backwards.
        let mut previous = 0usize;
        for step in 0..=20 {
            let unit = unit_at(&units, step as f32 / 20.0).expect("a unit");
            assert!(
                unit.range.0 >= previous,
                "cursor went backwards at {step}: {unit:?}"
            );
            previous = unit.range.0;
        }
    }

    #[test]
    fn a_long_word_holds_the_cursor_longer_than_a_short_one() {
        let text = "a internationalisation b";
        let units = units(text);
        let long = units[1];
        let held = (0..100)
            .filter(|step| unit_at(&units, *step as f32 / 100.0) == Some(long))
            .count();
        assert!(
            held > 60,
            "the long word should hold most of the clip: {held}%"
        );
    }

    #[test]
    fn empty_and_whitespace_text_have_no_units() {
        assert!(units("").is_empty());
        assert!(units("   \n  ").is_empty());
        assert_eq!(unit_at(&[], 0.5), None, "no units means no cursor");
    }
}
