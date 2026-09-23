//! Book model: the content side of the app, deliberately ignorant of the UI.

pub mod epub;
pub mod html;
pub mod media;
pub mod pdf;
pub mod sample;
pub mod text;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use readio_sdk::ReadioSdk;
use readio_sdk::documents::{Document as CoreDocument, HtmlParseOptions};
use readio_sdk::position::{Affinity, TextLocator};
use readio_sdk::search::SearchRequest;

/// A run of emphasised text inside a paragraph.
///
/// Byte range into the paragraph's own text, so the text itself stays a plain
/// string: wrapping, pacing, search and read-aloud all keep treating a paragraph
/// as characters, and emphasis is something the renderer lays over the top —
/// exactly how the read-aloud highlight already works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Emphasis {
    pub start: u32,
    pub end: u32,
    /// `<strong>` and `<b>` rather than `<em>`, `<i>` and `<cite>`.
    pub strong: bool,
}

/// Paragraph text, with whatever the source emphasised inside it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rich {
    pub text: String,
    pub emphasis: Vec<Emphasis>,
}

impl Rich {
    pub fn new(text: String, emphasis: Vec<Emphasis>) -> Self {
        Self { text, emphasis }
    }
}

impl From<String> for Rich {
    fn from(text: String) -> Self {
        Self {
            text,
            emphasis: Vec::new(),
        }
    }
}

impl From<&str> for Rich {
    fn from(text: &str) -> Self {
        Self::from(text.to_string())
    }
}

impl std::ops::Deref for Rich {
    type Target = str;

    fn deref(&self) -> &str {
        &self.text
    }
}

impl std::fmt::Display for Rich {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// Compare with a plain string and ignore the emphasis: callers asking whether a
/// paragraph reads a certain way are asking about the words.
impl PartialEq<str> for Rich {
    fn eq(&self, other: &str) -> bool {
        self.text == other
    }
}

impl PartialEq<&str> for Rich {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

/// A block of book content. Mirrors the handful of shapes that survive the
/// trip from XHTML to a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Para {
    Heading {
        level: u8,
        text: String,
    },
    Text(Rich),
    Quote(Rich),
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
            Para::Text(t) | Para::Quote(t) => &t.text,
            Para::Code(t) => t,
            Para::Image { alt, .. } => alt,
        }
    }

    /// Emphasis inside this paragraph, in its own byte coordinates. Headings are
    /// already emphatic and code means what it says, so neither carries any.
    pub fn emphasis(&self) -> &[Emphasis] {
        match self {
            Para::Text(t) | Para::Quote(t) => &t.emphasis,
            _ => &[],
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
    /// Line this chapter starts on in its source file. Several chapters can
    /// come from one EPUB document — a table of contents may point at three
    /// anchors inside `part1.xhtml` — and then the second one does not start at
    /// line 1.
    pub first_line: usize,
}

impl Chapter {
    /// A chapter that begins at the top of its own file.
    pub fn new(title: String, href: String, paras: Vec<Para>) -> Self {
        Self {
            title,
            href,
            paras,
            first_line: 1,
        }
    }

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
            + self.first_line
    }

    /// Lines this chapter occupies in its source file.
    pub fn line_count(&self) -> usize {
        self.source_line(self.paras.len()) - self.first_line + 1
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
    /// The cover image, already extracted to the host directory. Books that do
    /// not declare one, and formats that cannot have one, leave it empty.
    pub cover: Option<Cover>,
}

/// A book's cover: where the picture is now, and where it came from.
#[derive(Debug, Clone)]
pub struct Cover {
    pub file: PathBuf,
    /// Path inside the archive, so the tool line that shows it can be truthful.
    pub href: String,
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
            "html" | "htm" => html::load(&path)?,
            "txt" => text::load(&path, assets)?,
            "md" | "markdown" => text::load_markdown(&path, assets)?,
            other => {
                anyhow::bail!(
                    "unsupported format `.{other}` (expected .epub, .pdf, .html, .txt or .md)"
                )
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
        matches!(
            ext.as_str(),
            "epub" | "pdf" | "html" | "htm" | "txt" | "md" | "markdown"
        )
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

    /// Create the shared durable locator for a terminal chapter/paragraph.
    ///
    /// The terminal view is only a projection. Rebuilding its semantic blocks
    /// through the same Rust Document IR used by Web and Apple gives progress a
    /// coordinate independent of byte offsets, wrapping, and chapter cuts.
    pub fn create_text_locator(&self, chapter: usize, para: usize) -> Option<TextLocator> {
        let projected = self.locator_document()?;
        let offset = projected.offset_before(chapter, para);
        Some(ReadioSdk::new().documents.create_text_locator(
            &projected.document,
            offset,
            Affinity::Forward,
        ))
    }

    /// Resolve a shared locator back into the terminal's current chapter view.
    pub fn restore_text_locator(&self, locator: &TextLocator) -> Option<(usize, usize)> {
        let projected = self.locator_document()?;
        let resolved = ReadioSdk::new()
            .documents
            .restore_text_locator(locator, &projected.document)
            .ok()?;
        projected.position_at(
            resolved.projection.char_offset,
            resolved.projection.block_id.as_deref(),
        )
    }

    fn locator_document(&self) -> Option<LocatorDocument> {
        let mut html = String::new();
        let mut positions = Vec::new();
        for (chapter_index, chapter) in self.chapters.iter().enumerate() {
            for (para_index, para) in chapter.paras.iter().enumerate() {
                let text = para.text();
                if text.is_empty() {
                    continue;
                }
                let escaped = escape_text_for_html(text);
                match para {
                    Para::Heading { level, .. } => {
                        let level = (*level).clamp(1, 6);
                        html.push_str(&format!("<h{level}>{escaped}</h{level}>"));
                    }
                    Para::Quote(_) => html.push_str(&format!("<blockquote>{escaped}</blockquote>")),
                    Para::Code(_) => html.push_str(&format!("<pre>{escaped}</pre>")),
                    Para::Text(_) | Para::Image { .. } => {
                        html.push_str(&format!("<p>{escaped}</p>"));
                    }
                }
                positions.push((chapter_index, para_index));
            }
        }
        if positions.is_empty() {
            return None;
        }
        let document = ReadioSdk::new()
            .documents
            .parse_html(
                &html,
                &HtmlParseOptions {
                    max_input_bytes: html.len().saturating_add(1),
                    max_blocks: positions.len(),
                    base_url: None,
                },
            )
            .ok()?;
        if document.blocks.len() != positions.len() {
            return None;
        }
        Some(LocatorDocument {
            document,
            positions,
        })
    }

    /// The position `chars` characters into the book.
    ///
    /// This is the durable way to name a place. Chapter and paragraph indices
    /// are not: improve how a book is cut into chapters and every stored
    /// `(chapter, para)` means somewhere else, while a character offset still
    /// means the same sentence.
    pub fn locate(&self, chars: usize) -> (usize, usize) {
        let mut seen = 0usize;
        for (ci, chapter) in self.chapters.iter().enumerate() {
            for (pi, para) in chapter.paras.iter().enumerate() {
                if seen >= chars {
                    return (ci, pi);
                }
                seen += para.char_count();
            }
        }
        let last = self.chapters.len().saturating_sub(1);
        (last, self.chapters.get(last).map_or(0, |c| c.paras.len()))
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
        let Some(projected) = self.locator_document() else {
            return Hits::default();
        };
        let sdk = ReadioSdk::new();
        let revision = projected.document.revision.revision_digest.clone();
        let mut cursor = None;
        let mut hits = Hits::default();

        loop {
            // Core's public page size is deliberately bounded. Keep the TUI's
            // historical `usize` limit by consuming another page only when a
            // caller genuinely asks for more than one Core page.
            let remaining = limit.saturating_sub(hits.shown.len());
            let page_limit = remaining.min(u16::MAX as usize) as u16;
            let Ok(page) = sdk.documents.search(
                &projected.document,
                SearchRequest {
                    document_revision: revision.clone(),
                    query: needle.to_string(),
                    cursor: cursor.clone(),
                    limit: page_limit,
                },
            ) else {
                return Hits::default();
            };

            hits.total = usize::try_from(page.total_occurrences).unwrap_or(usize::MAX);
            hits.paragraphs = usize::try_from(page.total_blocks).unwrap_or(usize::MAX);
            for core_hit in page.hits {
                let Some((chapter, para, block_start)) =
                    projected.block_position(&core_hit.block_id)
                else {
                    continue;
                };
                let Some(text) = self
                    .chapters
                    .get(chapter)
                    .and_then(|chapter| chapter.paras.get(para))
                    .map(Para::text)
                else {
                    continue;
                };
                let Some(local_start) = core_hit.char_range[0].checked_sub(block_start) else {
                    continue;
                };
                let Some(local_end) = core_hit.char_range[1].checked_sub(block_start) else {
                    continue;
                };
                let Some(range) = scalar_range_to_byte_range(text, local_start, local_end) else {
                    continue;
                };
                hits.shown.push(Hit {
                    chapter,
                    para,
                    range,
                    excerpt: core_hit.snippet,
                });
            }

            cursor = page.next_cursor;
            if hits.shown.len() >= limit || cursor.is_none() || page_limit == 0 {
                break;
            }
        }

        hits
    }
}

struct LocatorDocument {
    document: CoreDocument,
    positions: Vec<(usize, usize)>,
}

impl LocatorDocument {
    fn offset_before(&self, chapter: usize, para: usize) -> u64 {
        let target = (chapter, para);
        let mut offset = 0_u64;
        for (index, (block, position)) in
            self.document.blocks.iter().zip(&self.positions).enumerate()
        {
            if *position >= target {
                return offset;
            }
            offset = offset
                .saturating_add(block.text.chars().count() as u64)
                .saturating_add(u64::from(index + 1 < self.document.blocks.len()));
        }
        self.document.revision.char_count
    }

    fn position_at(&self, offset: u64, block_id: Option<&str>) -> Option<(usize, usize)> {
        if offset >= self.document.revision.char_count {
            let chapter = self.positions.last()?.0;
            return Some((chapter, self.positions.last()?.1.saturating_add(1)));
        }
        if let Some(block_id) = block_id
            && let Some(index) = self
                .document
                .blocks
                .iter()
                .position(|block| block.id == block_id)
        {
            return self.positions.get(index).copied();
        }

        let mut block_start = 0_u64;
        for (index, block) in self.document.blocks.iter().enumerate() {
            let block_end = block_start.saturating_add(block.text.chars().count() as u64);
            if offset <= block_end {
                return self.positions.get(index).copied();
            }
            block_start = block_end.saturating_add(1);
        }
        self.positions.last().copied()
    }

    /// Terminal coordinates and the shared document offset for a Core block.
    /// Search ranges are global Unicode-scalar offsets; rendering needs a byte
    /// range inside one terminal paragraph, so both coordinate systems meet at
    /// this boundary and nowhere else.
    fn block_position(&self, block_id: &str) -> Option<(usize, usize, u64)> {
        let mut block_start = 0_u64;
        for (index, block) in self.document.blocks.iter().enumerate() {
            if block.id == block_id {
                let (chapter, para) = *self.positions.get(index)?;
                return Some((chapter, para, block_start));
            }
            block_start = block_start
                .saturating_add(block.text.chars().count() as u64)
                .saturating_add(u64::from(index + 1 < self.document.blocks.len()));
        }
        None
    }
}

fn escape_text_for_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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

fn scalar_range_to_byte_range(text: &str, start: u64, end: u64) -> Option<(usize, usize)> {
    let start = usize::try_from(start).ok()?;
    let end = usize::try_from(end).ok()?;
    if end < start {
        return None;
    }
    let mut offsets: Vec<usize> = text.char_indices().map(|(offset, _)| offset).collect();
    offsets.push(text.len());
    Some((*offsets.get(start)?, *offsets.get(end)?))
}

/// Stable per-book key for the progress store.
///
/// Derived from the file's contents, not its path, so importing a book (which
/// copies or moves it into the host directory) keeps the reading progress that
/// was recorded before the import.
pub fn content_id(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("cannot read {} for content identity", path.display()))?;
    Ok(readio_sdk::ReadioSdk::new()
        .imports
        .content_identity(&bytes)
        .digest)
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
            chapters: vec![Chapter::new(
                "one".into(),
                "one.html".into(),
                paras
                    .iter()
                    .map(|text| Para::Text((*text).into()))
                    .collect(),
            )],
            cover: None,
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
