//! Terminal projection of the shared Rust SDK PDF publication.
//!
//! Glyph extraction, page-furniture cleanup, paragraph reconstruction,
//! hyphenation repair, outline/heading/page grouping, metadata, and page
//! provenance all live in `readio-core::media`. This adapter only supplies
//! localized fallback titles and the terminal book shape.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use readio_sdk::ReadioSdk;
use readio_sdk::media::{MediaError, ParsedPdfChapter};

use super::{Book, Chapter, Source, html};

pub fn load(path: &Path) -> Result<Book> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot open {}", path.display()))?;
    let publication = ReadioSdk::new()
        .media
        .parse_pdf(&bytes)
        .map_err(|error| terminal_error(path, error))?;
    let file = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "book.pdf".to_string());
    let chapters = publication
        .chapters
        .iter()
        .enumerate()
        .filter_map(|(index, source)| project_chapter(source, index, &file))
        .collect::<Vec<_>>();
    if chapters.is_empty() {
        return Err(anyhow!(
            "{}",
            crate::i18n::tf("pdf.empty", &[&path.display()])
        ));
    }
    let title = publication
        .title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".to_string())
        });

    Ok(Book {
        id: String::new(),
        title,
        author: publication.author,
        path: Some(path.to_path_buf()),
        source: Source::Pdf,
        chapters,
        cover: None,
    })
}

fn project_chapter(source: &ParsedPdfChapter, index: usize, file: &str) -> Option<Chapter> {
    let paras = html::project_document(&source.document);
    if paras.is_empty() {
        return None;
    }
    let span = page_span(source.page_start, source.page_end);
    let title = source
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| crate::i18n::tf("pdf.part", &[&(index + 1), &span]));
    Some(Chapter::new(title, format!("{file}#{span}"), paras))
}

fn page_span(start: usize, end: usize) -> String {
    if start == end {
        format!("p{start}")
    } else {
        format!("p{start}-{end}")
    }
}

fn terminal_error(path: &Path, error: MediaError) -> anyhow::Error {
    match error {
        MediaError::ScannedPdf {
            characters_per_page,
        } => anyhow!(
            "{}",
            crate::i18n::tf("pdf.scanned", &[&characters_per_page])
        ),
        MediaError::EmptyPdf => anyhow!("{}", crate::i18n::tf("pdf.empty", &[&path.display()])),
        MediaError::InvalidPdf(_) | MediaError::InputTooLarge { .. } => {
            anyhow!("{}", crate::i18n::tf("pdf.broken", &[&path.display()]))
        }
        other => anyhow!("{other}"),
    }
}
