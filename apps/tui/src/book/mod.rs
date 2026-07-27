//! Book model: the content side of the app, deliberately ignorant of the UI.

pub mod epub;
pub mod html;
pub mod media;
pub mod pdf;
pub mod sample;
pub mod text;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// A block of book content. Mirrors the handful of shapes that survive the
/// trip from XHTML to a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Para {
    Heading {
        level: u8,
        text: String,
    },
    Text(String),
    Quote(String),
    Code(String),
    /// An illustration. `src` is a path on disk — EPUB images are extracted to
    /// the host directory at load time, so rendering never has to reopen the
    /// archive. `alt` is the caption, and the only part of an image that has
    /// characters to read aloud.
    Image {
        src: PathBuf,
        alt: String,
    },
}

impl Para {
    pub fn text(&self) -> &str {
        match self {
            Para::Heading { text, .. } => text,
            Para::Text(t) | Para::Quote(t) | Para::Code(t) => t,
            Para::Image { alt, .. } => alt,
        }
    }

    pub fn char_count(&self) -> usize {
        self.text().chars().count()
    }

    pub fn word_count(&self) -> usize {
        crate::metrics::words_in(self.text())
    }

    pub fn is_image(&self) -> bool {
        matches!(self, Para::Image { .. })
    }
}

#[derive(Debug, Clone)]
pub struct Chapter {
    pub title: String,
    /// Source location, shown in tool calls (`OEBPS/ch03.xhtml`).
    pub href: String,
    pub paras: Vec<Para>,
}

impl Chapter {
    pub fn char_count(&self) -> usize {
        self.paras.iter().map(Para::char_count).sum()
    }

    pub fn word_count(&self) -> usize {
        self.paras.iter().map(Para::word_count).sum()
    }

    /// Line number a paragraph would occupy in the source file. Fabricated but
    /// stable, which is all a tool-call header needs.
    pub fn source_line(&self, para_idx: usize) -> usize {
        self.paras
            .iter()
            .take(para_idx)
            .map(|p| p.text().lines().count().max(1) + 1)
            .sum::<usize>()
            + 1
    }

    /// Lines this chapter occupies in its source file.
    pub fn line_count(&self) -> usize {
        self.source_line(self.paras.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Epub,
    Pdf,
    Text,
    Sample,
}

#[derive(Debug, Clone)]
pub struct Book {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub path: Option<PathBuf>,
    pub source: Source,
    pub chapters: Vec<Chapter>,
}

impl Book {
    /// Load whatever the path points at; `None` yields the built-in sample.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        Self::load_from(path, None)
    }

    /// Load a book, with a second directory to look in for its illustrations.
    ///
    /// Copying a Markdown book into the library copies the text but not the
    /// pictures beside it, so the original's directory stays a valid place to
    /// find them. `None` means "only look next to the book".
    pub fn load_from(path: Option<&Path>, assets: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(sample::book());
        };
        let path = std::fs::canonicalize(path)
            .with_context(|| format!("cannot resolve {}", path.display()))?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        let mut book = match ext.as_str() {
            "epub" => epub::load(&path)?,
            "pdf" => pdf::load(&path)?,
            "txt" | "md" | "markdown" => text::load(&path, assets)?,
            other => {
                anyhow::bail!("unsupported format `.{other}` (expected .epub, .pdf, .txt or .md)")
            }
        };
        book.id = content_id(&path)?;
        book.path = Some(path);
        Ok(book)
    }

    /// Extensions readio knows how to read.
    pub fn supports(path: &Path) -> bool {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        matches!(ext.as_str(), "epub" | "pdf" | "txt" | "md" | "markdown")
    }

    pub fn char_count(&self) -> usize {
        self.chapters.iter().map(Chapter::char_count).sum()
    }

    pub fn word_count(&self) -> usize {
        self.chapters.iter().map(Chapter::word_count).sum()
    }

    pub fn chapter(&self, idx: usize) -> Option<&Chapter> {
        self.chapters.get(idx)
    }

    /// Characters preceding `(chapter, para)`, used for the progress readout.
    pub fn chars_before(&self, chapter: usize, para: usize) -> usize {
        let mut total = 0;
        for (ci, ch) in self.chapters.iter().enumerate() {
            if ci < chapter {
                total += ch.char_count();
            } else if ci == chapter {
                total += ch
                    .paras
                    .iter()
                    .take(para)
                    .map(Para::char_count)
                    .sum::<usize>();
            }
        }
        total
    }

    pub fn progress(&self, chapter: usize, para: usize) -> f32 {
        let total = self.char_count().max(1);
        self.chars_before(chapter, para) as f32 / total as f32
    }

    /// Line number to quote in a tool call.
    ///
    /// EPUB chapters are separate documents, so each counts from its own line 1.
    /// A `.txt`, `.md` or `.pdf` book is one file, so lines accumulate across
    /// chapters — otherwise three hits in chapter 3 would all claim to be near
    /// the top.
    pub fn line_of(&self, chapter: usize, para: usize) -> usize {
        let Some(target) = self.chapters.get(chapter) else {
            return 1;
        };
        let local = target.source_line(para);
        match self.source {
            Source::Epub => local,
            Source::Pdf | Source::Text | Source::Sample => {
                let before: usize = self.chapters[..chapter]
                    .iter()
                    .map(Chapter::line_count)
                    .sum();
                before + local
            }
        }
    }

    /// Root of the fabricated URI space used in tool-call headers.
    pub fn uri_root(&self) -> String {
        match self.source {
            Source::Epub => format!("epub://{}", slug(&self.title)),
            Source::Pdf => format!("pdf://{}", slug(&self.title)),
            Source::Text => format!("file://{}", slug(&self.title)),
            Source::Sample => "readio://sample".to_string(),
        }
    }

    /// Full-text search over the whole book.
    ///
    /// Every occurrence counts, including several in one paragraph: a reader who
    /// is told "12 times" when the word appears 45 times has been misinformed,
    /// and `limit` only caps how many are handed back for display.
    pub fn search(&self, needle: &str, limit: usize) -> Hits {
        let folded_needle = fold(needle).0;
        let mut hits = Hits::default();
        if folded_needle.is_empty() {
            return hits;
        }
        for (ci, ch) in self.chapters.iter().enumerate() {
            for (pi, para) in ch.paras.iter().enumerate() {
                let text = para.text();
                let (folded, offsets) = fold(text);
                let mut found_here = 0usize;
                let mut from = 0usize;
                while let Some(rel) = folded[from..].find(&folded_needle) {
                    let start_folded = from + rel;
                    let end_folded = start_folded + folded_needle.len();
                    // Map folded offsets back: case and width folding can change
                    // byte lengths, so the original positions are recorded as the
                    // folded string is built rather than assumed to match.
                    let start = offsets[start_folded];
                    let end = offsets.get(end_folded).copied().unwrap_or(text.len());
                    found_here += 1;
                    hits.total += 1;
                    if hits.shown.len() < limit {
                        hits.shown.push(Hit {
                            chapter: ci,
                            para: pi,
                            range: (start, end),
                            excerpt: excerpt(text, start, end - start),
                        });
                    }
                    from = end_folded.max(start_folded + 1);
                }
                if found_here > 0 {
                    hits.paragraphs += 1;
                }
            }
        }
        hits
    }
}

/// One occurrence, with enough information to go there and light it up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub chapter: usize,
    pub para: usize,
    /// Byte range of the match inside the paragraph's text.
    pub range: (usize, usize),
    pub excerpt: String,
}

/// What a search found: the true totals, plus as many hits as were asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hits {
    /// Every occurrence in the book.
    pub total: usize,
    /// Paragraphs containing at least one.
    pub paragraphs: usize,
    /// The first `limit` of them, for display.
    pub shown: Vec<Hit>,
}

impl Hits {
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// True when there are more occurrences than were handed back.
    pub fn truncated(&self) -> bool {
        self.total > self.shown.len()
    }
}

/// Case- and width-folded copy of `text`, plus the original byte offset of every
/// byte in it (and one past the end).
///
/// Searching a lowercased copy and reusing its offsets is wrong in general:
/// lowercasing changes byte lengths for some characters, and full-width forms are
/// three bytes where their ASCII equivalents are one. Recording the mapping while
/// folding keeps "ＣＩＴＹ" findable as "city" and the highlight in the right place.
fn fold(text: &str) -> (String, Vec<usize>) {
    let mut folded = String::with_capacity(text.len());
    let mut offsets = Vec::with_capacity(text.len() + 1);
    for (index, c) in text.char_indices() {
        let plain = match c {
            // Full-width ASCII forms sit at U+FF01..U+FF5E, exactly 0xFEE0 above
            // their ASCII counterparts. The ideographic space folds to a space.
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            '\u{3000}' => ' ',
            other => other,
        };
        for lowered in plain.to_lowercase() {
            let before = folded.len();
            folded.push(lowered);
            offsets.resize(folded.len(), index);
            debug_assert!(offsets.len() > before);
        }
    }
    offsets.push(text.len());
    (folded, offsets)
}

/// A window of text around a match, snapped to character boundaries.
fn excerpt(text: &str, at: usize, needle_len: usize) -> String {
    let start = text[..at]
        .char_indices()
        .rev()
        .take(18)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(at);
    let tail_from = (at + needle_len).min(text.len());
    let end = text[tail_from..]
        .char_indices()
        .take(40)
        .last()
        .map(|(i, c)| tail_from + i + c.len_utf8())
        .unwrap_or(tail_from);
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(text[start..end].trim());
    if end < text.len() {
        out.push('…');
    }
    out
}

/// Stable per-book key for the progress store.
///
/// Derived from the file's contents, not its path, so importing a book (which
/// copies or moves it into the host directory) keeps the reading progress that
/// was recorded before the import.
pub fn content_id(path: &Path) -> Result<String> {
    use std::io::Read;

    let mut file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);

    // FNV-1a over the length plus a 128 KiB prefix: cheap, and two different
    // books colliding on both is not a realistic concern here.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    };
    mix(&len.to_le_bytes());

    let mut buffer = vec![0u8; 8 * 1024];
    let mut read_total = 0usize;
    while read_total < 128 * 1024 {
        let n = file.read(&mut buffer).unwrap_or(0);
        if n == 0 {
            break;
        }
        mix(&buffer[..n]);
        read_total += n;
    }
    Ok(format!("{hash:016x}"))
}

/// Fallback key for content that has no file behind it (the built-in sample).
pub fn synthetic_id(label: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in label.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// ASCII-safe short name for URIs; CJK titles collapse to their first chars.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars().take(24) {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => out.push(c),
            ' ' | '.' | '/' => out.push('-'),
            c if (c as u32) > 0x2E7F => out.push(c),
            _ => {}
        }
    }
    if out.is_empty() {
        out.push_str("book");
    }
    out
}

#[cfg(test)]
mod search_tests {
    use super::*;

    fn book_of(paras: &[&str]) -> Book {
        Book {
            id: "test".into(),
            title: "t".into(),
            author: None,
            source: Source::Sample,
            path: None,
            chapters: vec![Chapter {
                title: "one".into(),
                href: "one.html".into(),
                paras: paras
                    .iter()
                    .map(|text| Para::Text(text.to_string()))
                    .collect(),
            }],
        }
    }

    /// The bug this replaces: `find()` stopped at the first match in a paragraph,
    /// so a book with 45 occurrences was reported as having 12.
    #[test]
    fn every_occurrence_counts_even_several_in_one_paragraph() {
        let book = book_of(&["城市和记忆，记忆里的城市，还有记忆。", "无关的一段。"]);
        let hits = book.search("记忆", 10);
        assert_eq!(hits.total, 3, "three occurrences in one paragraph");
        assert_eq!(hits.paragraphs, 1, "all of them in the same paragraph");
        assert_eq!(hits.shown.len(), 3);
        assert!(!hits.truncated());
    }

    #[test]
    fn the_limit_caps_the_list_but_not_the_count() {
        let many = "记忆记忆记忆记忆记忆记忆";
        let book = book_of(&[many]);
        let hits = book.search("记忆", 2);
        assert_eq!(hits.total, 6, "the count is the truth about the book");
        assert_eq!(hits.shown.len(), 2, "the list is only what was asked for");
        assert!(hits.truncated(), "and it must admit to being partial");
    }

    #[test]
    fn ranges_point_at_the_match_itself() {
        let book = book_of(&["a city of memory", "MEMORY again"]);
        let hits = book.search("memory", 10);
        assert_eq!(hits.total, 2);
        assert_eq!(hits.paragraphs, 2);
        for hit in &hits.shown {
            let text = book.chapters[0].paras[hit.para].text();
            assert_eq!(
                text[hit.range.0..hit.range.1].to_lowercase(),
                "memory",
                "the range has to be usable as a highlight"
            );
        }
    }

    /// Full-width forms are what a Chinese keyboard produces for Latin letters
    /// and digits, and folding them keeps a search from silently missing them.
    #[test]
    fn case_and_width_fold_together() {
        let book = book_of(&["ＣＩＴＹ of glass", "the City of Glass"]);
        let hits = book.search("city", 10);
        assert_eq!(hits.total, 2, "full-width and capitalised both match");
        let first = &hits.shown[0];
        let text = book.chapters[0].paras[first.para].text();
        assert_eq!(
            &text[first.range.0..first.range.1],
            "ＣＩＴＹ",
            "the range must span the original characters, not the folded ones"
        );
    }

    #[test]
    fn an_empty_needle_finds_nothing_rather_than_everything() {
        let book = book_of(&["anything"]);
        assert!(book.search("", 10).is_empty());
        assert!(book.search("   ", 10).is_empty());
    }

    #[test]
    fn overlapping_needles_do_not_loop_forever() {
        let book = book_of(&["aaaa"]);
        let hits = book.search("aa", 10);
        assert_eq!(hits.total, 2, "non-overlapping matches: aa|aa");
    }
}
