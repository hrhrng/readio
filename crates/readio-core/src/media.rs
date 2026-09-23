//! Portable byte-oriented document format parsing for the Rust Core.
//!
//! This module owns EPUB/PDF container semantics and produces shared Document
//! IR. It knows nothing about SDK facades, UI, filesystems, accounts,
//! bookmarks, or platform rendering.

use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Read},
};

use crate::documents::{
    BlockKind, Document, HtmlParseOptions, ResolvedMediaAsset, document_from_semantic_blocks,
    parse_html_with_media_resolver,
};
use crate::reader::{ReaderChapterSource, ReaderIr, ReaderNavigationSource, compose_reader_ir};
use pdf_extract::{Dictionary, Document as PdfDocument, Object, ObjectId};
use quick_xml::{Reader, events::Event};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_PDF_BYTES: usize = 100 * 1024 * 1024;
const SCAN_CHARS_PER_PAGE: usize = 25;
const PDF_PARSER_VERSION: &str = "pdf-ir-1";
const HEADING_MAX_CHARS: usize = 32;
const PDF_GROUP_CHARS: usize = 3_000;
const MAX_EPUB_BYTES: usize = 200 * 1024 * 1024;
const MAX_EPUB_ENTRIES: usize = 10_000;
const MAX_EPUB_XML_BYTES: usize = 1024 * 1024;
const MAX_EPUB_CHAPTER_BYTES: usize = 10 * 1024 * 1024;
const MAX_EPUB_BLOCKS_PER_CHAPTER: usize = 100_000;
const MAX_EPUB_ASSET_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedPdf {
    pub title: Option<String>,
    pub author: Option<String>,
    pub canonical_text: String,
    pub page_count: usize,
    pub chapters: Vec<ParsedPdfChapter>,
    pub reader: ReaderIr,
}

/// A PDF chapter expressed as the same Document IR used by HTML and EPUB.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedPdfChapter {
    /// Outline or detected heading title. `None` lets each UI localize a stable
    /// fallback such as “Part 1 (p1-3)” without putting presentation copy here.
    pub title: Option<String>,
    pub page_start: usize,
    pub page_end: usize,
    pub document: Document,
    pub block_provenance: Vec<ParsedPdfBlockProvenance>,
}

/// Page coordinates for one semantic Document block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedPdfBlockProvenance {
    pub block_id: String,
    pub page_start: usize,
    pub page_end: usize,
}

/// Platform-neutral EPUB publication whose chapters use the shared Document IR.
/// Asset metadata stays small; original bytes remain in the persisted EPUB and
/// are loaded on demand by stable ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedEpub {
    pub title: Option<String>,
    pub author: Option<String>,
    pub cover_asset_id: Option<String>,
    pub assets: Vec<ParsedEpubAsset>,
    pub toc: Vec<ParsedEpubTocEntry>,
    pub chapters: Vec<ParsedEpubChapter>,
    pub reader: ReaderIr,
}

impl ParsedEpub {
    pub fn canonical_text(&self) -> String {
        self.reader.document.canonical_text()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedEpubAsset {
    pub id: String,
    pub href: String,
    pub media_type: String,
    pub byte_length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedEpubChapter {
    pub id: String,
    pub href: String,
    pub title: String,
    pub document: Document,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedEpubTocEntry {
    pub id: String,
    pub title: String,
    pub depth: usize,
    pub href: String,
    pub chapter_id: String,
    pub block_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaError {
    InputTooLarge { actual: usize, limit: usize },
    InvalidPdf(String),
    EmptyPdf,
    ScannedPdf { characters_per_page: usize },
    InvalidEpub(String),
    EmptyEpub,
    EpubAssetNotFound { asset_id: String },
    EpubAssetTooLarge { actual: u64, limit: usize },
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InputTooLarge { actual, limit } => {
                write!(formatter, "PDF input uses {actual} bytes; limit is {limit}")
            }
            Self::InvalidPdf(message) => write!(formatter, "invalid PDF: {message}"),
            Self::EmptyPdf => formatter.write_str("PDF contains no pages"),
            Self::ScannedPdf {
                characters_per_page,
            } => write!(
                formatter,
                "PDF appears to be scanned ({characters_per_page} text characters per page)"
            ),
            Self::InvalidEpub(message) => write!(formatter, "invalid EPUB: {message}"),
            Self::EmptyEpub => formatter.write_str("EPUB contains no readable chapters"),
            Self::EpubAssetNotFound { asset_id } => {
                write!(
                    formatter,
                    "EPUB asset {asset_id} is not declared by the package"
                )
            }
            Self::EpubAssetTooLarge { actual, limit } => {
                write!(
                    formatter,
                    "EPUB asset uses {actual} bytes; limit is {limit}"
                )
            }
        }
    }
}

pub fn extract_epub_asset(bytes: &[u8], asset_id: &str) -> Result<Vec<u8>, MediaError> {
    if bytes.len() > MAX_EPUB_BYTES {
        return Err(MediaError::InvalidEpub(format!(
            "input uses {} bytes; limit is {MAX_EPUB_BYTES}",
            bytes.len()
        )));
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| MediaError::InvalidEpub(error.to_string()))?;
    if archive.len() > MAX_EPUB_ENTRIES {
        return Err(MediaError::InvalidEpub(format!(
            "archive contains {} entries; limit is {MAX_EPUB_ENTRIES}",
            archive.len()
        )));
    }
    let entries = index_entries(&mut archive)?;
    let container = read_text_entry(
        &mut archive,
        &entries,
        "META-INF/container.xml",
        MAX_EPUB_XML_BYTES,
    )?
    .ok_or_else(|| MediaError::InvalidEpub("missing META-INF/container.xml".to_string()))?;
    let opf_path = find_attribute(&container, "rootfile", "full-path")?
        .ok_or_else(|| MediaError::InvalidEpub("container has no rootfile".to_string()))?;
    let opf_path = normalize_archive_path(&opf_path)
        .ok_or_else(|| MediaError::InvalidEpub("package path escapes the archive".to_string()))?;
    let opf = read_text_entry(&mut archive, &entries, &opf_path, MAX_EPUB_XML_BYTES)?
        .ok_or_else(|| MediaError::InvalidEpub(format!("package file {opf_path} is missing")))?;
    let package = parse_package(&opf)?;
    let package_base = parent_directory(&opf_path);
    let assets = index_epub_assets(&mut archive, &entries, &package, &package_base)?;
    let asset = assets
        .iter()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| MediaError::EpubAssetNotFound {
            asset_id: asset_id.to_string(),
        })?;
    if asset.byte_length > MAX_EPUB_ASSET_BYTES as u64 {
        return Err(MediaError::EpubAssetTooLarge {
            actual: asset.byte_length,
            limit: MAX_EPUB_ASSET_BYTES,
        });
    }
    read_binary_entry(&mut archive, &entries, &asset.href, MAX_EPUB_ASSET_BYTES)?.ok_or_else(|| {
        MediaError::EpubAssetNotFound {
            asset_id: asset_id.to_string(),
        }
    })
}

pub fn parse_epub(bytes: &[u8]) -> Result<ParsedEpub, MediaError> {
    if bytes.len() > MAX_EPUB_BYTES {
        return Err(MediaError::InvalidEpub(format!(
            "input uses {} bytes; limit is {MAX_EPUB_BYTES}",
            bytes.len()
        )));
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| MediaError::InvalidEpub(error.to_string()))?;
    if archive.len() > MAX_EPUB_ENTRIES {
        return Err(MediaError::InvalidEpub(format!(
            "archive contains {} entries; limit is {MAX_EPUB_ENTRIES}",
            archive.len()
        )));
    }
    let entries = index_entries(&mut archive)?;
    let container = read_text_entry(
        &mut archive,
        &entries,
        "META-INF/container.xml",
        MAX_EPUB_XML_BYTES,
    )?
    .ok_or_else(|| MediaError::InvalidEpub("missing META-INF/container.xml".to_string()))?;
    let opf_path = find_attribute(&container, "rootfile", "full-path")?
        .ok_or_else(|| MediaError::InvalidEpub("container has no rootfile".to_string()))?;
    let opf_path = normalize_archive_path(&opf_path)
        .ok_or_else(|| MediaError::InvalidEpub("package path escapes the archive".to_string()))?;
    let opf = read_text_entry(&mut archive, &entries, &opf_path, MAX_EPUB_XML_BYTES)?
        .ok_or_else(|| MediaError::InvalidEpub(format!("package file {opf_path} is missing")))?;
    let package = parse_package(&opf)?;
    let package_base = parent_directory(&opf_path);
    let assets = index_epub_assets(&mut archive, &entries, &package, &package_base)?;
    let assets_by_href = assets
        .iter()
        .map(|asset| (asset.href.as_str(), asset))
        .collect::<HashMap<_, _>>();
    let cover_asset_id = resolve_cover_asset_id(&package, &package_base, &assets_by_href);
    let mut chapters = Vec::new();

    for idref in &package.spine {
        let Some(item) = package.manifest.get(idref) else {
            continue;
        };
        if !item.media_type.to_ascii_lowercase().contains("html") {
            continue;
        }
        let Some(href) = join_archive_path(&package_base, &item.href) else {
            continue;
        };
        let Some(html) = read_text_entry(&mut archive, &entries, &href, MAX_EPUB_CHAPTER_BYTES)?
        else {
            continue;
        };
        let chapter_base = parent_directory(&href);
        let resolve_asset = |source: &str| {
            let asset_href = join_archive_path(&chapter_base, source)?;
            let asset = assets_by_href.get(asset_href.as_str())?;
            Some(ResolvedMediaAsset {
                id: asset.id.clone(),
                source: asset.href.clone(),
            })
        };
        let document = parse_html_with_media_resolver(
            &html,
            &HtmlParseOptions {
                max_input_bytes: MAX_EPUB_CHAPTER_BYTES,
                max_blocks: MAX_EPUB_BLOCKS_PER_CHAPTER,
                base_url: None,
            },
            &resolve_asset,
        )
        .map_err(|error| MediaError::InvalidEpub(format!("chapter {href}: {error:?}")))?;
        if document.blocks.is_empty() {
            continue;
        }
        let title = document
            .blocks
            .iter()
            .find(|block| matches!(block.kind, BlockKind::Heading { .. }))
            .map(|block| block.text.trim().to_string())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| format!("Section {}", chapters.len() + 1));
        chapters.push(ParsedEpubChapter {
            id: idref.clone(),
            href,
            title,
            document,
        });
    }

    if chapters.is_empty() {
        return Err(MediaError::EmptyEpub);
    }
    let toc = parse_epub_toc(&mut archive, &entries, &package, &package_base, &chapters)?;
    let reader = compose_reader_ir(
        bytes,
        "epub-reader-ir-1",
        chapters
            .iter()
            .map(|chapter| ReaderChapterSource {
                id: chapter.id.clone(),
                title: chapter.title.clone(),
                document: chapter.document.clone(),
            })
            .collect(),
        toc.iter()
            .map(|entry| ReaderNavigationSource {
                id: entry.id.clone(),
                title: entry.title.clone(),
                depth: entry.depth,
                chapter_id: entry.chapter_id.clone(),
                block_id: entry.block_id.clone(),
            })
            .collect(),
    );
    Ok(ParsedEpub {
        title: package.title,
        author: package.author,
        cover_asset_id,
        assets,
        toc,
        chapters,
        reader,
    })
}

impl std::error::Error for MediaError {}

pub fn parse_pdf(bytes: &[u8]) -> Result<ParsedPdf, MediaError> {
    if bytes.len() > MAX_PDF_BYTES {
        return Err(MediaError::InputTooLarge {
            actual: bytes.len(),
            limit: MAX_PDF_BYTES,
        });
    }
    let loaded = std::panic::catch_unwind(|| PdfDocument::load_mem(bytes))
        .map_err(|_| MediaError::InvalidPdf("parser aborted on malformed input".to_string()))?
        .map_err(|error| MediaError::InvalidPdf(error.to_string()))?;
    let mut pdf = loaded;
    if pdf.is_encrypted() {
        pdf.decrypt("")
            .map_err(|error| MediaError::InvalidPdf(error.to_string()))?;
    }
    let pages = pdf
        .get_pages()
        .keys()
        .copied()
        .map(|number| extract_pdf_page(&pdf, number))
        .collect::<Vec<_>>();
    if pages.is_empty() {
        return Err(MediaError::EmptyPdf);
    }

    let visible_characters = pages
        .iter()
        .flat_map(|page| page.chars())
        .filter(|character| !character.is_whitespace())
        .count();
    let characters_per_page = visible_characters / pages.len();
    if characters_per_page < SCAN_CHARS_PER_PAGE {
        return Err(MediaError::ScannedPdf {
            characters_per_page,
        });
    }

    let page_lines = strip_pdf_boilerplate(&split_pdf_lines(&pages));
    let blocks = reconstruct_pdf_blocks(&page_lines);
    if blocks.is_empty() {
        return Err(MediaError::ScannedPdf {
            characters_per_page,
        });
    }
    let groups = pdf_outline_groups(&pdf, &blocks)
        .or_else(|| pdf_heading_groups(&blocks))
        .unwrap_or_else(|| pdf_page_groups(&blocks));
    let chapters = assemble_pdf_chapters(bytes, &groups);
    let reader_chapters = chapters
        .iter()
        .enumerate()
        .map(|(index, chapter)| ReaderChapterSource {
            id: pdf_reader_chapter_id(index, chapter.page_start, chapter.page_end),
            title: chapter.title.clone().unwrap_or_default(),
            document: chapter.document.clone(),
        })
        .collect::<Vec<_>>();
    let reader_navigation = reader_chapters
        .iter()
        .map(|chapter| ReaderNavigationSource {
            id: chapter.id.clone(),
            title: chapter.title.clone(),
            depth: 0,
            chapter_id: chapter.id.clone(),
            block_id: None,
        })
        .collect();
    let reader = compose_reader_ir(bytes, "pdf-reader-ir-1", reader_chapters, reader_navigation);
    let canonical_text = reader.document.canonical_text();
    if canonical_text.is_empty() {
        return Err(MediaError::ScannedPdf {
            characters_per_page,
        });
    }
    Ok(ParsedPdf {
        title: pdf_info_text(&pdf, b"Title"),
        author: pdf_info_text(&pdf, b"Author"),
        canonical_text,
        page_count: pages.len(),
        chapters,
        reader,
    })
}

fn pdf_reader_chapter_id(index: usize, page_start: usize, page_end: usize) -> String {
    let span = if page_start == page_end {
        format!("p{page_start}")
    } else {
        format!("p{page_start}-{page_end}")
    };
    format!("pdf-chapter-{}-{span}", index + 1)
}

fn extract_pdf_page(pdf: &PdfDocument, number: u32) -> String {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut text = String::new();
        {
            let mut sink = pdf_extract::PlainTextOutput::new(&mut text);
            if pdf_extract::output_doc_page(pdf, &mut sink, number).is_err() {
                return String::new();
            }
        }
        text
    }))
    .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PdfBlock {
    page: usize,
    through: usize,
    text: String,
    heading: bool,
}

type PdfGroup<'a> = (Option<String>, Vec<&'a PdfBlock>);

fn split_pdf_lines(pages: &[String]) -> Vec<Vec<String>> {
    pages
        .iter()
        .map(|page| {
            page.lines()
                .map(collapse_pdf_whitespace)
                .filter(|line| !line.is_empty())
                .collect()
        })
        .collect()
}

fn strip_pdf_boilerplate(pages: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut seen = HashMap::<String, usize>::new();
    for lines in pages {
        let mut once = HashSet::new();
        for index in pdf_edge_indices(lines.len()) {
            let key = pdf_line_fingerprint(&lines[index]);
            if once.insert(key.clone()) {
                *seen.entry(key).or_default() += 1;
            }
        }
    }
    let repeated = |line: &str| {
        pages.len() >= 3
            && seen.get(&pdf_line_fingerprint(line)).copied().unwrap_or(0) * 10 >= pages.len() * 6
    };
    pages
        .iter()
        .map(|lines| {
            let edges = pdf_edge_indices(lines.len());
            lines
                .iter()
                .enumerate()
                .filter(|(index, line)| {
                    if edges.contains(index) {
                        return !is_pdf_page_number(line) && !repeated(line);
                    }
                    !line.chars().all(|character| character.is_ascii_digit())
                })
                .map(|(_, line)| line.clone())
                .collect()
        })
        .collect()
}

fn pdf_edge_indices(length: usize) -> Vec<usize> {
    (0..length)
        .filter(|index| *index < 2 || index + 2 >= length)
        .collect()
}

fn pdf_line_fingerprint(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut in_number = false;
    for character in line.chars().filter(|character| !character.is_whitespace()) {
        if character.is_ascii_digit() {
            if !in_number {
                output.push('#');
            }
            in_number = true;
        } else {
            in_number = false;
            output.extend(character.to_lowercase());
        }
    }
    output
}

fn is_pdf_page_number(line: &str) -> bool {
    let bare = line
        .trim()
        .trim_matches(|character: char| {
            character.is_whitespace() || "-–—·.[]()<>|/*".contains(character)
        })
        .trim();
    if bare.is_empty() {
        return false;
    }
    if bare.chars().all(|character| character.is_ascii_digit()) {
        return true;
    }
    if bare.len() <= 6
        && bare
            .chars()
            .all(|character| "ivxlcdmIVXLCDM".contains(character))
    {
        return true;
    }
    let digits_only =
        |text: &str| !text.is_empty() && text.chars().all(|character| character.is_ascii_digit());
    if let Some(rest) = bare.strip_prefix('第')
        && let Some(number) = rest.strip_suffix('页')
    {
        return digits_only(number.trim());
    }
    let lower = bare.to_lowercase();
    let body = lower.strip_prefix("page").unwrap_or(&lower).trim();
    let words = body
        .split(|character: char| character.is_whitespace() || character == '/')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    match words.as_slice() {
        [number] => digits_only(number),
        [number, "of", total] => digits_only(number) && digits_only(total),
        [number, total] => {
            digits_only(number)
                && digits_only(total)
                && (lower.starts_with("page") || bare.contains('/'))
        }
        _ => false,
    }
}

fn reconstruct_pdf_blocks(pages: &[Vec<String>]) -> Vec<PdfBlock> {
    let mut blocks: Vec<PdfBlock> = Vec::new();
    let mut carry = false;
    for (index, lines) in pages.iter().enumerate() {
        let page = index + 1;
        for (order, text) in pdf_paragraphs(lines).into_iter().enumerate() {
            let heading = is_pdf_heading(&text);
            let continues = carry
                && order == 0
                && !heading
                && !starts_pdf_block(&text)
                && blocks.last().is_some_and(|previous| !previous.heading);
            match blocks.last_mut() {
                Some(previous) if continues => {
                    previous.text = join_pdf_lines(&previous.text, &text);
                    previous.through = page;
                }
                _ => blocks.push(PdfBlock {
                    page,
                    through: page,
                    text,
                    heading,
                }),
            }
        }
        carry = pdf_page_ends_mid_paragraph(lines);
    }
    blocks
}

fn pdf_paragraphs(lines: &[String]) -> Vec<String> {
    let typical = typical_pdf_line_length(lines);
    let mut paragraphs = Vec::new();
    let mut current = String::new();
    for line in lines {
        if !current.is_empty() && starts_pdf_block(line) {
            paragraphs.push(std::mem::take(&mut current));
        }
        if current.is_empty() {
            current.push_str(line);
        } else {
            current = join_pdf_lines(&current, line);
        }
        if ends_pdf_sentence(line) || is_short_pdf_line(line, typical) || is_pdf_heading(line) {
            paragraphs.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        paragraphs.push(current);
    }
    paragraphs.retain(|paragraph| !paragraph.trim().is_empty());
    paragraphs
}

fn pdf_page_ends_mid_paragraph(lines: &[String]) -> bool {
    let typical = typical_pdf_line_length(lines);
    lines.last().is_some_and(|last| {
        !ends_pdf_sentence(last) && !is_short_pdf_line(last, typical) && !is_pdf_heading(last)
    })
}

fn typical_pdf_line_length(lines: &[String]) -> usize {
    if lines.len() < 3 {
        return 0;
    }
    let mut lengths = lines
        .iter()
        .map(|line| line.chars().count())
        .collect::<Vec<_>>();
    lengths.sort_unstable();
    lengths[lengths.len() / 2]
}

fn is_short_pdf_line(line: &str, typical: usize) -> bool {
    typical >= 24 && line.chars().count() * 4 < typical * 3
}

fn join_pdf_lines(previous: &str, next: &str) -> String {
    let head = previous.trim_end();
    let tail = next.trim_start();
    if tail.is_empty() {
        return head.to_string();
    }
    let last = head.chars().next_back();
    let before_last = head.chars().rev().nth(1);
    let first = tail.chars().next();
    if last == Some('-') && before_last.is_some_and(|character| character.is_ascii_alphabetic()) {
        let stem = match first {
            Some(character) if character.is_ascii_lowercase() => &head[..head.len() - 1],
            _ => head,
        };
        return format!("{stem}{tail}");
    }
    let mut joined = head.to_string();
    let flush = last.is_some_and(is_wide_pdf_character) && first.is_some_and(is_wide_pdf_character);
    if !flush {
        joined.push(' ');
    }
    joined.push_str(tail);
    joined
}

fn is_wide_pdf_character(character: char) -> bool {
    matches!(character as u32, 0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFF60)
}

fn ends_pdf_sentence(line: &str) -> bool {
    line.trim_end()
        .trim_end_matches(['”', '’', '"', '\'', '）', ')', '』', '」', '》', '〉', ']'])
        .ends_with(['。', '！', '？', '…', '.', '!', '?'])
}

fn starts_pdf_block(line: &str) -> bool {
    let characters = line.trim_start().chars().collect::<Vec<_>>();
    let Some(first) = characters.first().copied() else {
        return false;
    };
    if "•·●○▪◦‣※*".contains(first) {
        return true;
    }
    if "-–—".contains(first)
        && characters
            .get(1)
            .is_some_and(|character| character.is_whitespace())
    {
        return true;
    }
    if first == '(' || first == '（' {
        let inner = characters[1..]
            .iter()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>();
        if !inner.is_empty()
            && characters
                .get(1 + inner.chars().count())
                .is_some_and(|character| *character == ')' || *character == '）')
        {
            return true;
        }
    }
    let label = characters
        .iter()
        .take_while(|character| !character.is_whitespace())
        .collect::<String>();
    let marker = |set: &str, tail: &str| {
        let digits = label
            .chars()
            .take_while(|character| set.contains(*character))
            .collect::<String>();
        !digits.is_empty()
            && label
                .chars()
                .nth(digits.chars().count())
                .is_some_and(|character| tail.contains(character))
            && label.chars().count() <= digits.chars().count() + 1
    };
    marker("0123456789", ".)、．")
        || marker("零〇一二三四五六七八九十", "、.．")
        || is_pdf_heading(line)
}

fn is_pdf_heading(line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() || line.chars().count() > HEADING_MAX_CHARS {
        return false;
    }
    if line.ends_with([
        '。', '，', '、', '；', '：', '！', '？', '…', ',', '.', ';', ':', '!', '?',
    ]) {
        return false;
    }
    is_pdf_chapter_label(line)
}

fn is_pdf_chapter_label(line: &str) -> bool {
    let lower = line.to_lowercase();
    if ["chapter ", "part ", "section ", "appendix ", "book "]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }
    if [
        "contents",
        "preface",
        "foreword",
        "introduction",
        "epilogue",
        "index",
        "bibliography",
        "前言",
        "序",
        "序言",
        "目录",
        "引言",
        "后记",
        "结语",
        "附录",
        "致谢",
        "参考文献",
    ]
    .contains(&lower.as_str())
    {
        return true;
    }
    let (number, rest) = lower
        .split_once(char::is_whitespace)
        .unwrap_or((&lower, ""));
    let parts = number.trim_end_matches('.').split('.').collect::<Vec<_>>();
    if !rest.trim().is_empty()
        && parts.len() >= 2
        && parts.iter().all(|part| {
            part.len() <= 3 && !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return true;
    }
    is_chinese_pdf_chapter_label(line)
}

fn is_chinese_pdf_chapter_label(line: &str) -> bool {
    let characters = line.chars().collect::<Vec<_>>();
    if characters.first() != Some(&'第') {
        return false;
    }
    let Some(unit) = characters
        .iter()
        .position(|character| "章节卷回篇部".contains(*character))
    else {
        return false;
    };
    let numeral = "0123456789零〇一二三四五六七八九十百千两 ";
    let number_ok = unit > 1 && characters[1..unit].iter().all(|c| numeral.contains(*c));
    let tail_ok = characters
        .get(unit + 1)
        .is_none_or(|character| character.is_whitespace() || "：:·-—、.".contains(*character));
    number_ok && tail_ok
}

fn pdf_heading_groups(blocks: &[PdfBlock]) -> Option<Vec<PdfGroup<'_>>> {
    let mut groups = Vec::<PdfGroup<'_>>::new();
    for block in blocks {
        let previous_has_prose = groups
            .last()
            .is_none_or(|(_, blocks)| blocks.iter().any(|block| !block.heading));
        if block.heading && previous_has_prose {
            groups.push((Some(block.text.clone()), Vec::new()));
        }
        match groups.last_mut() {
            Some((_, current)) => current.push(block),
            None => groups.push((None, vec![block])),
        }
    }
    let usable = groups.len() >= 2
        && groups
            .iter()
            .all(|(_, blocks)| blocks.iter().any(|block| !block.heading));
    usable.then_some(groups)
}

fn pdf_outline_groups<'a>(pdf: &PdfDocument, blocks: &'a [PdfBlock]) -> Option<Vec<PdfGroup<'a>>> {
    let marks = pdf_outline(pdf);
    if marks.len() < 2 {
        return None;
    }
    let mut groups = Vec::new();
    for (index, (page, title)) in marks.iter().enumerate() {
        let end = marks
            .get(index + 1)
            .map(|(page, _)| *page)
            .unwrap_or(usize::MAX);
        let start = if index == 0 { 0 } else { *page };
        let slice = blocks
            .iter()
            .filter(|block| block.page >= start && block.page < end)
            .collect::<Vec<_>>();
        if slice.is_empty() {
            return None;
        }
        groups.push((Some(title.clone()), slice));
    }
    Some(groups)
}

fn pdf_outline(pdf: &PdfDocument) -> Vec<(usize, String)> {
    let page_numbers = pdf
        .get_pages()
        .into_iter()
        .map(|(number, id)| (id, number as usize))
        .collect::<HashMap<ObjectId, usize>>();
    let Ok(catalog) = pdf.catalog() else {
        return Vec::new();
    };
    let Some(root) = catalog
        .get(b"Outlines")
        .ok()
        .and_then(|object| pdf_dictionary(pdf, object))
    else {
        return Vec::new();
    };
    let mut marks = Vec::new();
    let mut next = root
        .get(b"First")
        .ok()
        .and_then(|object| object.as_reference().ok());
    let mut visited = HashSet::new();
    while let Some(id) = next {
        if !visited.insert(id) || visited.len() > 512 {
            break;
        }
        let Ok(item) = pdf.get_dictionary(id) else {
            break;
        };
        if let (Some(title), Some(page)) = (
            item.get(b"Title")
                .ok()
                .and_then(|object| pdf_text(pdf, object))
                .filter(|title| title.chars().count() <= 120),
            pdf_destination_page(pdf, item, &page_numbers),
        ) {
            marks.push((page, title));
        }
        next = item
            .get(b"Next")
            .ok()
            .and_then(|object| object.as_reference().ok());
    }
    marks.sort_by_key(|(page, _)| *page);
    marks.dedup_by_key(|(page, _)| *page);
    marks
}

fn pdf_destination_page(
    pdf: &PdfDocument,
    item: &Dictionary,
    page_numbers: &HashMap<ObjectId, usize>,
) -> Option<usize> {
    let direct = item.get(b"Dest").ok();
    let via_action = item
        .get(b"A")
        .ok()
        .and_then(|action| pdf_dictionary(pdf, action))
        .and_then(|action| action.get(b"D").ok());
    let destination = direct.or(via_action)?;
    let destination = pdf
        .dereference(destination)
        .map(|(_, object)| object)
        .unwrap_or(destination);
    let target = destination.as_array().ok()?.first()?;
    if let Ok(id) = target.as_reference() {
        return page_numbers.get(&id).copied();
    }
    target
        .as_i64()
        .ok()
        .and_then(|index| usize::try_from(index).ok())
        .map(|index| index + 1)
}

fn pdf_page_groups(blocks: &[PdfBlock]) -> Vec<PdfGroup<'_>> {
    let mut groups = Vec::<PdfGroup<'_>>::new();
    let mut current = Vec::new();
    let mut characters = 0usize;
    for (index, block) in blocks.iter().enumerate() {
        current.push(block);
        characters += block.text.chars().count();
        let page_turns = blocks
            .get(index + 1)
            .is_some_and(|next| next.page > block.page);
        if characters >= PDF_GROUP_CHARS && page_turns {
            groups.push((None, std::mem::take(&mut current)));
            characters = 0;
        }
    }
    if !current.is_empty() {
        match groups.last_mut() {
            Some((_, previous)) if characters < PDF_GROUP_CHARS / 3 => {
                previous.append(&mut current)
            }
            _ => groups.push((None, current)),
        }
    }
    groups
}

fn assemble_pdf_chapters(bytes: &[u8], groups: &[PdfGroup<'_>]) -> Vec<ParsedPdfChapter> {
    groups
        .iter()
        .filter(|(_, blocks)| !blocks.is_empty())
        .map(|(title, blocks)| {
            let page_start = blocks.first().map(|block| block.page).unwrap_or(1);
            let page_end = blocks
                .iter()
                .map(|block| block.through)
                .max()
                .unwrap_or(page_start);
            let semantic_blocks = blocks
                .iter()
                .map(|block| {
                    let kind = if block.heading {
                        BlockKind::Heading { level: 2 }
                    } else {
                        BlockKind::Paragraph
                    };
                    (kind, block.text.clone())
                })
                .collect();
            let document =
                document_from_semantic_blocks(bytes, PDF_PARSER_VERSION, semantic_blocks);
            let block_provenance = document
                .blocks
                .iter()
                .zip(blocks.iter())
                .map(|(document_block, source)| ParsedPdfBlockProvenance {
                    block_id: document_block.id.clone(),
                    page_start: source.page,
                    page_end: source.through,
                })
                .collect();
            ParsedPdfChapter {
                title: title.clone(),
                page_start,
                page_end,
                document,
                block_provenance,
            }
        })
        .collect()
}

fn pdf_info_text(pdf: &PdfDocument, key: &[u8]) -> Option<String> {
    let info = pdf
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|object| pdf_dictionary(pdf, object))?;
    info.get(key)
        .ok()
        .and_then(|object| pdf_text(pdf, object))
        .filter(|text| !text.is_empty() && text.chars().count() <= 120)
}

fn pdf_dictionary<'a>(pdf: &'a PdfDocument, object: &'a Object) -> Option<&'a Dictionary> {
    pdf.dereference(object)
        .map(|(_, resolved)| resolved)
        .unwrap_or(object)
        .as_dict()
        .ok()
}

fn pdf_text(pdf: &PdfDocument, object: &Object) -> Option<String> {
    let bytes = pdf
        .dereference(object)
        .map(|(_, resolved)| resolved)
        .unwrap_or(object)
        .as_str()
        .ok()?;
    let decoded = if bytes.starts_with(&[0xFE, 0xFF]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&units)
    } else {
        bytes.iter().map(|byte| *byte as char).collect()
    };
    let cleaned = collapse_pdf_whitespace(&decoded);
    (!cleaned.is_empty()).then_some(cleaned)
}

fn collapse_pdf_whitespace(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            space = !output.is_empty();
        } else {
            if space {
                output.push(' ');
            }
            space = false;
            output.push(character);
        }
    }
    output
}

#[derive(Debug, Default)]
struct EpubPackage {
    title: Option<String>,
    author: Option<String>,
    cover_id: Option<String>,
    manifest: HashMap<String, EpubManifestItem>,
    spine: Vec<String>,
    toc_id: Option<String>,
}

#[derive(Debug)]
struct EpubManifestItem {
    href: String,
    media_type: String,
    properties: Vec<String>,
}

fn parse_package(opf: &str) -> Result<EpubPackage, MediaError> {
    let mut package = EpubPackage::default();
    let mut reader = Reader::from_str(opf);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut metadata_field: Option<&'static str> = None;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                let tag = local_name(element.name().as_ref());
                let attributes = attributes(&element);
                match tag.as_str() {
                    "title" => metadata_field = Some("title"),
                    "creator" => metadata_field = Some("creator"),
                    "meta" => {
                        let is_epub_two_cover = attributes
                            .get("name")
                            .is_some_and(|name| name.eq_ignore_ascii_case("cover"));
                        if is_epub_two_cover && package.cover_id.is_none() {
                            package.cover_id = attributes.get("content").cloned();
                        }
                    }
                    "item" => {
                        let id = attributes.get("id").cloned().unwrap_or_default();
                        let href = attributes.get("href").cloned().unwrap_or_default();
                        let media_type = attributes.get("media-type").cloned().unwrap_or_default();
                        let properties = attributes
                            .get("properties")
                            .map(|value| {
                                value
                                    .split_whitespace()
                                    .map(str::to_ascii_lowercase)
                                    .collect()
                            })
                            .unwrap_or_default();
                        if !id.is_empty() && !href.is_empty() {
                            package.manifest.insert(
                                id,
                                EpubManifestItem {
                                    href,
                                    media_type,
                                    properties,
                                },
                            );
                        }
                    }
                    "spine" => {
                        if package.toc_id.is_none() {
                            package.toc_id = attributes.get("toc").cloned();
                        }
                    }
                    "itemref" => {
                        let is_linear = attributes
                            .get("linear")
                            .is_none_or(|linear| !linear.eq_ignore_ascii_case("no"));
                        if is_linear && let Some(idref) = attributes.get("idref") {
                            package.spine.push(idref.clone());
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(text)) => {
                let value = text
                    .decode()
                    .map(|value| value.trim().to_string())
                    .unwrap_or_default();
                if !value.is_empty() {
                    match metadata_field.take() {
                        Some("title") if package.title.is_none() => package.title = Some(value),
                        Some("creator") if package.author.is_none() => package.author = Some(value),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(_)) => metadata_field = None,
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(MediaError::InvalidEpub(format!(
                    "malformed package XML: {error}"
                )));
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(package)
}

fn index_epub_assets(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    entries: &HashMap<String, usize>,
    package: &EpubPackage,
    package_base: &str,
) -> Result<Vec<ParsedEpubAsset>, MediaError> {
    let mut assets = Vec::new();
    for item in package.manifest.values() {
        let media_type = item.media_type.trim().to_ascii_lowercase();
        if !is_supported_epub_image(&media_type) {
            continue;
        }
        let Some(href) = join_archive_path(package_base, &item.href) else {
            continue;
        };
        let Some(index) = entries.get(&href) else {
            continue;
        };
        let entry = archive
            .by_index(*index)
            .map_err(|error| MediaError::InvalidEpub(error.to_string()))?;
        assets.push(ParsedEpubAsset {
            id: epub_asset_id(&href),
            href,
            media_type,
            byte_length: entry.size(),
        });
    }
    assets.sort_by(|left, right| left.href.cmp(&right.href));
    assets.dedup_by(|left, right| left.href == right.href);
    Ok(assets)
}

fn resolve_cover_asset_id(
    package: &EpubPackage,
    package_base: &str,
    assets_by_href: &HashMap<&str, &ParsedEpubAsset>,
) -> Option<String> {
    let item = package
        .cover_id
        .as_ref()
        .and_then(|cover_id| package.manifest.get(cover_id))
        .or_else(|| {
            package
                .manifest
                .iter()
                .filter(|(_, item)| {
                    item.properties
                        .iter()
                        .any(|property| property == "cover-image")
                })
                .min_by(|left, right| left.0.cmp(right.0))
                .map(|(_, item)| item)
        })?;
    let href = join_archive_path(package_base, &item.href)?;
    assets_by_href
        .get(href.as_str())
        .map(|asset| asset.id.clone())
}

fn is_supported_epub_image(media_type: &str) -> bool {
    matches!(
        media_type,
        "image/avif" | "image/gif" | "image/jpeg" | "image/png" | "image/svg+xml" | "image/webp"
    )
}

fn epub_asset_id(href: &str) -> String {
    let digest = Sha256::digest(href.as_bytes());
    format!("epub-asset-{digest:x}")
}

#[derive(Debug)]
struct RawTocEntry {
    title: String,
    href: String,
    depth: usize,
}

fn parse_epub_toc(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    entries: &HashMap<String, usize>,
    package: &EpubPackage,
    package_base: &str,
    chapters: &[ParsedEpubChapter],
) -> Result<Vec<ParsedEpubTocEntry>, MediaError> {
    let mut nav_candidates = package
        .manifest
        .iter()
        .filter(|(_, item)| item.properties.iter().any(|property| property == "nav"))
        .collect::<Vec<_>>();
    nav_candidates.sort_by(|left, right| left.0.cmp(right.0));
    let selected = nav_candidates
        .first()
        .map(|(_, item)| (*item, false))
        .or_else(|| {
            let toc_id = package.toc_id.as_ref()?;
            package.manifest.get(toc_id).map(|item| (item, true))
        });
    let Some((toc_item, declared_ncx)) = selected else {
        return Ok(Vec::new());
    };
    let Some(toc_path) = join_archive_path(package_base, &toc_item.href) else {
        return Ok(Vec::new());
    };
    let Some(contents) = read_text_entry(archive, entries, &toc_path, MAX_EPUB_XML_BYTES)? else {
        return Ok(Vec::new());
    };
    let raw_entries = if declared_ncx || toc_item.media_type.to_ascii_lowercase().contains("ncx") {
        parse_ncx_toc(&contents)?
    } else {
        parse_epub3_toc(&contents)?
    };
    let toc_base = parent_directory(&toc_path);
    let mut toc = Vec::new();
    for raw in raw_entries {
        let (target_href, fragment) = split_fragment(&raw.href);
        let Some(target_path) = join_archive_path(&toc_base, target_href) else {
            continue;
        };
        let Some(chapter) = chapters.iter().find(|chapter| chapter.href == target_path) else {
            continue;
        };
        let mut effective_fragment = fragment;
        let mut block_id = fragment.and_then(|source_id| {
            chapter
                .document
                .anchors
                .iter()
                .find(|anchor| anchor.source_id == source_id)
                .map(|anchor| anchor.block_id.clone())
        });
        if fragment.is_some() && block_id.is_none() {
            let first_block_id = chapter
                .document
                .blocks
                .first()
                .map(|block| block.id.as_str());
            let start_is_claimed = toc.iter().any(|entry: &ParsedEpubTocEntry| {
                entry.chapter_id == chapter.id
                    && (entry.block_id.is_none() || entry.block_id.as_deref() == first_block_id)
            });
            if start_is_claimed {
                continue;
            }
            // Real EPUBs frequently append a decorative `#top` without an
            // actual anchor. The first such target can safely name the spine
            // document's start; subsequent broken fragments remain ambiguous.
            effective_fragment = None;
            block_id = None;
        }
        let title = raw.title.split_whitespace().collect::<Vec<_>>().join(" ");
        if title.is_empty() {
            continue;
        }
        let href = match effective_fragment {
            Some(fragment) => format!("{target_path}#{fragment}"),
            None => target_path,
        };
        toc.push(ParsedEpubTocEntry {
            id: format!("toc-{:06}", toc.len()),
            title,
            depth: raw.depth,
            href,
            chapter_id: chapter.id.clone(),
            block_id,
        });
    }
    Ok(toc)
}

fn parse_epub3_toc(contents: &str) -> Result<Vec<RawTocEntry>, MediaError> {
    let mut reader = Reader::from_str(contents);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut in_toc = false;
    let mut nav_nesting = 0_usize;
    let mut list_depth = 0_usize;
    let mut active_link: Option<RawTocEntry> = None;
    let mut toc = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) => {
                let tag = local_name(element.name().as_ref());
                let element_attributes = attributes(&element);
                if !in_toc {
                    let is_toc = tag == "nav"
                        && (element_attributes.get("type").is_some_and(|value| {
                            value.split_whitespace().any(|value| value == "toc")
                        }) || element_attributes
                            .get("role")
                            .is_some_and(|value| value == "doc-toc"));
                    if is_toc {
                        in_toc = true;
                        nav_nesting = 1;
                    }
                } else {
                    nav_nesting += 1;
                    match tag.as_str() {
                        "ol" | "ul" => list_depth += 1,
                        "a" => {
                            if let Some(href) = element_attributes.get("href") {
                                active_link = Some(RawTocEntry {
                                    title: String::new(),
                                    href: href.clone(),
                                    depth: list_depth.saturating_sub(1),
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Text(text)) if active_link.is_some() => {
                if let Some(link) = active_link.as_mut() {
                    link.title.push_str(&decode_xml_text(&text));
                }
            }
            Ok(Event::End(element)) if in_toc => {
                let tag = local_name(element.name().as_ref());
                if tag == "a"
                    && let Some(link) = active_link.take()
                {
                    toc.push(link);
                }
                if matches!(tag.as_str(), "ol" | "ul") {
                    list_depth = list_depth.saturating_sub(1);
                }
                nav_nesting = nav_nesting.saturating_sub(1);
                if nav_nesting == 0 {
                    in_toc = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(MediaError::InvalidEpub(format!(
                    "malformed EPUB navigation XML: {error}"
                )));
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(toc)
}

#[derive(Debug)]
struct NcxPoint {
    order: usize,
    depth: usize,
    title: String,
    href: String,
}

fn parse_ncx_toc(contents: &str) -> Result<Vec<RawTocEntry>, MediaError> {
    let mut reader = Reader::from_str(contents);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut in_nav_map = false;
    let mut nav_map_nesting = 0_usize;
    let mut captures_label = false;
    let mut next_order = 0_usize;
    let mut stack: Vec<NcxPoint> = Vec::new();
    let mut points = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) => {
                let tag = local_name(element.name().as_ref());
                if !in_nav_map && tag == "navmap" {
                    in_nav_map = true;
                    nav_map_nesting = 1;
                } else if in_nav_map {
                    nav_map_nesting += 1;
                    match tag.as_str() {
                        "navpoint" => {
                            stack.push(NcxPoint {
                                order: next_order,
                                depth: stack.len(),
                                title: String::new(),
                                href: String::new(),
                            });
                            next_order += 1;
                        }
                        "text" => captures_label = !stack.is_empty(),
                        "content" => {
                            if let Some(point) = stack.last_mut()
                                && let Some(source) = attributes(&element).get("src")
                            {
                                point.href = source.clone();
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Empty(element)) if in_nav_map => {
                if local_name(element.name().as_ref()) == "content"
                    && let Some(point) = stack.last_mut()
                    && let Some(source) = attributes(&element).get("src")
                {
                    point.href = source.clone();
                }
            }
            Ok(Event::Text(text)) if captures_label => {
                if let Some(point) = stack.last_mut() {
                    point.title.push_str(&decode_xml_text(&text));
                }
            }
            Ok(Event::End(element)) if in_nav_map => {
                let tag = local_name(element.name().as_ref());
                if tag == "text" {
                    captures_label = false;
                } else if tag == "navpoint"
                    && let Some(point) = stack.pop()
                {
                    points.push(point);
                }
                nav_map_nesting = nav_map_nesting.saturating_sub(1);
                if nav_map_nesting == 0 {
                    in_nav_map = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(MediaError::InvalidEpub(format!(
                    "malformed NCX navigation XML: {error}"
                )));
            }
            _ => {}
        }
        buffer.clear();
    }

    points.sort_by_key(|point| point.order);
    Ok(points
        .into_iter()
        .filter(|point| !point.href.is_empty())
        .map(|point| RawTocEntry {
            title: point.title,
            href: point.href,
            depth: point.depth,
        })
        .collect())
}

fn decode_xml_text(text: &quick_xml::events::BytesText<'_>) -> String {
    let Ok(decoded) = text.decode() else {
        return String::new();
    };
    quick_xml::escape::unescape(&decoded)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| decoded.into_owned())
}

fn split_fragment(href: &str) -> (&str, Option<&str>) {
    match href.split_once('#') {
        Some((path, fragment)) if !fragment.is_empty() => (path, Some(fragment)),
        Some((path, _)) => (path, None),
        None => (href, None),
    }
}

fn find_attribute(
    xml: &str,
    expected_tag: &str,
    expected_attribute: &str,
) -> Result<Option<String>, MediaError> {
    let mut reader = Reader::from_str(xml);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                if local_name(element.name().as_ref()) == expected_tag {
                    return Ok(attributes(&element).get(expected_attribute).cloned());
                }
            }
            Ok(Event::Eof) => return Ok(None),
            Err(error) => {
                return Err(MediaError::InvalidEpub(format!(
                    "malformed container XML: {error}"
                )));
            }
            _ => {}
        }
        buffer.clear();
    }
}

fn attributes(element: &quick_xml::events::BytesStart<'_>) -> HashMap<String, String> {
    element
        .attributes()
        .flatten()
        .filter_map(|attribute| {
            let name = local_name(attribute.key.as_ref());
            let value = attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .ok()?
                .into_owned();
            Some((name, value))
        })
        .collect()
}

fn local_name(raw: &[u8]) -> String {
    let name = String::from_utf8_lossy(raw);
    name.rsplit(':')
        .next()
        .unwrap_or(&name)
        .to_ascii_lowercase()
}

fn index_entries(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
) -> Result<HashMap<String, usize>, MediaError> {
    let mut entries = HashMap::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| MediaError::InvalidEpub(error.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        if let Some(name) = normalize_archive_path(entry.name()) {
            entries.entry(name).or_insert(index);
        }
    }
    Ok(entries)
}

fn read_text_entry(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    entries: &HashMap<String, usize>,
    path: &str,
    limit: usize,
) -> Result<Option<String>, MediaError> {
    let Some(path) = normalize_archive_path(path) else {
        return Ok(None);
    };
    let Some(index) = entries.get(&path) else {
        return Ok(None);
    };
    let entry = archive
        .by_index(*index)
        .map_err(|error| MediaError::InvalidEpub(error.to_string()))?;
    if entry.size() > limit as u64 {
        return Err(MediaError::InvalidEpub(format!(
            "archive entry {path} uses {} bytes; limit is {limit}",
            entry.size()
        )));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| MediaError::InvalidEpub(error.to_string()))?;
    if bytes.len() > limit {
        return Err(MediaError::InvalidEpub(format!(
            "archive entry {path} exceeds {limit} bytes"
        )));
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn read_binary_entry(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    entries: &HashMap<String, usize>,
    path: &str,
    limit: usize,
) -> Result<Option<Vec<u8>>, MediaError> {
    let Some(path) = normalize_archive_path(path) else {
        return Ok(None);
    };
    let Some(index) = entries.get(&path) else {
        return Ok(None);
    };
    let entry = archive
        .by_index(*index)
        .map_err(|error| MediaError::InvalidEpub(error.to_string()))?;
    if entry.size() > limit as u64 {
        return Err(MediaError::EpubAssetTooLarge {
            actual: entry.size(),
            limit,
        });
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| MediaError::InvalidEpub(error.to_string()))?;
    if bytes.len() > limit {
        return Err(MediaError::EpubAssetTooLarge {
            actual: bytes.len() as u64,
            limit,
        });
    }
    Ok(Some(bytes))
}

/// Normalizes only actual archive entries. Parent traversal is rejected here;
/// it is resolved separately for manifest hrefs relative to the package file.
fn normalize_archive_path(path: &str) -> Option<String> {
    if path.starts_with('/') || path.starts_with('\\') {
        return None;
    }
    let slashed = path.replace('\\', "/");
    let mut parts = Vec::new();
    for part in slashed.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            value => parts.push(value),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/").to_ascii_lowercase())
}

fn join_archive_path(base: &str, href: &str) -> Option<String> {
    let href = href.split('#').next().unwrap_or(href).replace('\\', "/");
    let mut parts: Vec<&str> = if href.starts_with('/') {
        Vec::new()
    } else {
        base.split('/').filter(|part| !part.is_empty()).collect()
    };
    for part in href.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            value => parts.push(value),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/").to_ascii_lowercase())
}

fn parent_directory(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(directory, _)| directory.to_string())
        .unwrap_or_default()
}
