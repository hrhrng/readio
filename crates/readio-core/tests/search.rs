use readio_core::documents::{HtmlParseOptions, parse_html};
use readio_core::search::{SearchError, SearchRequest, search_document};

fn request(document_revision: &str, query: &str, limit: u16) -> SearchRequest {
    SearchRequest {
        document_revision: document_revision.to_string(),
        query: query.to_string(),
        cursor: None,
        limit,
    }
}

#[test]
fn search_folds_case_and_full_width_while_returning_scalar_ranges() {
    let document = parse_html(
        "<p>ＣＩＴＹ of glass</p><p>the City of Glass</p>",
        &HtmlParseOptions::default(),
    )
    .expect("valid document");

    let result = search_document(
        &document,
        request(&document.revision.revision_digest, "city", 10),
    )
    .expect("search succeeds");

    assert_eq!(result.folded_query, "city");
    assert_eq!(result.total_occurrences, 2);
    assert_eq!(result.total_blocks, 2);
    assert_eq!(result.hits[0].char_range, [0, 4]);
    assert_eq!(result.hits[0].snippet_match_range, [0, 4]);
    assert_eq!(result.hits[0].snippet, "ＣＩＴＹ of glass");
    assert!(result.next_cursor.is_none());
}

#[test]
fn search_counts_all_occurrences_but_cursor_limits_the_returned_page() {
    let document = parse_html(
        "<p>记忆记忆记忆</p><p>记忆</p>",
        &HtmlParseOptions::default(),
    )
    .expect("valid document");

    let first = search_document(
        &document,
        request(&document.revision.revision_digest, "记忆", 2),
    )
    .expect("first page");
    assert_eq!(first.total_occurrences, 4);
    assert_eq!(first.hits.len(), 2);
    assert_eq!(first.next_cursor.as_deref(), Some("2"));

    let second = search_document(
        &document,
        SearchRequest {
            cursor: first.next_cursor,
            ..request(&document.revision.revision_digest, "记忆", 2)
        },
    )
    .expect("second page");
    assert_eq!(second.total_occurrences, 4);
    assert_eq!(second.hits.len(), 2);
    assert!(second.next_cursor.is_none());
    assert_ne!(first.hits[0].hit_id, second.hits[0].hit_id);
}

#[test]
fn zero_limit_reports_counts_without_a_non_advancing_cursor() {
    let document =
        parse_html("<p>alpha alpha</p>", &HtmlParseOptions::default()).expect("document");

    let result = search_document(
        &document,
        request(&document.revision.revision_digest, "alpha", 0),
    )
    .expect("search succeeds");

    assert_eq!(result.total_occurrences, 2);
    assert!(result.hits.is_empty());
    assert!(result.next_cursor.is_none());
}

#[test]
fn search_rejects_a_stale_document_revision_instead_of_returning_zero_hits() {
    let document = parse_html("<p>needle</p>", &HtmlParseOptions::default()).expect("document");

    let error = search_document(&document, request("stale", "needle", 10))
        .expect_err("stale revision must be visible");

    assert_eq!(
        error,
        SearchError::StaleRevision {
            expected: "stale".to_string(),
            actual: document.revision.revision_digest,
        }
    );
}

#[test]
fn search_does_not_loop_on_overlapping_matches() {
    let document = parse_html("<p>aaaa</p>", &HtmlParseOptions::default()).expect("document");

    let result = search_document(
        &document,
        request(&document.revision.revision_digest, "aa", 10),
    )
    .expect("search succeeds");

    assert_eq!(result.total_occurrences, 2);
    assert_eq!(result.hits[0].char_range, [0, 2]);
    assert_eq!(result.hits[1].char_range, [2, 4]);
}
