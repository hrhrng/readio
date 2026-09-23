use serde::{Deserialize, Serialize};

use crate::{
    documents::Document,
    position::{Affinity, project_offset},
};

/// Default labels stay compact enough for every platform's bookmark row while
/// preserving a structural passage boundary at the beginning.
const BOOKMARK_NOTE_PREVIEW_MAX_CHARS: usize = 72;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    pub item_id: String,
    pub char_offset: u64,
    pub affinity: Affinity,
    pub note: String,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookmarkUpsertRequest {
    pub bookmark_id: String,
    pub item_id: String,
    pub char_offset: u64,
    pub affinity: Affinity,
    pub note: String,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub base_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookmarkTransition {
    pub bookmark: Bookmark,
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookmarkError {
    OffsetOutOfRange { offset: u64, char_count: u64 },
    VersionConflict { expected: u64, actual: u64 },
    IdCollision { bookmark_id: String },
}

impl std::fmt::Display for BookmarkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OffsetOutOfRange { offset, char_count } => {
                write!(
                    formatter,
                    "bookmark offset {offset} exceeds document length {char_count}"
                )
            }
            Self::VersionConflict { expected, actual } => {
                write!(
                    formatter,
                    "bookmark version conflict: expected {expected}, current {actual}"
                )
            }
            Self::IdCollision { bookmark_id } => {
                write!(
                    formatter,
                    "bookmark id already belongs to another offset: {bookmark_id}"
                )
            }
        }
    }
}

impl std::error::Error for BookmarkError {}

pub fn upsert_bookmark(
    document: &Document,
    bookmarks: &[Bookmark],
    request: BookmarkUpsertRequest,
) -> Result<BookmarkTransition, BookmarkError> {
    if request.char_offset > document.revision.char_count {
        return Err(BookmarkError::OffsetOutOfRange {
            offset: request.char_offset,
            char_count: document.revision.char_count,
        });
    }

    let at_offset = bookmarks.iter().find(|bookmark| {
        bookmark.item_id == request.item_id && bookmark.char_offset == request.char_offset
    });
    if let Some(existing) = at_offset {
        if request.base_version != existing.version {
            return Err(BookmarkError::VersionConflict {
                expected: request.base_version,
                actual: existing.version,
            });
        }
        return Ok(BookmarkTransition {
            bookmark: Bookmark {
                id: existing.id.clone(),
                item_id: existing.item_id.clone(),
                char_offset: existing.char_offset,
                affinity: request.affinity,
                note: resolved_note(
                    document,
                    request.char_offset,
                    request.affinity,
                    &request.note,
                ),
                created_at_unix_ms: existing.created_at_unix_ms,
                updated_at_unix_ms: request.updated_at_unix_ms,
                version: existing.version.saturating_add(1),
            },
            created: false,
        });
    }

    if request.base_version != 0 {
        return Err(BookmarkError::VersionConflict {
            expected: request.base_version,
            actual: 0,
        });
    }
    if bookmarks
        .iter()
        .any(|bookmark| bookmark.id == request.bookmark_id)
    {
        return Err(BookmarkError::IdCollision {
            bookmark_id: request.bookmark_id,
        });
    }

    Ok(BookmarkTransition {
        bookmark: Bookmark {
            id: request.bookmark_id,
            item_id: request.item_id,
            char_offset: request.char_offset,
            affinity: request.affinity,
            note: resolved_note(
                document,
                request.char_offset,
                request.affinity,
                &request.note,
            ),
            created_at_unix_ms: request.created_at_unix_ms,
            updated_at_unix_ms: request.updated_at_unix_ms,
            version: 1,
        },
        created: true,
    })
}

fn resolved_note(document: &Document, offset: u64, affinity: Affinity, note: &str) -> String {
    let note = note.trim();
    if !note.is_empty() {
        return note.to_string();
    }

    let projection = project_offset(document, offset, affinity);
    let passage = projection
        .block_id
        .as_deref()
        .and_then(|block_id| document.blocks.iter().find(|block| block.id == block_id))
        .map(|block| block.text.as_str())
        .unwrap_or_default();
    preview_note(passage)
}

fn preview_note(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars = normalized.chars().collect::<Vec<_>>();
    if chars.len() <= BOOKMARK_NOTE_PREVIEW_MAX_CHARS {
        return normalized;
    }

    let visible_limit = BOOKMARK_NOTE_PREVIEW_MAX_CHARS.saturating_sub(1);
    let word_end = chars[..visible_limit]
        .iter()
        .rposition(|character| character.is_whitespace())
        .filter(|end| *end > 0)
        .unwrap_or(visible_limit);
    let mut preview = chars[..word_end].iter().collect::<String>();
    preview.push('…');
    preview
}
