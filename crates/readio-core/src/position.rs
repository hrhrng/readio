use serde::{Deserialize, Serialize};

use crate::documents::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Affinity {
    Forward,
    Backward,
}

pub const TEXT_LOCATOR_VERSION: u16 = 1;
const LOCATOR_EXACT_SCALARS: usize = 32;
const LOCATOR_CONTEXT_SCALARS: usize = 16;

/// A renderer-independent text anchor. All offsets count Unicode scalars in
/// the normalized Document IR; UTF-8, UTF-16, DOM paths, and layout never enter
/// the durable coordinate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextLocator {
    pub version: u16,
    pub document_revision: String,
    pub char_offset: u64,
    pub block_id: String,
    pub offset_in_block: u64,
    pub affinity: Affinity,
    pub quote: TextQuote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextQuote {
    pub prefix: String,
    pub exact: String,
    pub suffix: String,
    pub offset_in_exact: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocatorResolutionKind {
    ExactRevision,
    StableBlock,
    TextQuote,
    FallbackOffset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocatorResolution {
    pub kind: LocatorResolutionKind,
    pub projection: PositionProjection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocatorError {
    UnsupportedVersion { version: u16 },
    QuoteTooLarge,
}

impl std::fmt::Display for LocatorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion { version } => {
                write!(formatter, "text locator version {version} is not supported")
            }
            Self::QuoteTooLarge => formatter.write_str("text locator quote exceeds its budget"),
        }
    }
}

impl std::error::Error for LocatorError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingPosition {
    pub item_id: String,
    pub document_revision: String,
    pub char_offset: u64,
    pub affinity: Affinity,
    pub observed_at_unix_ms: i64,
    pub session_id: String,
    pub mutation_id: String,
    pub base_version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<TextLocator>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionProjection {
    pub char_offset: u64,
    pub block_id: Option<String>,
    pub offset_in_block: u64,
    pub chapter_id: Option<String>,
    pub sentence_index: Option<u32>,
    pub progress_basis_points: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoredPosition {
    pub char_offset: u64,
    pub affinity: Affinity,
    pub revision_changed: bool,
    pub projection: PositionProjection,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator_resolution: Option<LocatorResolutionKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionError {
    BlockNotFound { block_id: String },
}

impl std::fmt::Display for PositionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlockNotFound { block_id } => {
                write!(formatter, "document block was not found: {block_id}")
            }
        }
    }
}

impl std::error::Error for PositionError {}

/// Project a durable scalar offset into the current document without changing
/// the durable coordinate. Projection is a convenience for scrolling and
/// highlighting; callers must persist `char_offset` rather than its fields.
pub fn project_offset(document: &Document, offset: u64, affinity: Affinity) -> PositionProjection {
    let char_count = document.revision.char_count;
    let char_offset = offset.min(char_count);
    let mut cursor = 0_u64;

    if document.blocks.is_empty() {
        return PositionProjection {
            char_offset,
            block_id: None,
            offset_in_block: 0,
            chapter_id: None,
            sentence_index: None,
            progress_basis_points: progress_basis_points(char_offset, char_count),
        };
    }

    for (index, block) in document.blocks.iter().enumerate() {
        let length = block.text.chars().count() as u64;
        let end = cursor.saturating_add(length);
        let is_last = index + 1 == document.blocks.len();
        let in_block = if char_offset < end || (char_offset == end && is_last) {
            Some(char_offset.saturating_sub(cursor).min(length))
        } else if char_offset == end {
            match affinity {
                Affinity::Forward => None,
                Affinity::Backward => Some(length),
            }
        } else {
            None
        };

        if let Some(offset_in_block) = in_block {
            return PositionProjection {
                char_offset,
                block_id: Some(block.id.clone()),
                offset_in_block,
                chapter_id: None,
                sentence_index: None,
                progress_basis_points: progress_basis_points(char_offset, char_count),
            };
        }

        cursor = end.saturating_add(if is_last { 0 } else { 1 });
    }

    let last = document.blocks.last().expect("non-empty document");
    PositionProjection {
        char_offset,
        block_id: Some(last.id.clone()),
        offset_in_block: last.text.chars().count() as u64,
        chapter_id: None,
        sentence_index: None,
        progress_basis_points: progress_basis_points(char_offset, char_count),
    }
}

/// Resolves a renderer-facing block anchor back into the one durable scalar
/// coordinate. Platform adapters use this instead of duplicating newline and
/// Unicode accounting rules.
pub fn resolve_block_offset(
    document: &Document,
    block_id: &str,
    offset_in_block: u64,
    affinity: Affinity,
) -> Result<PositionProjection, PositionError> {
    let mut block_start = 0_u64;

    for (index, block) in document.blocks.iter().enumerate() {
        let block_length = block.text.chars().count() as u64;
        if block.id == block_id {
            return Ok(project_offset(
                document,
                block_start.saturating_add(offset_in_block.min(block_length)),
                affinity,
            ));
        }
        block_start = block_start
            .saturating_add(block_length)
            .saturating_add(u64::from(index + 1 < document.blocks.len()));
    }

    Err(PositionError::BlockNotFound {
        block_id: block_id.to_string(),
    })
}

pub fn restore_position(stored: &ReadingPosition, document: &Document) -> RestoredPosition {
    let revision_changed = stored.document_revision != document.revision.revision_digest;
    let locator_resolution = revision_changed
        .then(|| stored.locator.as_ref())
        .flatten()
        .and_then(|locator| restore_text_locator(locator, document).ok());
    let projection = locator_resolution.as_ref().map_or_else(
        || project_offset(document, stored.char_offset, stored.affinity),
        |resolution| resolution.projection.clone(),
    );
    RestoredPosition {
        char_offset: projection.char_offset,
        affinity: stored.affinity,
        revision_changed,
        projection,
        locator_resolution: locator_resolution.map(|resolution| resolution.kind),
    }
}

pub fn create_text_locator(
    document: &Document,
    char_offset: u64,
    affinity: Affinity,
) -> TextLocator {
    let projection = project_offset(document, char_offset, affinity);
    let scalars = document.canonical_text().chars().collect::<Vec<_>>();
    let point = usize::try_from(projection.char_offset)
        .unwrap_or(usize::MAX)
        .min(scalars.len());
    let (exact_start, exact_end) = if point < scalars.len() {
        (
            point,
            point
                .saturating_add(LOCATOR_EXACT_SCALARS)
                .min(scalars.len()),
        )
    } else {
        (point.saturating_sub(LOCATOR_EXACT_SCALARS), point)
    };
    let prefix_start = exact_start.saturating_sub(LOCATOR_CONTEXT_SCALARS);
    let suffix_end = exact_end
        .saturating_add(LOCATOR_CONTEXT_SCALARS)
        .min(scalars.len());

    TextLocator {
        version: TEXT_LOCATOR_VERSION,
        document_revision: document.revision.revision_digest.clone(),
        char_offset: projection.char_offset,
        block_id: projection.block_id.unwrap_or_default(),
        offset_in_block: projection.offset_in_block,
        affinity,
        quote: TextQuote {
            prefix: scalars[prefix_start..exact_start].iter().collect(),
            exact: scalars[exact_start..exact_end].iter().collect(),
            suffix: scalars[exact_end..suffix_end].iter().collect(),
            offset_in_exact: (point - exact_start) as u64,
        },
    }
}

/// Resolve in decreasing order of authority. A text quote is accepted only
/// when one candidate scores strictly better than every other candidate;
/// ambiguous content therefore degrades visibly to the stored scalar offset.
pub fn restore_text_locator(
    locator: &TextLocator,
    document: &Document,
) -> Result<LocatorResolution, LocatorError> {
    if locator.version != TEXT_LOCATOR_VERSION {
        return Err(LocatorError::UnsupportedVersion {
            version: locator.version,
        });
    }
    if locator.quote.exact.chars().count() > LOCATOR_EXACT_SCALARS
        || locator.quote.prefix.chars().count() > LOCATOR_CONTEXT_SCALARS
        || locator.quote.suffix.chars().count() > LOCATOR_CONTEXT_SCALARS
    {
        return Err(LocatorError::QuoteTooLarge);
    }

    if locator.document_revision == document.revision.revision_digest {
        return Ok(LocatorResolution {
            kind: LocatorResolutionKind::ExactRevision,
            projection: project_offset(document, locator.char_offset, locator.affinity),
        });
    }

    if !locator.block_id.is_empty()
        && let Ok(projection) = resolve_block_offset(
            document,
            &locator.block_id,
            locator.offset_in_block,
            locator.affinity,
        )
    {
        return Ok(LocatorResolution {
            kind: LocatorResolutionKind::StableBlock,
            projection,
        });
    }

    if let Some(char_offset) = resolve_unique_quote(locator, document) {
        return Ok(LocatorResolution {
            kind: LocatorResolutionKind::TextQuote,
            projection: project_offset(document, char_offset, locator.affinity),
        });
    }

    Ok(LocatorResolution {
        kind: LocatorResolutionKind::FallbackOffset,
        projection: project_offset(document, locator.char_offset, locator.affinity),
    })
}

fn resolve_unique_quote(locator: &TextLocator, document: &Document) -> Option<u64> {
    let text = document.canonical_text().chars().collect::<Vec<_>>();
    let exact = locator.quote.exact.chars().collect::<Vec<_>>();
    if exact.is_empty() || exact.len() > text.len() {
        return None;
    }
    let prefix = locator.quote.prefix.chars().collect::<Vec<_>>();
    let suffix = locator.quote.suffix.chars().collect::<Vec<_>>();
    let mut candidates = Vec::new();
    for start in 0..=text.len() - exact.len() {
        if text[start..start + exact.len()] != exact {
            continue;
        }
        let prefix_matches = prefix.is_empty()
            || (start >= prefix.len() && text[start - prefix.len()..start] == prefix);
        let end = start + exact.len();
        let suffix_matches = suffix.is_empty()
            || (end + suffix.len() <= text.len() && text[end..end + suffix.len()] == suffix);
        let score = u8::from(prefix_matches) + u8::from(suffix_matches);
        candidates.push((score, start));
    }
    let best_score = candidates.iter().map(|(score, _)| *score).max()?;
    let mut best = candidates
        .into_iter()
        .filter(|(score, _)| *score == best_score);
    let (_, start) = best.next()?;
    if best.next().is_some() {
        return None;
    }
    let offset_in_exact = usize::try_from(locator.quote.offset_in_exact)
        .unwrap_or(usize::MAX)
        .min(exact.len());
    Some(start.saturating_add(offset_in_exact) as u64)
}

fn progress_basis_points(offset: u64, char_count: u64) -> u16 {
    if char_count == 0 {
        return 0;
    }
    ((u128::from(offset) * 10_000) / u128::from(char_count)).min(10_000) as u16
}
