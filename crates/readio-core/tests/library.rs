use readio_core::library::{
    LibraryItem, LibraryItemCategory, LibraryItemType, LibraryQuery, LibrarySortBy, SortOrder,
    query_library,
};

fn item(
    id: &str,
    title: &str,
    item_type: LibraryItemType,
    progress: u8,
    date: &str,
) -> LibraryItem {
    LibraryItem {
        id: id.to_string(),
        title: title.to_string(),
        author: None,
        item_type,
        progress,
        date: date.to_string(),
        category: LibraryItemCategory::Imported,
        folder_id: "f1".to_string(),
        source: format!("upload:{id}"),
        file_path: Some(format!("local:{id}")),
        cover_image: None,
        voice: None,
        speed: None,
        current_chapter: None,
        content_format: None,
        import_record: None,
    }
}

#[test]
fn library_query_filters_case_and_full_width_folded_titles() {
    let items = vec![
        item("a", "Ａlpha Notes", LibraryItemType::Txt, 0, "2026-01-01"),
        item("b", "Beta", LibraryItemType::Pdf, 0, "2026-01-02"),
    ];

    let result = query_library(
        &items,
        &LibraryQuery {
            search: Some("alpha".to_string()),
            ..LibraryQuery::default()
        },
    )
    .expect("query library");

    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].id, "a");
}

#[test]
fn library_query_combines_type_progress_and_folder_filters() {
    let mut matching = item("a", "A", LibraryItemType::Epub, 60, "2026-01-01");
    matching.folder_id = "reading".to_string();
    let mut wrong_folder = item("b", "B", LibraryItemType::Epub, 60, "2026-01-02");
    wrong_folder.folder_id = "archive".to_string();
    let wrong_type = item("c", "C", LibraryItemType::Pdf, 60, "2026-01-03");

    let result = query_library(
        &[matching, wrong_folder, wrong_type],
        &LibraryQuery {
            folder_id: Some("reading".to_string()),
            item_type: Some(LibraryItemType::Epub),
            progress_min: Some(1),
            progress_max: Some(99),
            ..LibraryQuery::default()
        },
    )
    .expect("query library");

    assert_eq!(
        result
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a"]
    );
}

#[test]
fn library_query_sorts_deterministically_before_paginating() {
    let items = vec![
        item("c", "Gamma", LibraryItemType::Txt, 0, "2026-01-01"),
        item("a", "Alpha", LibraryItemType::Txt, 0, "2026-01-02"),
        item("b", "Beta", LibraryItemType::Txt, 0, "2026-01-03"),
    ];

    let result = query_library(
        &items,
        &LibraryQuery {
            page: 2,
            page_size: 1,
            sort_by: LibrarySortBy::Title,
            sort_order: SortOrder::Ascending,
            ..LibraryQuery::default()
        },
    )
    .expect("query library");

    assert_eq!(result.total, 3);
    assert_eq!(result.total_pages, 3);
    assert_eq!(result.items[0].id, "b");
}

#[test]
fn library_query_rejects_zero_based_or_unbounded_pages() {
    let result = query_library(
        &[],
        &LibraryQuery {
            page: 0,
            page_size: 101,
            ..LibraryQuery::default()
        },
    );

    assert!(result.is_err());
}

#[test]
fn library_query_preserves_optional_publication_author_metadata() {
    let mut encoded = serde_json::to_value(item(
        "epub-1",
        "Portable EPUB",
        LibraryItemType::Epub,
        0,
        "2026-08-07",
    ))
    .expect("library item JSON");
    encoded["author"] = serde_json::json!("Rust SDK");
    let decoded: LibraryItem = serde_json::from_value(encoded).expect("library item");

    let result = query_library(&[decoded], &LibraryQuery::default()).expect("query library");
    let projected = serde_json::to_value(&result.items[0]).expect("summary JSON");

    assert_eq!(projected["author"], serde_json::json!("Rust SDK"));
}
