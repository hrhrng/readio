use readio_core::documents::{HtmlParseOptions, parse_html};
use readio_core::position::{
    Affinity, LocatorResolutionKind, ReadingPosition, create_text_locator, project_offset,
    resolve_block_offset, restore_position, restore_text_locator,
};

#[test]
fn document_revision_counts_unicode_scalars_in_canonical_text() {
    let document =
        parse_html("<p>A🙂中</p><p>B</p>", &HtmlParseOptions::default()).expect("valid document");

    assert_eq!(document.canonical_text(), "A🙂中\nB");
    assert_eq!(document.revision.char_count, 5);
    assert_eq!(
        document.revision.content_identity.algorithm,
        "blake3-prefix-v1"
    );
    assert!(!document.revision.revision_digest.is_empty());
}

#[test]
fn projection_uses_scalar_offsets_and_clamps_to_document_end() {
    let document =
        parse_html("<p>A🙂中</p><p>B</p>", &HtmlParseOptions::default()).expect("valid document");

    let projection = project_offset(&document, 2, Affinity::Forward);
    assert_eq!(projection.char_offset, 2);
    assert_eq!(projection.block_id, Some(document.blocks[0].id.clone()));
    assert_eq!(projection.offset_in_block, 2);

    let end = project_offset(&document, 999, Affinity::Backward);
    assert_eq!(end.char_offset, 5);
    assert_eq!(end.block_id, Some(document.blocks[1].id.clone()));
    assert_eq!(end.offset_in_block, 1);
    assert_eq!(end.progress_basis_points, 10_000);
}

#[test]
fn block_offsets_resolve_to_the_authoritative_scalar_coordinate() {
    let document =
        parse_html("<p>A🙂</p><p>中B</p>", &HtmlParseOptions::default()).expect("document");

    let projection = resolve_block_offset(&document, &document.blocks[1].id, 1, Affinity::Forward)
        .expect("known block resolves");

    assert_eq!(projection.char_offset, 4);
    assert_eq!(projection.block_id, Some(document.blocks[1].id.clone()));
    assert_eq!(projection.offset_in_block, 1);
}

#[test]
fn restore_keeps_authoritative_offset_when_revision_changes() {
    let original =
        parse_html("<p>Hello 🙂</p>", &HtmlParseOptions::default()).expect("original document");
    let edited = parse_html("<p>Intro</p><p>Hello 🙂</p>", &HtmlParseOptions::default())
        .expect("edited document");
    let stored = ReadingPosition {
        item_id: "book-1".to_string(),
        document_revision: original.revision.revision_digest.clone(),
        char_offset: 6,
        affinity: Affinity::Forward,
        observed_at_unix_ms: 10,
        session_id: "session-1".to_string(),
        mutation_id: "mutation-1".to_string(),
        base_version: 1,
        locator: None,
    };

    let restored = restore_position(&stored, &edited);

    assert!(restored.revision_changed);
    assert_eq!(restored.char_offset, 6);
    assert_eq!(restored.affinity, Affinity::Forward);
    assert_eq!(
        restored.projection.block_id,
        Some(edited.blocks[1].id.clone())
    );
}

#[test]
fn persisted_locator_moves_progress_with_its_stable_block() {
    let original = parse_html(
        "<p>Opening</p><p>Hello brave world.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("original");
    let target = resolve_block_offset(&original, &original.blocks[1].id, 6, Affinity::Forward)
        .expect("target");
    let edited = parse_html(
        "<p>A newly inserted introduction</p><p>Opening</p><p>Hello brave world.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("edited");
    let stored = ReadingPosition {
        item_id: "book-1".to_string(),
        document_revision: original.revision.revision_digest.clone(),
        char_offset: target.char_offset,
        affinity: Affinity::Forward,
        observed_at_unix_ms: 10,
        session_id: "session-1".to_string(),
        mutation_id: "mutation-1".to_string(),
        base_version: 1,
        locator: Some(create_text_locator(
            &original,
            target.char_offset,
            Affinity::Forward,
        )),
    };

    let restored = restore_position(&stored, &edited);

    assert_eq!(
        restored.locator_resolution,
        Some(LocatorResolutionKind::StableBlock)
    );
    assert_eq!(
        restored.projection.block_id,
        Some(edited.blocks[2].id.clone())
    );
    assert_eq!(restored.projection.offset_in_block, 6);
    assert_eq!(restored.char_offset, 44);
}

#[test]
fn locator_uses_the_stable_block_when_content_is_inserted_before_it() {
    let original = parse_html(
        "<p>Opening</p><p>Hello brave world.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("original document");
    let target = resolve_block_offset(&original, &original.blocks[1].id, 6, Affinity::Forward)
        .expect("target projection");
    let locator = create_text_locator(&original, target.char_offset, Affinity::Forward);
    let edited = parse_html(
        "<p>A newly inserted introduction</p><p>Opening</p><p>Hello brave world.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("edited document");

    let restored = restore_text_locator(&locator, &edited).expect("supported locator");

    assert_eq!(restored.kind, LocatorResolutionKind::StableBlock);
    assert_eq!(
        restored.projection.block_id,
        Some(edited.blocks[2].id.clone())
    );
    assert_eq!(restored.projection.offset_in_block, 6);
}

#[test]
fn locator_uses_a_unique_text_quote_when_the_target_block_id_changes() {
    let original = parse_html(
        "<p>Opening</p><p>A calm city wakes at dawn.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("original document");
    let local_offset = "A calm ".chars().count() as u64;
    let target = resolve_block_offset(
        &original,
        &original.blocks[1].id,
        local_offset,
        Affinity::Forward,
    )
    .expect("target projection");
    let locator = create_text_locator(&original, target.char_offset, Affinity::Forward);
    let edited = parse_html(
        "<p>Opening</p><p>A very calm city wakes at dawn.</p>",
        &HtmlParseOptions::default(),
    )
    .expect("edited document");

    let restored = restore_text_locator(&locator, &edited).expect("supported locator");

    assert_eq!(restored.kind, LocatorResolutionKind::TextQuote);
    assert_eq!(
        restored.projection.block_id,
        Some(edited.blocks[1].id.clone())
    );
    assert_eq!(restored.projection.offset_in_block, 12);
}

#[test]
fn an_ambiguous_text_quote_falls_back_instead_of_guessing() {
    let original = parse_html(
        "<p>Before</p><p>target phrase appears here</p>",
        &HtmlParseOptions::default(),
    )
    .expect("original document");
    let target = resolve_block_offset(&original, &original.blocks[1].id, 0, Affinity::Forward)
        .expect("target projection");
    let mut locator = create_text_locator(&original, target.char_offset, Affinity::Forward);
    locator.document_revision = "stale-revision".to_string();
    locator.block_id = "missing-block".to_string();
    locator.quote.prefix.clear();
    locator.quote.suffix.clear();
    let repeated = parse_html(
        "<p>target phrase appears here</p><p>target phrase appears here</p>",
        &HtmlParseOptions::default(),
    )
    .expect("repeated document");

    let restored = restore_text_locator(&locator, &repeated).expect("supported locator");

    assert_eq!(restored.kind, LocatorResolutionKind::FallbackOffset);
    assert_eq!(restored.projection.char_offset, target.char_offset);
}
