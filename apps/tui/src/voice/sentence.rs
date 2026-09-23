//! Splitting a passage into utterances for the speech engine.
//!
//! Speech wants contextual windows: complete paragraphs where they fit, and
//! complete sentences where they do not. Each utterance keeps the byte range it
//! came from, so when its audio starts the reader can highlight those words.

use readio_sdk::ReadioSdk;
use readio_sdk::documents::{Block, BlockKind, Document};
use readio_sdk::highlight::{DocumentTextIndex, HighlightCursor};

/// One thing to say, and where it sits in the passage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utterance {
    /// Text handed to the engine, with markup removed.
    pub text: String,
    /// Byte range inside the passage the text came from.
    pub range: (usize, usize),
}

/// Rust-owned sentence/word index plus the terminal-only source mapping.
///
/// Core ranges count Unicode scalars inside semantic blocks. The terminal
/// renderer needs UTF-8 byte ranges inside the original passage; `sources` is
/// the one-way adapter between those coordinate systems.
pub struct HighlightPlan {
    document: Document,
    index: DocumentTextIndex,
    sources: Vec<BlockSource>,
}

struct BlockSource {
    block_id: String,
    source_start: usize,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightRange {
    pub sentence: (usize, usize),
    pub word: Option<(usize, usize)>,
}

impl HighlightPlan {
    pub fn new(passage: &str) -> Option<Self> {
        let lines = readable_lines(passage);
        if lines.is_empty() {
            return None;
        }
        let mut document = Document::default();
        let mut sources = Vec::with_capacity(lines.len());
        for (index, line) in lines.into_iter().enumerate() {
            let block_id = format!("tui-passage-{index}");
            document.blocks.push(Block {
                id: block_id.clone(),
                kind: BlockKind::Paragraph,
                text: line.body.to_string(),
                inline_spans: Vec::new(),
            });
            sources.push(BlockSource {
                block_id,
                source_start: line.start,
                text: line.body.to_string(),
            });
        }
        let index = ReadioSdk::new().documents.index_text(&document);
        (!index.sentences.is_empty()).then_some(Self {
            document,
            index,
            sources,
        })
    }

    /// Resolve one media-clock sample through the shared Rust highlighter.
    pub fn at(&self, progress: f32) -> Option<HighlightRange> {
        let total = self
            .index
            .sentences
            .iter()
            .map(|sentence| sentence.text.chars().count() as u64)
            .sum::<u64>();
        if total == 0 {
            return None;
        }
        let target =
            ((progress.clamp(0.0, 1.0) as f64 * total as f64) as u64).min(total.saturating_sub(1));
        let mut seen = 0_u64;
        let sentence = self.index.sentences.iter().find(|sentence| {
            let end = seen.saturating_add(sentence.text.chars().count() as u64);
            let contains = target < end;
            if !contains {
                seen = end;
            }
            contains
        })?;
        let sentence_length = sentence.text.chars().count().max(1) as u64;
        let word_progress_basis_points =
            ((target.saturating_sub(seen) * 10_000) / sentence_length).min(10_000) as u16;
        let target = ReadioSdk::new()
            .documents
            .resolve_highlight(
                &self.document,
                &self.index,
                HighlightCursor {
                    document_revision: self.document.revision.revision_digest.clone(),
                    sentence_index: sentence.index,
                    word_index: None,
                    word_progress_basis_points: Some(word_progress_basis_points),
                },
            )
            .ok()?;
        let source = self
            .sources
            .iter()
            .find(|source| source.block_id == target.block_id)?;
        Some(HighlightRange {
            sentence: source_range(source, target.sentence_range)?,
            word: target
                .word_range
                .and_then(|range| source_range(source, range)),
        })
    }
}

fn source_range(source: &BlockSource, range: [u64; 2]) -> Option<(usize, usize)> {
    Some((
        source.source_start + scalar_to_byte(&source.text, range[0])?,
        source.source_start + scalar_to_byte(&source.text, range[1])?,
    ))
}

fn scalar_to_byte(text: &str, scalar: u64) -> Option<usize> {
    let scalar = usize::try_from(scalar).ok()?;
    if scalar == text.chars().count() {
        return Some(text.len());
    }
    text.char_indices().nth(scalar).map(|(offset, _)| offset)
}

/// Sentence-final punctuation, Chinese and ASCII.
const HARD_STOP: &str = "。！？…";
/// Preferred synthesis-window ceiling. A complete sentence may exceed it: a
/// semantic boundary is worth more than an arbitrary size guarantee.
const MAX_CHARS: usize = 90;
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
    readable_lines(passage)
        .into_iter()
        .filter_map(|line| {
            let clauses = split_line(line.body, line.start);
            (!clauses.is_empty()).then_some(Paragraph {
                clauses,
                barrier_before: line.barrier_before,
            })
        })
        .collect()
}

struct SourceLine<'a> {
    body: &'a str,
    start: usize,
    barrier_before: bool,
}

fn readable_lines(passage: &str) -> Vec<SourceLine<'_>> {
    let mut lines = Vec::new();
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
        let body = line[indent + marker..].trim_end();
        if !body.is_empty() {
            lines.push(SourceLine {
                body,
                start,
                barrier_before: std::mem::take(&mut barrier_before),
            });
        }
        base += advance;
    }
    lines
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
    let mut document = Document::default();
    document.blocks.push(Block {
        id: "tui-fine-sentence".to_string(),
        kind: BlockKind::Paragraph,
        text: line.to_string(),
        inline_spans: Vec::new(),
    });
    ReadioSdk::new()
        .documents
        .index_text(&document)
        .sentences
        .into_iter()
        .filter_map(|sentence| {
            let start = scalar_to_byte(line, sentence.range[0])?;
            let end = scalar_to_byte(line, sentence.range[1])?;
            Some(Utterance {
                text: sentence.text,
                range: (origin + start, origin + end),
            })
        })
        .collect()
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

    #[test]
    fn fine_sentences_follow_the_shared_rust_text_index() {
        let fine = sentences("好。真的。");
        assert_eq!(
            fine.iter()
                .map(|sentence| sentence.text.as_str())
                .collect::<Vec<_>>(),
            vec!["好。", "真的。"],
            "TUI navigation must not keep a second sentence inside the first Rust range"
        );
    }

    #[test]
    fn shared_highlight_maps_unicode_scalars_back_to_source_bytes() {
        let text = "Next🙂 word?";
        let plan = HighlightPlan::new(text).expect("shared text index");
        let first = plan.at(0.0).expect("opening highlight");
        let last = plan.at(0.8).expect("later highlight");

        assert_eq!(&text[first.sentence.0..first.sentence.1], text);
        assert_eq!(
            first.word.map(|range| &text[range.0..range.1]),
            Some("Next")
        );
        assert_eq!(last.word.map(|range| &text[range.0..range.1]), Some("word"));
    }
}
