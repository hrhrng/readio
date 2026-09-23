//! TUI projection of the shared Rust SDK EPUB publication.
//!
//! Package parsing, spine order, navigation anchors, author emphasis, image
//! identity, and safety budgets live in the shared Rust Core. This module owns only the
//! terminal-shaped projection: TOC targets become `Chapter` slices and image
//! bytes are materialized as files for the terminal decoder.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use readio_sdk::ReadioSdk;
use readio_sdk::documents::{Block, BlockKind, MediaAsset};
use readio_sdk::media::{ParsedEpub, ParsedEpubChapter, ParsedEpubTocEntry};

use super::{Book, Chapter, Para, Source, html, media};

pub fn load(path: &Path) -> Result<Book> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot open {}", path.display()))?;
    let publication = ReadioSdk::new()
        .media
        .parse_epub(&bytes)
        .with_context(|| format!("{} is not a readable EPUB", path.display()))?;

    let extracted = extract_terminal_images(path, &publication);
    let chapters = project_chapters(&publication, &extracted);
    if chapters.is_empty() {
        return Err(anyhow!("EPUB contains no readable chapters"));
    }

    let cover = publication.cover_asset_id.as_deref().and_then(|asset_id| {
        let asset = publication
            .assets
            .iter()
            .find(|asset| asset.id == asset_id)?;
        let file = terminal_asset_path(&extracted, &asset.href)?.clone();
        Some(super::Cover {
            file,
            href: asset.href.clone(),
        })
    });

    let title = publication
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("book")
                .to_string()
        });
    Ok(Book {
        id: String::new(),
        title,
        author: publication.author,
        path: Some(path.to_path_buf()),
        source: Source::Epub,
        chapters,
        cover,
    })
}

/// The SDK intentionally returns byte-free asset IDs. A terminal image widget
/// needs a path, so extraction stays an adapter effect and cannot influence
/// which archive entries become semantic blocks.
fn extract_terminal_images(path: &Path, publication: &ParsedEpub) -> BTreeMap<String, PathBuf> {
    if publication.assets.is_empty() {
        return BTreeMap::new();
    }
    let destination =
        crate::paths::images_dir().join(crate::book::synthetic_id(&path.to_string_lossy()));
    media::extract_epub_images(path, &destination).unwrap_or_default()
}

fn project_chapters(
    publication: &ParsedEpub,
    extracted: &BTreeMap<String, PathBuf>,
) -> Vec<Chapter> {
    let mut projected = Vec::new();
    for source in &publication.chapters {
        let mut line_cursor = 1_usize;
        for cut in chapter_cuts(source, &publication.toc) {
            let piece = project_blocks(
                &source.document.blocks[cut.from..cut.to],
                &source.document.assets,
                extracted,
            );
            if piece.is_empty() {
                continue;
            }
            let title = cut
                .title
                .filter(|title| !title.trim().is_empty())
                .or_else(|| html::first_heading(&piece).filter(|title| title.chars().count() <= 60))
                .unwrap_or_else(|| source.title.clone());
            let chapter = Chapter {
                title,
                href: cut.href.unwrap_or_else(|| source.href.clone()),
                paras: piece,
                first_line: line_cursor,
            };
            line_cursor += chapter.line_count().saturating_sub(1);
            projected.push(chapter);
        }
    }
    projected
}

fn project_blocks(
    blocks: &[Block],
    assets: &[MediaAsset],
    extracted: &BTreeMap<String, PathBuf>,
) -> Vec<Para> {
    let assets = assets
        .iter()
        .map(|asset| (asset.id.as_str(), asset))
        .collect::<HashMap<_, _>>();
    blocks
        .iter()
        .filter_map(|block| match &block.kind {
            BlockKind::Heading { level } => Some(Para::Heading {
                level: *level,
                text: block.text.clone(),
            }),
            BlockKind::Paragraph => Some(Para::Text(html::project_rich_text(
                &block.text,
                &block.inline_spans,
            ))),
            BlockKind::Quote => Some(Para::Quote(html::project_rich_text(
                &block.text,
                &block.inline_spans,
            ))),
            BlockKind::Code => Some(Para::Code(block.text.clone())),
            BlockKind::Image { asset_id } => {
                let asset = assets.get(asset_id.as_str())?;
                match terminal_asset_path(extracted, &asset.source) {
                    Some(file) => Some(Para::Image {
                        src: file.clone(),
                        alt: asset.alt.clone(),
                    }),
                    None if !asset.alt.trim().is_empty() => {
                        Some(Para::Text(asset.alt.clone().into()))
                    }
                    None => None,
                }
            }
        })
        .collect()
}

fn terminal_asset_path<'a>(
    extracted: &'a BTreeMap<String, PathBuf>,
    archive_href: &str,
) -> Option<&'a PathBuf> {
    extracted.get(&media::resolve_href("", archive_href))
}

#[derive(Debug)]
struct ChapterCut {
    from: usize,
    to: usize,
    title: Option<String>,
    href: Option<String>,
}

/// A TOC can point several times into one spine document. The SDK supplies
/// stable block IDs; this adapter merely slices the terminal's chapter view at
/// those already-resolved boundaries.
fn chapter_cuts(chapter: &ParsedEpubChapter, toc: &[ParsedEpubTocEntry]) -> Vec<ChapterCut> {
    let block_indexes = chapter
        .document
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut points = Vec::new();
    for entry in toc.iter().filter(|entry| entry.chapter_id == chapter.id) {
        let index = match entry.block_id.as_deref() {
            Some(block_id) => match block_indexes.get(block_id) {
                Some(index) => *index,
                None => continue,
            },
            None => 0,
        };
        if points
            .iter()
            .any(|(existing, _, _): &(usize, String, String)| *existing == index)
        {
            continue;
        }
        points.push((index, entry.title.clone(), entry.href.clone()));
    }
    points.sort_by_key(|(index, _, _)| *index);
    if points.first().map(|(index, _, _)| *index) != Some(0) {
        points.insert(0, (0, String::new(), chapter.href.clone()));
    }
    if points.is_empty() {
        points.push((0, String::new(), chapter.href.clone()));
    }

    points
        .iter()
        .enumerate()
        .filter_map(|(position, (from, title, href))| {
            let to = points
                .get(position + 1)
                .map(|(next, _, _)| *next)
                .unwrap_or(chapter.document.blocks.len());
            (to > *from).then(|| ChapterCut {
                from: *from,
                to,
                title: (!title.is_empty()).then(|| title.clone()),
                href: Some(href.clone()),
            })
        })
        .collect()
}
