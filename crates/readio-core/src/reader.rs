use std::collections::{HashMap, HashSet};

use pulldown_cmark::{Options as MarkdownOptions, Parser as MarkdownParser, html};
use serde::{Deserialize, Serialize};

use crate::{
    documents::{
        Block, BlockKind, Document, DocumentAnchor, DocumentError, HtmlParseOptions,
        ResolvedMediaAsset, document_from_semantic_blocks, parse_html,
        parse_html_with_media_resolver, refresh_document_revision,
    },
    highlight::{DocumentTextIndex, index_document},
};

pub const READER_IR_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderChapterSource {
    pub id: String,
    pub title: String,
    pub document: Document,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderNavigationSource {
    pub id: String,
    pub title: String,
    pub depth: usize,
    pub chapter_id: String,
    pub block_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderIr {
    pub ir_version: u16,
    pub document: Document,
    pub text_index: DocumentTextIndex,
    pub chapters: Vec<ReaderChapter>,
    pub navigation: Vec<ReaderNavigation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderChapter {
    pub id: String,
    pub title: String,
    pub block_range: [u64; 2],
    pub sentence_range: [u64; 2],
    pub char_range: [u64; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderNavigation {
    pub id: String,
    pub title: String,
    pub depth: usize,
    pub chapter_id: String,
    pub block_id: Option<String>,
    pub sentence_index: Option<u32>,
}

/// Parses one HTML source into the complete value consumed by reader GUIs.
/// Heading navigation and every scalar range are fixed before crossing FFI.
pub fn parse_html_reader(
    html: &str,
    options: &HtmlParseOptions,
) -> Result<ReaderIr, DocumentError> {
    let document = parse_html(html, options)?;
    Ok(compose_single_document_reader_ir(
        html.as_bytes(),
        "html-reader-ir-1",
        document,
    ))
}

/// Parses plain text into semantic paragraphs before producing Reader IR.
/// Blank-line grouping and whitespace normalization therefore stay identical
/// across native and Web renderers.
pub fn parse_plain_text_reader(
    text: &str,
    options: &HtmlParseOptions,
) -> Result<ReaderIr, DocumentError> {
    if text.len() > options.max_input_bytes {
        return Err(DocumentError::InputTooLarge {
            actual: text.len(),
            limit: options.max_input_bytes,
        });
    }
    let normalized_newlines = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut paragraphs = Vec::new();
    let mut current = Vec::new();
    for line in normalized_newlines.lines() {
        if line.trim().is_empty() {
            push_plain_text_paragraph(&mut paragraphs, &mut current, options.max_blocks)?;
        } else {
            current.push(line);
        }
    }
    push_plain_text_paragraph(&mut paragraphs, &mut current, options.max_blocks)?;
    let document = document_from_semantic_blocks(
        text.as_bytes(),
        "plain-text-ir-1",
        paragraphs
            .into_iter()
            .map(|paragraph| (BlockKind::Paragraph, paragraph))
            .collect(),
    );
    Ok(compose_single_document_reader_ir(
        text.as_bytes(),
        "plain-text-reader-ir-1",
        document,
    ))
}

/// Parses Markdown into semantic blocks before producing Reader IR. The
/// renderer never receives Markdown syntax and therefore cannot invent a
/// platform-specific heading, quote, emphasis, or navigation interpretation.
pub fn parse_markdown_reader(
    markdown: &str,
    options: &HtmlParseOptions,
) -> Result<ReaderIr, DocumentError> {
    if markdown.len() > options.max_input_bytes {
        return Err(DocumentError::InputTooLarge {
            actual: markdown.len(),
            limit: options.max_input_bytes,
        });
    }

    let mut rendered_html = String::new();
    let parser = MarkdownParser::new_ext(
        markdown,
        MarkdownOptions::ENABLE_STRIKETHROUGH
            | MarkdownOptions::ENABLE_TABLES
            | MarkdownOptions::ENABLE_TASKLISTS,
    );
    html::push_html(&mut rendered_html, parser);

    // pulldown-cmark escapes authored text while producing HTML, so the
    // intermediate can be larger than the source. Its expansion is linear;
    // this derived ceiling retains the source budget without a false rejection
    // of documents containing many escapable characters.
    let generated_limit = options.max_input_bytes.saturating_mul(8);
    if rendered_html.len() > generated_limit {
        return Err(DocumentError::InputTooLarge {
            actual: rendered_html.len(),
            limit: generated_limit,
        });
    }
    let mut html_options = options.clone();
    html_options.max_input_bytes = generated_limit;
    let document = parse_html_with_media_resolver(
        &rendered_html,
        &html_options,
        &resolve_markdown_media,
    )?;
    Ok(compose_single_document_reader_ir(
        markdown.as_bytes(),
        "markdown-reader-ir-1",
        document,
    ))
}

fn resolve_markdown_media(source: &str) -> Option<ResolvedMediaAsset> {
    let source = source.trim();
    if source.is_empty() {
        return None;
    }
    if let Ok(url) = url::Url::parse(source) {
        return matches!(url.scheme(), "http" | "https").then(|| {
            let source = url.to_string();
            ResolvedMediaAsset {
                id: markdown_asset_id(&source),
                source,
            }
        });
    }
    if source.starts_with('/') || source.contains('\\') {
        return None;
    }
    let segments = source
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>();
    if segments.is_empty() || segments.iter().any(|segment| *segment == "..") {
        return None;
    }
    let source = segments.join("/");
    Some(ResolvedMediaAsset {
        id: markdown_asset_id(&source),
        source,
    })
}

fn markdown_asset_id(source: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"readio-markdown-asset-v1\0");
    hasher.update(source.as_bytes());
    format!("asset-{}", hasher.finalize().to_hex())
}

fn push_plain_text_paragraph(
    paragraphs: &mut Vec<String>,
    current: &mut Vec<&str>,
    max_blocks: usize,
) -> Result<(), DocumentError> {
    if current.is_empty() {
        return Ok(());
    }
    if paragraphs.len() >= max_blocks {
        return Err(DocumentError::TooManyBlocks { limit: max_blocks });
    }
    let paragraph = current
        .drain(..)
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ");
    if !paragraph.is_empty() {
        paragraphs.push(paragraph);
    }
    Ok(())
}

fn compose_single_document_reader_ir(
    source: &[u8],
    parser_version: &str,
    document: Document,
) -> ReaderIr {
    let title = document
        .blocks
        .iter()
        .find_map(|block| {
            matches!(block.kind, BlockKind::Heading { .. }).then(|| block.text.clone())
        })
        .unwrap_or_else(|| "Document".to_string());
    let mut navigation = document
        .blocks
        .iter()
        .filter_map(|block| {
            let BlockKind::Heading { level } = block.kind else {
                return None;
            };
            Some(ReaderNavigationSource {
                id: format!("document#{}", block.id),
                title: block.text.clone(),
                depth: usize::from(level.saturating_sub(1)),
                chapter_id: "document".to_string(),
                block_id: Some(block.id.clone()),
            })
        })
        .collect::<Vec<_>>();
    if navigation.is_empty() {
        navigation.push(ReaderNavigationSource {
            id: "document".to_string(),
            title: title.clone(),
            depth: 0,
            chapter_id: "document".to_string(),
            block_id: None,
        });
    }
    compose_reader_ir(
        source,
        parser_version,
        vec![ReaderChapterSource {
            id: "document".to_string(),
            title,
            document,
        }],
        navigation,
    )
}

/// Composes parser-owned chapter Documents into the single versioned value
/// consumed by reader GUIs. Chapter-local block IDs are deterministically
/// scoped before the whole-book revision and text index are calculated.
pub fn compose_reader_ir(
    source: &[u8],
    parser_version: &str,
    chapters: Vec<ReaderChapterSource>,
    navigation: Vec<ReaderNavigationSource>,
) -> ReaderIr {
    let mut document = Document::default();
    let mut chapter_ranges = Vec::with_capacity(chapters.len());
    let mut block_mapping = HashMap::new();
    let mut seen_assets = HashSet::new();

    for chapter in chapters {
        let block_start = document.blocks.len();
        let chapter_scope = chapter.id.clone();
        for block in chapter.document.blocks {
            let original_id = block.id.clone();
            let scoped_id = scoped_block_id(&chapter_scope, &original_id);
            block_mapping.insert((chapter_scope.clone(), original_id), scoped_id.clone());
            document.blocks.push(Block {
                id: scoped_id,
                kind: block.kind,
                text: block.text,
                inline_spans: block.inline_spans,
            });
        }
        for asset in chapter.document.assets {
            if seen_assets.insert(asset.id.clone()) {
                document.assets.push(asset);
            }
        }
        for anchor in chapter.document.anchors {
            if let Some(block_id) = block_mapping
                .get(&(chapter_scope.clone(), anchor.block_id))
                .cloned()
            {
                document.anchors.push(DocumentAnchor {
                    source_id: format!("{}#{}", chapter_scope, anchor.source_id),
                    block_id,
                });
            }
        }
        chapter_ranges.push((
            chapter.id,
            chapter.title,
            [block_start as u64, document.blocks.len() as u64],
        ));
    }

    refresh_document_revision(source, parser_version, &mut document);
    let text_index = index_document(&document);
    let chapters = chapter_ranges
        .into_iter()
        .map(|(id, title, block_range)| {
            let block_ids = document.blocks[block_range[0] as usize..block_range[1] as usize]
                .iter()
                .map(|block| block.id.as_str())
                .collect::<HashSet<_>>();
            let chapter_sentences = text_index
                .sentences
                .iter()
                .filter(|sentence| block_ids.contains(sentence.block_id.as_str()))
                .collect::<Vec<_>>();
            let sentence_start = chapter_sentences
                .first()
                .map_or(0, |sentence| u64::from(sentence.index));
            let sentence_end = chapter_sentences
                .last()
                .map_or(sentence_start, |sentence| u64::from(sentence.index) + 1);
            ReaderChapter {
                id,
                title,
                block_range,
                sentence_range: [sentence_start, sentence_end],
                char_range: chapter_char_range(&document, block_range),
            }
        })
        .collect::<Vec<_>>();
    let navigation = navigation
        .into_iter()
        .map(|entry| {
            let block_id = entry.block_id.and_then(|block_id| {
                block_mapping
                    .get(&(entry.chapter_id.clone(), block_id))
                    .cloned()
            });
            let sentence_index = block_id
                .as_ref()
                .and_then(|block_id| {
                    text_index
                        .sentences
                        .iter()
                        .find(|sentence| sentence.block_id == *block_id)
                        .map(|sentence| sentence.index)
                })
                .or_else(|| {
                    chapters
                        .iter()
                        .find(|chapter| chapter.id == entry.chapter_id)
                        .and_then(|chapter| {
                            (chapter.sentence_range[0] < chapter.sentence_range[1])
                                .then(|| chapter.sentence_range[0] as u32)
                        })
                });
            ReaderNavigation {
                id: entry.id,
                title: entry.title,
                depth: entry.depth,
                chapter_id: entry.chapter_id,
                block_id,
                sentence_index,
            }
        })
        .collect();

    ReaderIr {
        ir_version: READER_IR_VERSION,
        document,
        text_index,
        chapters,
        navigation,
    }
}

fn scoped_block_id(chapter_id: &str, block_id: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"readio-reader-block-v1\0");
    hasher.update(chapter_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(block_id.as_bytes());
    format!("block-{}", hasher.finalize().to_hex())
}

fn chapter_char_range(document: &Document, block_range: [u64; 2]) -> [u64; 2] {
    let start_index = block_range[0] as usize;
    let end_index = block_range[1] as usize;
    let start = document
        .blocks
        .iter()
        .take(start_index)
        .enumerate()
        .map(|(index, block)| {
            block.text.chars().count() as u64 + u64::from(index + 1 < document.blocks.len())
        })
        .sum();
    let length = document.blocks[start_index..end_index]
        .iter()
        .enumerate()
        .map(|(index, block)| {
            block.text.chars().count() as u64 + u64::from(index + 1 < end_index - start_index)
        })
        .sum::<u64>();
    [start, start.saturating_add(length)]
}
