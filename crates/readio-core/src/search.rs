use serde::{Deserialize, Serialize};

use crate::documents::Document;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRequest {
    pub document_revision: String,
    pub query: String,
    pub cursor: Option<String>,
    pub limit: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub hit_id: String,
    pub char_range: [u64; 2],
    pub block_id: String,
    pub snippet: String,
    pub snippet_match_range: [u32; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResultPage {
    pub folded_query: String,
    pub total_occurrences: u64,
    pub total_blocks: u64,
    pub hits: Vec<SearchHit>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    StaleRevision { expected: String, actual: String },
    InvalidCursor { cursor: String },
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "search revision is stale: requested {expected}, current {actual}"
            ),
            Self::InvalidCursor { cursor } => {
                write!(formatter, "search cursor is invalid: {cursor}")
            }
        }
    }
}

impl std::error::Error for SearchError {}

pub fn search_document(
    document: &Document,
    request: SearchRequest,
) -> Result<SearchResultPage, SearchError> {
    if request.document_revision != document.revision.revision_digest {
        return Err(SearchError::StaleRevision {
            expected: request.document_revision,
            actual: document.revision.revision_digest.clone(),
        });
    }

    let (folded_query, _) = fold(request.query.trim());
    let cursor = decode_cursor(request.cursor.as_deref())?;
    if folded_query.is_empty() {
        return Ok(SearchResultPage {
            folded_query,
            total_occurrences: 0,
            total_blocks: 0,
            hits: Vec::new(),
            next_cursor: None,
        });
    }

    let mut all_hits = Vec::new();
    let mut total_blocks = 0_u64;
    let mut block_start = 0_u64;

    for (block_index, block) in document.blocks.iter().enumerate() {
        let block_chars: Vec<char> = block.text.chars().collect();
        let (folded, source_spans) = fold(&block.text);
        let mut from = 0_usize;
        let mut found_in_block = false;

        while from <= folded.len() {
            let Some(relative) = folded[from..].find(&folded_query) else {
                break;
            };
            let start_folded = from + relative;
            let end_folded = start_folded + folded_query.len();
            let Some(&(local_start, _)) = source_spans.get(start_folded) else {
                break;
            };
            let Some(&(_, local_end)) = source_spans.get(end_folded.saturating_sub(1)) else {
                break;
            };
            let global_start = block_start.saturating_add(local_start);
            let global_end = block_start.saturating_add(local_end);
            let (snippet, snippet_match_range) = make_snippet(&block_chars, local_start, local_end);
            all_hits.push(SearchHit {
                hit_id: format!("{}:{global_start}:{global_end}", block.id),
                char_range: [global_start, global_end],
                block_id: block.id.clone(),
                snippet,
                snippet_match_range,
            });
            found_in_block = true;
            from = end_folded.max(start_folded.saturating_add(1));
        }

        if found_in_block {
            total_blocks += 1;
        }
        block_start = block_start
            .saturating_add(block_chars.len() as u64)
            .saturating_add(u64::from(block_index + 1 < document.blocks.len()));
    }

    let total_occurrences = all_hits.len() as u64;
    let page_start = cursor.min(all_hits.len());
    let page_end = page_start
        .saturating_add(request.limit as usize)
        .min(all_hits.len());
    let hits = all_hits[page_start..page_end].to_vec();
    // A zero-size request is useful for counts, but it must not manufacture a
    // cursor that points to the same page forever.
    let next_cursor =
        (page_end > page_start && page_end < all_hits.len()).then(|| page_end.to_string());

    Ok(SearchResultPage {
        folded_query,
        total_occurrences,
        total_blocks,
        hits,
        next_cursor,
    })
}

fn decode_cursor(cursor: Option<&str>) -> Result<usize, SearchError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    cursor
        .parse::<usize>()
        .map_err(|_| SearchError::InvalidCursor {
            cursor: cursor.to_string(),
        })
}

/// Fold to a searchable string while retaining the original scalar span for
/// every folded UTF-8 byte. A lowercase expansion therefore still highlights
/// the complete source scalar instead of producing an empty or shifted range.
fn fold(text: &str) -> (String, Vec<(u64, u64)>) {
    let mut folded = String::with_capacity(text.len());
    let mut source_spans = Vec::with_capacity(text.len());
    for (scalar_index, character) in text.chars().enumerate() {
        let plain = match character {
            '\u{ff01}'..='\u{ff5e}' => {
                char::from_u32(character as u32 - 0xFEE0).unwrap_or(character)
            }
            '\u{3000}' => ' ',
            other => other,
        };
        for lowered in plain.to_lowercase() {
            let before = folded.len();
            folded.push(lowered);
            source_spans.resize(folded.len(), (scalar_index as u64, scalar_index as u64 + 1));
            debug_assert!(source_spans.len() > before);
        }
    }
    (folded, source_spans)
}

fn make_snippet(chars: &[char], start: u64, end: u64) -> (String, [u32; 2]) {
    let start = usize::try_from(start)
        .unwrap_or(usize::MAX)
        .min(chars.len());
    let end = usize::try_from(end)
        .unwrap_or(usize::MAX)
        .min(chars.len())
        .max(start);
    let snippet_start = start.saturating_sub(24);
    let snippet_end = end.saturating_add(24).min(chars.len());
    let snippet: String = chars[snippet_start..snippet_end].iter().collect();
    let match_start = u32::try_from(start - snippet_start).unwrap_or(u32::MAX);
    let match_end = u32::try_from(end - snippet_start).unwrap_or(u32::MAX);
    (snippet, [match_start, match_end])
}
