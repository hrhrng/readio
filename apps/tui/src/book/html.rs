//! Terminal projection of the shared Rust HTML Document IR.
//!
//! HTML parsing, malformed-markup recovery, semantic inline spans, active
//! content filtering, and resource budgets all live in Rust Core. This module
//! only maps semantic blocks and Unicode-scalar spans into terminal shapes.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use readio_sdk::ReadioSdk;
use readio_sdk::documents::{Block, BlockKind, Document, HtmlParseOptions, InlineSpan, MediaAsset};

use super::{Book, Chapter, Emphasis, Para, Rich, Source};

pub fn load(path: &Path) -> Result<Book> {
    let raw = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let html = String::from_utf8_lossy(&raw);
    let document = ReadioSdk::new()
        .documents
        .parse_html(&html, &HtmlParseOptions::default())
        .map_err(|error| anyhow!("cannot parse {}: {error:?}", path.display()))?;
    let title = document
        .blocks
        .iter()
        .find_map(|block| match block.kind {
            BlockKind::Heading { .. } => Some(block.text.clone()),
            _ => None,
        })
        .or_else(|| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "Untitled".to_string());
    let paras = project_document(&document);
    anyhow::ensure!(!paras.is_empty(), "{} has no readable text", path.display());
    let href = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document.html".to_string());

    Ok(Book {
        id: String::new(),
        title: title.clone(),
        author: None,
        path: Some(path.to_path_buf()),
        source: Source::Text,
        chapters: vec![Chapter::new(title, href, paras)],
        cover: None,
    })
}

/// Test/support entry point that deliberately uses the production Core parser.
pub fn to_paras(html: &str) -> Vec<Para> {
    ReadioSdk::new()
        .documents
        .parse_html(html, &HtmlParseOptions::default())
        .map(|document| project_document(&document))
        .unwrap_or_default()
}

pub(crate) fn project_document(document: &Document) -> Vec<Para> {
    let assets = document
        .assets
        .iter()
        .map(|asset| (asset.id.as_str(), asset))
        .collect::<HashMap<_, _>>();
    document
        .blocks
        .iter()
        .filter_map(|block| project_block(block, &assets))
        .collect()
}

fn project_block(block: &Block, assets: &HashMap<&str, &MediaAsset>) -> Option<Para> {
    match &block.kind {
        BlockKind::Heading { level } => Some(Para::Heading {
            level: *level,
            text: block.text.clone(),
        }),
        BlockKind::Paragraph => Some(Para::Text(project_rich_text(
            &block.text,
            &block.inline_spans,
        ))),
        BlockKind::Quote => Some(Para::Quote(project_rich_text(
            &block.text,
            &block.inline_spans,
        ))),
        BlockKind::Code => Some(Para::Code(block.text.clone())),
        BlockKind::Image { asset_id } => {
            let asset = assets.get(asset_id.as_str())?;
            Some(Para::Image {
                src: asset.source.clone().into(),
                alt: asset.alt.clone(),
            })
        }
    }
}

pub(crate) fn project_rich_text(text: &str, spans: &[InlineSpan]) -> Rich {
    let mut scalar_boundaries = text
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    scalar_boundaries.push(text.len());
    let emphasis = spans
        .iter()
        .filter(|span| span.bold || span.italic)
        .filter_map(|span| {
            let start = *scalar_boundaries.get(span.range[0] as usize)?;
            let end = *scalar_boundaries.get(span.range[1] as usize)?;
            let start = u32::try_from(start).ok()?;
            let end = u32::try_from(end).ok()?;
            (start < end).then_some(Emphasis {
                start,
                end,
                strong: span.bold,
            })
        })
        .collect();
    Rich::new(text.to_string(), emphasis)
}

/// First heading in a projected document, used as an EPUB title fallback.
pub fn first_heading(paras: &[Para]) -> Option<String> {
    paras.iter().find_map(|para| match para {
        Para::Heading { text, .. } => Some(text.clone()),
        _ => None,
    })
}
