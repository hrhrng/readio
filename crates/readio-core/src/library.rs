use std::cmp::Ordering;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::imports::ImportRecord;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryItemType {
    Web,
    Txt,
    Pdf,
    Epub,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryItemCategory {
    Imported,
    Podcasts,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LibraryItem {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(rename = "type")]
    pub item_type: LibraryItemType,
    pub progress: u8,
    pub date: String,
    pub category: LibraryItemCategory,
    pub folder_id: String,
    pub source: String,
    pub file_path: Option<String>,
    pub cover_image: Option<String>,
    pub voice: Option<String>,
    pub speed: Option<f64>,
    pub current_chapter: Option<String>,
    pub content_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import_record: Option<ImportRecord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LibraryItemSummary {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(rename = "type")]
    pub item_type: LibraryItemType,
    pub progress: u8,
    pub date: String,
    pub category: LibraryItemCategory,
    pub folder_id: String,
    pub source: String,
    pub file_path: Option<String>,
    pub cover_image: Option<String>,
    pub voice: Option<String>,
    pub speed: Option<f64>,
    pub content_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import_record: Option<ImportRecord>,
}

impl From<&LibraryItem> for LibraryItemSummary {
    fn from(item: &LibraryItem) -> Self {
        Self {
            id: item.id.clone(),
            title: item.title.clone(),
            author: item.author.clone(),
            item_type: item.item_type,
            progress: item.progress,
            date: item.date.clone(),
            category: item.category,
            folder_id: item.folder_id.clone(),
            source: item.source.clone(),
            file_path: item.file_path.clone(),
            cover_image: item.cover_image.clone(),
            voice: item.voice.clone(),
            speed: item.speed,
            content_format: item.content_format.clone(),
            import_record: item.import_record.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySortBy {
    #[default]
    CreatedAt,
    Title,
    Progress,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Ascending,
    #[default]
    Descending,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct LibraryQuery {
    pub category: Option<LibraryItemCategory>,
    pub search: Option<String>,
    pub folder_id: Option<String>,
    #[serde(rename = "type")]
    pub item_type: Option<LibraryItemType>,
    pub page: u32,
    pub page_size: u16,
    pub sort_by: LibrarySortBy,
    pub sort_order: SortOrder,
    pub progress_min: Option<u8>,
    pub progress_max: Option<u8>,
}

impl Default for LibraryQuery {
    fn default() -> Self {
        Self {
            category: None,
            search: None,
            folder_id: None,
            item_type: None,
            page: 1,
            page_size: 20,
            sort_by: LibrarySortBy::CreatedAt,
            sort_order: SortOrder::Descending,
            progress_min: None,
            progress_max: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LibraryPage {
    pub items: Vec<LibraryItemSummary>,
    pub total: u64,
    pub page: u32,
    pub page_size: u16,
    pub total_pages: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryError {
    InvalidPagination,
    InvalidProgressRange,
}

impl Display for LibraryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPagination => {
                formatter.write_str("library page must be positive and page size must be 1..=100")
            }
            Self::InvalidProgressRange => {
                formatter.write_str("library progress range must be ordered within 0..=100")
            }
        }
    }
}

impl std::error::Error for LibraryError {}

pub fn query_library(
    items: &[LibraryItem],
    query: &LibraryQuery,
) -> Result<LibraryPage, LibraryError> {
    if query.page == 0 || query.page_size == 0 || query.page_size > 100 {
        return Err(LibraryError::InvalidPagination);
    }
    if query
        .progress_min
        .zip(query.progress_max)
        .is_some_and(|(min, max)| min > max)
        || query.progress_min.is_some_and(|value| value > 100)
        || query.progress_max.is_some_and(|value| value > 100)
    {
        return Err(LibraryError::InvalidProgressRange);
    }

    let folded_search = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(fold);
    let mut filtered = items
        .iter()
        .filter(|item| query.category.is_none_or(|value| item.category == value))
        .filter(|item| query.item_type.is_none_or(|value| item.item_type == value))
        .filter(|item| {
            query
                .folder_id
                .as_deref()
                .is_none_or(|value| item.folder_id == value)
        })
        .filter(|item| {
            query
                .progress_min
                .is_none_or(|value| item.progress >= value)
        })
        .filter(|item| {
            query
                .progress_max
                .is_none_or(|value| item.progress <= value)
        })
        .filter(|item| {
            folded_search
                .as_deref()
                .is_none_or(|value| fold(&item.title).contains(value))
        })
        .collect::<Vec<_>>();

    filtered.sort_by(|left, right| {
        let primary = match query.sort_by {
            LibrarySortBy::CreatedAt => left.date.cmp(&right.date),
            LibrarySortBy::Title => fold(&left.title).cmp(&fold(&right.title)),
            LibrarySortBy::Progress => left.progress.cmp(&right.progress),
        };
        let ordered = primary.then_with(|| left.id.cmp(&right.id));
        match query.sort_order {
            SortOrder::Ascending => ordered,
            SortOrder::Descending => reverse(ordered),
        }
    });

    let total = filtered.len() as u64;
    let page_size = usize::from(query.page_size);
    let start = usize::try_from(query.page.saturating_sub(1))
        .unwrap_or(usize::MAX)
        .saturating_mul(page_size)
        .min(filtered.len());
    let end = start.saturating_add(page_size).min(filtered.len());
    let total_pages = total.div_ceil(u64::from(query.page_size));

    Ok(LibraryPage {
        items: filtered[start..end]
            .iter()
            .map(|item| LibraryItemSummary::from(*item))
            .collect(),
        total,
        page: query.page,
        page_size: query.page_size,
        total_pages,
    })
}

fn reverse(ordering: Ordering) -> Ordering {
    match ordering {
        Ordering::Less => Ordering::Greater,
        Ordering::Equal => Ordering::Equal,
        Ordering::Greater => Ordering::Less,
    }
}

fn fold(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| {
            let plain = match character {
                '\u{ff01}'..='\u{ff5e}' => {
                    char::from_u32(character as u32 - 0xFEE0).unwrap_or(character)
                }
                '\u{3000}' => ' ',
                other => other,
            };
            plain.to_lowercase()
        })
        .collect()
}
