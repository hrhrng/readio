use readio_core::documents::{HtmlParseOptions, parse_html};
use readio_core::highlight::{HighlightCursor, HighlightError, index_document, resolve_highlight};

#[test]
fn document_index_uses_scalar_sentence_and_word_ranges() {
    let document = parse_html(
        "<p>Hello world. 你好世界！ Next🙂 word?</p>",
        &HtmlParseOptions::default(),
    )
    .expect("document");

    let index = index_document(&document);

    assert_eq!(
        index
            .sentences
            .iter()
            .map(|sentence| (sentence.text.as_str(), sentence.range, sentence.char_range,))
            .collect::<Vec<_>>(),
        vec![
            ("Hello world.", [0, 12], [0, 12]),
            ("你好世界！", [13, 18], [13, 18]),
            ("Next🙂 word?", [19, 30], [19, 30]),
        ]
    );
    assert_eq!(
        index.sentences[1]
            .words
            .iter()
            .map(|word| (word.text.as_str(), word.range))
            .collect::<Vec<_>>(),
        vec![
            ("你", [13, 14]),
            ("好", [14, 15]),
            ("世", [15, 16]),
            ("界", [16, 17])
        ]
    );
    assert_eq!(
        index.sentences[2]
            .words
            .iter()
            .map(|word| (word.text.as_str(), word.range))
            .collect::<Vec<_>>(),
        vec![("Next", [19, 23]), ("word", [25, 29])]
    );
}

#[test]
fn sentence_identity_survives_an_unrelated_block_insert() {
    let original = parse_html(
        "<p>Opening.</p><p>The stable sentence. Another sentence.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("original");
    let edited = parse_html(
        "<p>Inserted.</p><p>Opening.</p><p>The stable sentence. Another sentence.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("edited");

    let original_id = &index_document(&original).sentences[1].id;
    let edited_id = &index_document(&edited).sentences[2].id;

    assert_eq!(original_id, edited_id);
}

#[test]
fn highlight_maps_one_playback_cursor_to_renderer_local_ranges() {
    let document = parse_html(
        "<p>First sentence.</p><p>Second bright sentence.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("document");
    let index = index_document(&document);

    let target = resolve_highlight(
        &document,
        &index,
        HighlightCursor {
            document_revision: document.revision.revision_digest.clone(),
            sentence_index: 1,
            word_index: Some(1),
            word_progress_basis_points: None,
        },
    )
    .expect("valid cursor");

    assert_eq!(target.block_id, document.blocks[1].id);
    assert_eq!(target.sentence_range, [0, 23]);
    assert_eq!(target.word_range, Some([7, 13]));
    assert_eq!(target.sentence_char_range, [16, 39]);
    assert_eq!(target.word_char_range, Some([23, 29]));
}

#[test]
fn stale_highlight_cursors_are_rejected_instead_of_painting_the_wrong_text() {
    let document =
        parse_html("<p>Only sentence.</p>", &HtmlParseOptions::default()).expect("document");
    let index = index_document(&document);

    let error = resolve_highlight(
        &document,
        &index,
        HighlightCursor {
            document_revision: "stale".to_string(),
            sentence_index: 0,
            word_index: None,
            word_progress_basis_points: None,
        },
    )
    .expect_err("stale cursor");

    assert!(matches!(error, HighlightError::StaleRevision { .. }));
}

#[test]
fn core_maps_playback_progress_to_its_own_word_boundaries() {
    let document =
        parse_html("<p>one two three four.</p>", &HtmlParseOptions::default()).expect("document");
    let index = index_document(&document);

    let target = resolve_highlight(
        &document,
        &index,
        HighlightCursor {
            document_revision: document.revision.revision_digest.clone(),
            sentence_index: 0,
            word_index: None,
            word_progress_basis_points: Some(5_000),
        },
    )
    .expect("progress cursor");

    assert_eq!(target.word_range, Some([8, 13]));
}
