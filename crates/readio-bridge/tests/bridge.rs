use readio_bridge::{
    build_support_bundle_json, builtin_sample_json, content_identity_json, create_playback_state_json,
    create_text_advance_state_json, create_text_locator_json, index_document_text_json,
    parse_html_json, parse_html_reader_json, parse_markdown_reader_json,
    parse_plain_text_reader_json, plan_forget_json, plan_import_json, plan_sync_push_json,
    project_offset_json, project_reading_progress_json, reduce_playback_json,
    reduce_text_advance_json, resolve_block_offset_json, resolve_highlight_json,
    resolve_reading_pace_json, resolve_reading_seek_json, restore_position_json,
    restore_text_locator_json, search_document_json, upsert_bookmark_json,
};
use serde_json::{Value, json};

#[test]
fn sample_json_bridge_returns_the_reserved_virtual_document() {
    let sample = builtin_sample_json("en-GB").expect("sample JSON");
    let value: Value = serde_json::from_str(&sample).expect("valid sample JSON");

    assert_eq!(value["item_id"], json!("readio:sample:v1"));
    assert_eq!(value["locale"], json!("en"));
    assert_eq!(value["title"], json!("The Shape of Attention"));
    assert_eq!(value["is_builtin_sample"], json!(true));
}

#[test]
fn diagnostics_json_bridge_redacts_book_content() {
    let bundle = build_support_bundle_json(
        &json!({
            "generated_at_unix_ms": 100,
            "platform": "web",
            "app_version": "0.2.0",
            "events": [{
                "timestamp_unix_ms": 90,
                "operation_id": "op-shared",
                "component": "web-reader",
                "category": "playback",
                "name": "clock_sample",
                "fields": {"media_position_ms": 250, "book_text": "Private words"}
            }]
        })
        .to_string(),
    )
    .expect("support bundle JSON");
    let value: Value = serde_json::from_str(&bundle).expect("valid bundle JSON");

    assert_eq!(value["events"][0]["fields"]["media_position_ms"], json!(250));
    assert!(value["events"][0]["fields"].get("book_text").is_none());
    assert!(!bundle.contains("Private words"));
}

#[test]
fn playback_json_bridge_runs_the_core_reducer() {
    let state = create_playback_state_json(2).expect("create state");

    let transition = reduce_playback_json(None, &state, r#"{"type":"play_requested"}"#)
        .expect("reduce playback");
    let value: Value = serde_json::from_str(&transition).expect("transition JSON");

    assert_eq!(value["state"]["status"], json!("loading"));
    assert_eq!(value["state"]["current_sentence"], json!(0));
    assert_eq!(
        value["effects"],
        json!([
            {
                "type": "fetch_audio",
                "sentence": 0,
                "generation": 1,
                "priority": "user",
            },
            {
                "type": "prefetch_audio",
                "sentences": [1],
            },
        ])
    );
}

#[test]
fn reading_json_bridge_returns_the_shared_mode_and_effort_projection() {
    let pace = resolve_reading_pace_json(
        r#"{
            "mode":"read_aloud",
            "base_text_characters_per_second":48,
            "effort":"xhigh",
            "multipliers":{"xhigh":1.25}
        }"#,
    )
    .expect("resolve reading pace");
    let value: Value = serde_json::from_str(&pace).expect("pace JSON");

    assert_eq!(value["mode"], json!("read_aloud"));
    assert_eq!(value["effort"], json!("xhigh"));
    assert_eq!(value["multiplier"], json!(1.25));
    assert_eq!(value["text_characters_per_second"], json!(60.0));
    assert_eq!(value["audio_playback_rate"], json!(1.25));
    assert_eq!(value["advances_automatically"], json!(true));
    assert_eq!(value["uses_audio"], json!(true));
}

#[test]
fn reading_json_bridge_projects_honest_progress_and_seek_coordinates() {
    let progress = project_reading_progress_json(
        r#"{
            "segment_character_counts":[2,8],
            "current_segment":1,
            "current_segment_progress_basis_points":5000,
            "timing":{"type":"unknown"}
        }"#,
    )
    .expect("project reading progress");
    let progress: Value = serde_json::from_str(&progress).expect("progress JSON");
    assert_eq!(progress["char_offset"], json!(7));
    assert_eq!(progress["progress_basis_points"], json!(6_363));
    assert_eq!(progress["remaining_time"]["kind"], json!("unknown"));
    assert_eq!(progress["remaining_time"]["seconds"], Value::Null);

    let cursor = resolve_reading_seek_json(
        r#"{"segment_character_counts":[1,9],"progress_basis_points":5000}"#,
    )
    .expect("resolve progress seek");
    let cursor: Value = serde_json::from_str(&cursor).expect("cursor JSON");
    assert_eq!(cursor["segment"], json!(1));
    assert_eq!(cursor["segment_progress_basis_points"], json!(3_333));
    assert_eq!(cursor["char_offset"], json!(5));
}

#[test]
fn text_advance_json_bridge_returns_the_shared_deadline_effect() {
    let state = create_text_advance_state_json(2).expect("create text state");
    let transition = reduce_text_advance_json(
        &state,
        r#"{
            "type":"start_requested",
            "character_count":80,
            "text_characters_per_second":40,
            "monotonic_ms":10000
        }"#,
    )
    .expect("reduce text advance");
    let value: Value = serde_json::from_str(&transition).expect("transition JSON");

    assert_eq!(value["state"]["status"], json!("running"));
    assert_eq!(value["state"]["deadline_monotonic_ms"], json!(12_000));
    assert_eq!(
        value["effects"],
        json!([{
            "type":"schedule_advance",
            "generation":1,
            "after_ms":2_000
        }])
    );
}

#[test]
fn storage_json_bridge_returns_the_core_plan() {
    let plan = plan_sync_push_json(
        r#"{"entities":[],"mutations":[]}"#,
        r#"{"device_id":"web-a","mutations":[{"mutation_id":"m-1","entity_kind":"setting","entity_id":"theme","operation":"upsert","base_version":0,"client_timestamp_unix_ms":1,"payload":{"value":"dark"}}]}"#,
        10,
        0,
    )
    .expect("storage plan");
    let value: Value = serde_json::from_str(&plan).expect("plan JSON");

    assert_eq!(value["cursor"], json!(1));
    assert_eq!(value["writes"][0]["expected_version"], json!(0));
    assert_eq!(value["results"][0]["status"], json!("applied"));
}

#[test]
fn import_json_bridge_returns_identity_plan_and_safe_forget_effects() {
    let identity = content_identity_json(b"same book").expect("content identity");
    let identity_value: Value = serde_json::from_str(&identity).expect("identity JSON");
    let request = json!({
        "content_identity": identity_value,
        "ownership": "move",
        "source_key": "file:///book.txt",
        "owned_storage_key": "content/book.txt",
        "existing": null,
    });
    let plan = plan_import_json(&request.to_string()).expect("import plan");
    let plan_value: Value = serde_json::from_str(&plan).expect("plan JSON");

    assert_eq!(plan_value["outcome"], json!("created"));
    assert_eq!(
        plan_value["effects"][0]["type"],
        json!("write_owned_content")
    );
    assert_eq!(
        plan_value["effects"][1]["type"],
        json!("delete_source_after_commit")
    );

    let forget = plan_forget_json(&plan_value["record"].to_string()).expect("forget plan");
    let forget_value: Value = serde_json::from_str(&forget).expect("forget JSON");
    assert_eq!(forget_value[0]["type"], json!("delete_owned_content"));
}

#[test]
fn document_json_bridge_returns_the_versioned_ir() {
    let document = parse_html_json("<h1>Title</h1><p>Body.</p>", None).expect("parse document");
    let value: Value = serde_json::from_str(&document).expect("document JSON");

    assert_eq!(value["ir_version"], json!(1));
    assert_eq!(value["blocks"][0]["kind"]["type"], json!("heading"));
    assert_eq!(value["blocks"][0]["kind"]["level"], json!(1));
    assert_eq!(value["blocks"][1]["kind"]["type"], json!("paragraph"));
}

#[test]
fn html_reader_json_bridge_returns_one_complete_gui_input() {
    let reader =
        parse_html_reader_json("<h2>Title</h2><p>One. Two.</p>", None).expect("parse reader");
    let value: Value = serde_json::from_str(&reader).expect("reader JSON");

    assert_eq!(value["ir_version"], json!(1));
    assert_eq!(
        value["document"]["blocks"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        value["text_index"]["sentences"].as_array().map(Vec::len),
        Some(3)
    );
    assert_eq!(value["navigation"][0]["title"], json!("Title"));
}

#[test]
fn plain_text_reader_json_bridge_returns_one_complete_gui_input() {
    let reader = parse_plain_text_reader_json("One. Two.\n\n三。", None).expect("parse reader");
    let value: Value = serde_json::from_str(&reader).expect("reader JSON");

    assert_eq!(
        value["document"]["blocks"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        value["text_index"]["sentences"].as_array().map(Vec::len),
        Some(3)
    );
    assert_eq!(value["chapters"][0]["sentence_range"], json!([0, 3]));
}

#[test]
fn markdown_reader_json_bridge_returns_semantic_gui_input() {
    let reader = parse_markdown_reader_json("# Title\n\nA **shared** paragraph.", None)
        .expect("parse Markdown reader");
    let value: Value = serde_json::from_str(&reader).expect("reader JSON");

    assert_eq!(
        value["document"]["blocks"][0]["kind"]["type"],
        json!("heading")
    );
    assert_eq!(
        value["document"]["blocks"][1]["inline_spans"][0]["range"],
        json!([2, 8])
    );
    assert_eq!(value["navigation"][0]["title"], json!("Title"));
}

#[test]
fn position_json_bridge_preserves_unicode_scalar_offsets() {
    let document = parse_html_json("<p>A🙂中</p><p>B</p>", None).expect("parse document");

    let projection = project_offset_json(&document, r#"{"char_offset":2,"affinity":"forward"}"#)
        .expect("project position");
    let value: Value = serde_json::from_str(&projection).expect("projection JSON");

    assert_eq!(value["char_offset"], json!(2));
    assert_eq!(value["offset_in_block"], json!(2));
    assert_eq!(value["progress_basis_points"], json!(4_000));
}

#[test]
fn position_json_bridge_resolves_a_block_anchor() {
    let document = parse_html_json("<p>A🙂</p><p>中B</p>", None).expect("document");
    let document_value: Value = serde_json::from_str(&document).expect("document JSON");
    let request = json!({
        "block_id": document_value["blocks"][1]["id"],
        "offset_in_block": 1,
        "affinity": "forward",
    });

    let projection =
        resolve_block_offset_json(&document, &request.to_string()).expect("resolve anchor");
    let value: Value = serde_json::from_str(&projection).expect("projection JSON");

    assert_eq!(value["char_offset"], json!(4));
    assert_eq!(value["offset_in_block"], json!(1));
}

#[test]
fn position_json_bridge_restores_against_the_current_revision() {
    let document = parse_html_json("<p>A🙂中</p><p>B</p>", None).expect("parse document");
    let position = r#"{
        "item_id":"book-1",
        "document_revision":"stale-revision",
        "char_offset":999,
        "affinity":"backward",
        "observed_at_unix_ms":10,
        "session_id":"session-1",
        "mutation_id":"mutation-1",
        "base_version":1
    }"#;

    let restored = restore_position_json(&document, position).expect("restore position");
    let value: Value = serde_json::from_str(&restored).expect("restored JSON");

    assert_eq!(value["char_offset"], json!(5));
    assert_eq!(value["revision_changed"], json!(true));
    assert_eq!(value["projection"]["progress_basis_points"], json!(10_000));
}

#[test]
fn text_locator_and_highlight_json_are_portable_bridge_contracts() {
    let document = parse_html_json("<p>First sentence.</p><p>Second bright sentence.</p>", None)
        .expect("document");
    let value: Value = serde_json::from_str(&document).expect("document JSON");
    let revision = value["revision"]["revision_digest"]
        .as_str()
        .expect("revision");

    let locator = create_text_locator_json(&document, r#"{"char_offset":23,"affinity":"forward"}"#)
        .expect("locator");
    let locator_value: Value = serde_json::from_str(&locator).expect("locator JSON");
    assert_eq!(locator_value["version"], json!(1));
    assert_eq!(locator_value["offset_in_block"], json!(7));

    let restored = restore_text_locator_json(&document, &locator).expect("restore locator");
    let restored_value: Value = serde_json::from_str(&restored).expect("restored JSON");
    assert_eq!(restored_value["kind"], json!("exact_revision"));

    let index = index_document_text_json(&document).expect("text index");
    let index_value: Value = serde_json::from_str(&index).expect("index JSON");
    assert_eq!(index_value["sentences"][1]["range"], json!([0, 23]));

    let cursor = json!({
        "document_revision": revision,
        "sentence_index": 1,
        "word_index": 1,
    });
    let target =
        resolve_highlight_json(&document, &index, &cursor.to_string()).expect("highlight target");
    let target_value: Value = serde_json::from_str(&target).expect("target JSON");
    assert_eq!(target_value["sentence_range"], json!([0, 23]));
    assert_eq!(target_value["word_range"], json!([7, 13]));
}

#[test]
fn search_json_bridge_returns_original_scalar_ranges() {
    let document = parse_html_json("<p>ＣＩＴＹ and city</p>", None).expect("document");
    let document_value: Value = serde_json::from_str(&document).expect("document JSON");
    let request = json!({
        "document_revision": document_value["revision"]["revision_digest"],
        "query": "city",
        "cursor": null,
        "limit": 10,
    });

    let result = search_document_json(&document, &request.to_string()).expect("search");
    let value: Value = serde_json::from_str(&result).expect("search JSON");

    assert_eq!(value["total_occurrences"], json!(2));
    assert_eq!(value["hits"][0]["char_range"], json!([0, 4]));
}

#[test]
fn bookmark_json_bridge_applies_same_offset_semantics_in_core() {
    let document = parse_html_json("<p>Alpha beta.</p>", None).expect("document");
    let request = json!({
        "bookmark_id": "bookmark-1",
        "item_id": "book-1",
        "char_offset": 6,
        "affinity": "forward",
        "note": "",
        "created_at_unix_ms": 10,
        "updated_at_unix_ms": 10,
        "base_version": 0,
    });

    let transition =
        upsert_bookmark_json(&document, "[]", &request.to_string()).expect("bookmark transition");
    let value: Value = serde_json::from_str(&transition).expect("bookmark JSON");

    assert_eq!(value["created"], json!(true));
    assert_eq!(value["bookmark"]["char_offset"], json!(6));
    assert_eq!(value["bookmark"]["note"], json!("Alpha beta."));
}
