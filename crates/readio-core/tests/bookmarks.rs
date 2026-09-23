use readio_core::bookmarks::{Bookmark, BookmarkError, BookmarkUpsertRequest, upsert_bookmark};
use readio_core::documents::{HtmlParseOptions, parse_html};
use readio_core::position::{Affinity, project_offset};

fn request(char_offset: u64, note: &str, base_version: u64) -> BookmarkUpsertRequest {
    BookmarkUpsertRequest {
        bookmark_id: "bookmark-new".to_string(),
        item_id: "book-1".to_string(),
        char_offset,
        affinity: Affinity::Forward,
        note: note.to_string(),
        created_at_unix_ms: 10,
        updated_at_unix_ms: 10,
        base_version,
    }
}

#[test]
fn a_new_bookmark_uses_passage_text_when_the_note_is_empty() {
    let document = parse_html(
        "<p>Alpha beta gamma.</p><p>Delta.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("document");

    let transition =
        upsert_bookmark(&document, &[], request(6, "", 0)).expect("bookmark can be created");

    assert!(transition.created);
    assert_eq!(transition.bookmark.id, "bookmark-new");
    assert_eq!(transition.bookmark.char_offset, 6);
    assert_eq!(transition.bookmark.note, "Alpha beta gamma.");
    assert_eq!(transition.bookmark.version, 1);
}

#[test]
fn a_new_bookmark_uses_the_current_structural_block_for_its_default_note() {
    let first = "Model ecosystem context that must not leak into the next bookmark.";
    let second = "Section 3: open-source opportunities for Readio.";
    let document = parse_html(
        &format!("<p>{first}</p><p>{second}</p>"),
        &HtmlParseOptions::default(),
    )
    .expect("document");
    let second_block_start = first.chars().count() as u64 + 1;

    let transition = upsert_bookmark(&document, &[], request(second_block_start, "", 0))
        .expect("bookmark can be created");

    assert_eq!(transition.bookmark.note, second);
}

#[test]
fn marking_the_same_offset_updates_one_bookmark_instead_of_duplicating_it() {
    let document =
        parse_html("<p>Alpha beta.</p>", &HtmlParseOptions::default()).expect("document");
    let existing = Bookmark {
        id: "bookmark-existing".to_string(),
        item_id: "book-1".to_string(),
        char_offset: 6,
        affinity: Affinity::Backward,
        note: "old".to_string(),
        created_at_unix_ms: 5,
        updated_at_unix_ms: 5,
        version: 1,
    };

    let transition = upsert_bookmark(
        &document,
        &[existing],
        BookmarkUpsertRequest {
            updated_at_unix_ms: 20,
            ..request(6, "new note", 1)
        },
    )
    .expect("bookmark can be renamed");

    assert!(!transition.created);
    assert_eq!(transition.bookmark.id, "bookmark-existing");
    assert_eq!(transition.bookmark.created_at_unix_ms, 5);
    assert_eq!(transition.bookmark.updated_at_unix_ms, 20);
    assert_eq!(transition.bookmark.note, "new note");
    assert_eq!(transition.bookmark.version, 2);
}

#[test]
fn a_stale_bookmark_version_cannot_overwrite_the_current_note() {
    let document =
        parse_html("<p>Alpha beta.</p>", &HtmlParseOptions::default()).expect("document");
    let existing = Bookmark {
        id: "bookmark-existing".to_string(),
        item_id: "book-1".to_string(),
        char_offset: 6,
        affinity: Affinity::Forward,
        note: "current".to_string(),
        created_at_unix_ms: 5,
        updated_at_unix_ms: 10,
        version: 2,
    };

    let error = upsert_bookmark(&document, &[existing], request(6, "stale", 1))
        .expect_err("stale mutation must conflict");

    assert_eq!(
        error,
        BookmarkError::VersionConflict {
            expected: 1,
            actual: 2,
        }
    );
}

#[test]
fn bookmark_offset_projects_against_the_current_revision() {
    let original =
        parse_html("<p>Alpha beta.</p>", &HtmlParseOptions::default()).expect("original");
    let bookmark = upsert_bookmark(&original, &[], request(6, "beta", 0))
        .expect("bookmark")
        .bookmark;
    let current = parse_html(
        "<p>Intro</p><p>Alpha beta.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("current");

    let projection = project_offset(&current, bookmark.char_offset, bookmark.affinity);

    assert_eq!(bookmark.char_offset, 6);
    assert_eq!(projection.block_id, Some(current.blocks[1].id.clone()));
}
