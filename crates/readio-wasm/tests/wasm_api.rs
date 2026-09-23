use readio_wasm::{
    ReadioReplicaWasm, ReadioSyncStateWasm, build_support_bundle, builtin_sample, content_identity,
    create_playback_state, create_text_advance_state, create_text_locator, extract_epub_asset,
    index_document_text, parse_epub, parse_html_document, parse_html_reader, parse_markdown_reader,
    parse_pdf, parse_plain_text_reader, plan_forget, plan_import, plan_sync_push, project_offset,
    project_reading_progress, reduce_playback, reduce_text_advance, resolve_block_offset,
    resolve_highlight, resolve_reading_pace, resolve_reading_seek, restore_position,
    restore_text_locator, search_document, upsert_bookmark,
};
use serde_json::{Value, json};
use std::io::{Cursor, Write};

const EPUB_FIGURE_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nwasm-figure";

#[test]
fn wasm_sample_api_exposes_the_same_virtual_document() {
    let sample = builtin_sample("zh-CN".to_string()).expect("sample");
    let value: Value = serde_json::from_str(&sample).expect("sample JSON");

    assert_eq!(value["item_id"], json!("readio:sample:v1"));
    assert_eq!(value["locale"], json!("zh"));
    assert_eq!(value["is_builtin_sample"], json!(true));
}

#[test]
fn wasm_diagnostics_api_exposes_only_the_redacted_bundle() {
    let bundle = build_support_bundle(
        json!({
            "generated_at_unix_ms": 100,
            "platform": "web",
            "app_version": "0.2.0",
            "events": [{
                "timestamp_unix_ms": 90,
                "operation_id": "op-shared",
                "component": "web-reader",
                "category": "tts",
                "name": "provider_completed",
                "fields": {"provider": "cloud", "authorization": "Bearer secret"}
            }]
        })
        .to_string(),
    )
    .expect("support bundle");
    let value: Value = serde_json::from_str(&bundle).expect("bundle JSON");

    assert_eq!(value["events"][0]["fields"]["provider"], json!("cloud"));
    assert!(!bundle.contains("Bearer secret"));
}

#[test]
fn wasm_playback_api_exposes_the_shared_reducer_without_platform_logic() {
    let state = create_playback_state(2).expect("create state");
    let transition =
        reduce_playback(None, state, r#"{"type":"play_requested"}"#.to_string()).expect("reduce");
    let value: Value = serde_json::from_str(&transition).expect("transition JSON");

    assert_eq!(value["state"]["status"], json!("loading"));
    assert_eq!(value["effects"][0]["type"], json!("fetch_audio"));
}

#[test]
fn wasm_reading_api_exposes_the_shared_mode_and_effort_projection() {
    let pace = resolve_reading_pace(
        r#"{"mode":"auto","base_text_characters_per_second":50,"effort":"medium"}"#.to_string(),
    )
    .expect("reading pace");
    let value: Value = serde_json::from_str(&pace).expect("pace JSON");

    assert_eq!(value["mode"], json!("auto"));
    assert_eq!(value["multiplier"], json!(1.5));
    assert_eq!(value["text_characters_per_second"], json!(75.0));
    assert_eq!(value["audio_playback_rate"], json!(1.5));
}

#[test]
fn wasm_reading_api_exposes_honest_progress_and_remaining_time() {
    let projection = project_reading_progress(
        r#"{"segment_character_counts":[100,200,100],"current_segment":1,"current_segment_progress_basis_points":0,"timing":{"type":"observed_audio","playback_rate":2,"current_position_ms":500,"durations":[{"segment":0,"duration_ms":1000},{"segment":1,"duration_ms":2000}]}}"#.to_string(),
    )
    .expect("reading projection");
    let projection: Value = serde_json::from_str(&projection).expect("projection JSON");
    assert_eq!(projection["char_offset"], json!(151));
    assert_eq!(
        projection["remaining_time"]["kind"],
        json!("observed_audio")
    );
    assert_eq!(projection["remaining_time"]["seconds"], json!(2));

    let cursor = resolve_reading_seek(
        r#"{"segment_character_counts":[1,9],"progress_basis_points":5000}"#.to_string(),
    )
    .expect("reading seek");
    let cursor: Value = serde_json::from_str(&cursor).expect("cursor JSON");
    assert_eq!(cursor["segment"], json!(1));
    assert_eq!(cursor["char_offset"], json!(5));
}

#[test]
fn wasm_reading_api_exposes_the_shared_text_advance_reducer() {
    let state = create_text_advance_state(2).expect("text state");
    let transition = reduce_text_advance(
        state,
        r#"{"type":"start_requested","character_count":80,"text_characters_per_second":40,"monotonic_ms":10000}"#.to_string(),
    )
    .expect("text transition");
    let value: Value = serde_json::from_str(&transition).expect("transition JSON");

    assert_eq!(value["state"]["generation"], json!(1));
    assert_eq!(value["effects"][0]["after_ms"], json!(2_000));
}

#[test]
fn wasm_storage_api_exposes_the_portable_storage_plan() {
    let plan = plan_sync_push(
        r#"{"entities":[],"mutations":[]}"#.to_string(),
        r#"{"device_id":"worker-a","mutations":[]}"#.to_string(),
        10,
        4,
    )
    .expect("storage plan");
    let value: Value = serde_json::from_str(&plan).expect("plan JSON");

    assert_eq!(value["cursor"], json!(4));
    assert_eq!(value["writes"], json!([]));
}

#[test]
fn wasm_import_api_exposes_the_same_ownership_and_cleanup_plan() {
    let identity = content_identity(b"same book").expect("content identity");
    let identity: Value = serde_json::from_str(&identity).expect("identity JSON");
    let plan = plan_import(
        json!({
            "content_identity": identity,
            "ownership": "copy",
            "source_key": "opfs://incoming/book.txt",
            "owned_storage_key": "opfs://library/book.txt",
            "existing": null,
        })
        .to_string(),
    )
    .expect("import plan");
    let plan: Value = serde_json::from_str(&plan).expect("plan JSON");
    assert_eq!(plan["effects"][0]["type"], json!("write_owned_content"));

    let forget = plan_forget(plan["record"].to_string()).expect("forget plan");
    let forget: Value = serde_json::from_str(&forget).expect("forget JSON");
    assert_eq!(forget[0]["type"], json!("delete_owned_content"));
}

#[test]
fn wasm_document_api_exposes_the_same_versioned_ir() {
    let document = parse_html_document("<p>Shared.</p>".to_string(), None).expect("parse document");
    let value: Value = serde_json::from_str(&document).expect("document JSON");

    assert_eq!(value["ir_version"], json!(1));
    assert_eq!(value["blocks"][0]["text"], json!("Shared."));
}

#[test]
fn wasm_html_reader_api_returns_the_complete_reader_ir() {
    let reader = parse_html_reader("<h1>Title</h1><p>One. Two.</p>".to_string(), None)
        .expect("parse reader");
    let value: Value = serde_json::from_str(&reader).expect("reader JSON");

    assert_eq!(
        value["document"]["blocks"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        value["text_index"]["sentences"].as_array().map(Vec::len),
        Some(3)
    );
    assert_eq!(value["navigation"][0]["sentence_index"], json!(0));
}

#[test]
fn wasm_plain_text_reader_api_returns_the_complete_reader_ir() {
    let reader =
        parse_plain_text_reader("One. Two.\n\n三。".to_string(), None).expect("parse reader");
    let value: Value = serde_json::from_str(&reader).expect("reader JSON");

    assert_eq!(
        value["document"]["blocks"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        value["text_index"]["sentences"].as_array().map(Vec::len),
        Some(3)
    );
}

#[test]
fn wasm_markdown_reader_api_returns_semantic_gui_input() {
    let reader = parse_markdown_reader(
        "# Title\n\nA **shared** paragraph.\n\n> One quote.".to_string(),
        None,
    )
    .expect("parse Markdown reader");
    let value: Value = serde_json::from_str(&reader).expect("reader JSON");

    assert_eq!(
        value["document"]["blocks"][0]["kind"]["type"],
        json!("heading")
    );
    assert_eq!(
        value["document"]["blocks"][1]["text"],
        json!("A shared paragraph.")
    );
    assert_eq!(
        value["document"]["blocks"][1]["inline_spans"][0]["bold"],
        json!(true)
    );
    assert_eq!(
        value["document"]["blocks"][2]["kind"]["type"],
        json!("quote")
    );
    assert_eq!(value["navigation"][0]["title"], json!("Title"));
}

#[test]
fn wasm_pdf_api_returns_the_shared_canonical_text_and_page_count() {
    let parsed = parse_pdf(&minimal_text_pdf(&[
        "Portable PDF parsing belongs in Rust.",
        "Web and Apple read the same text.",
    ]))
    .expect("parse PDF");
    let value: Value = serde_json::from_str(&parsed).expect("PDF JSON");

    assert_eq!(value["page_count"], json!(1));
    assert_eq!(value["title"], json!("Readable PDF"));
    assert_eq!(value["author"], Value::Null);
    assert_eq!(value["chapters"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        value["chapters"][0]["document"]["revision"]["parser_version"],
        json!("pdf-ir-1")
    );
    assert_eq!(
        value["chapters"][0]["block_provenance"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        value["canonical_text"],
        json!("Portable PDF parsing belongs in Rust.\nWeb and Apple read the same text.")
    );
}

#[test]
fn wasm_epub_api_uses_spine_order_and_excludes_non_linear_sections() {
    let parsed = parse_epub(&minimal_epub()).expect("parse EPUB");
    let value: Value = serde_json::from_str(&parsed).expect("EPUB JSON");

    assert_eq!(value["title"], json!("Portable EPUB"));
    assert_eq!(value["author"], json!("Rust SDK"));
    assert_eq!(
        value["chapters"]
            .as_array()
            .expect("chapter array")
            .iter()
            .map(|chapter| chapter["id"].as_str().expect("chapter id"))
            .collect::<Vec<_>>(),
        vec!["second", "first"]
    );
    assert_eq!(value["chapters"][0]["title"], json!("Second Chapter"));
    assert_eq!(value["chapters"][1]["title"], json!("First Chapter"));
    assert_eq!(
        value["chapters"][0]["document"]["blocks"][1]["inline_spans"],
        json!([
            { "range": [7, 11], "bold": true, "italic": false },
            { "range": [15, 23], "bold": false, "italic": true }
        ])
    );
    assert_eq!(
        value["toc"]
            .as_array()
            .expect("TOC array")
            .iter()
            .map(|entry| {
                (
                    entry["title"].as_str().expect("TOC title"),
                    entry["depth"].as_u64().expect("TOC depth"),
                    entry["chapter_id"].as_str().expect("TOC chapter"),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("Part Two", 0, "second"),
            ("Focused body", 1, "second"),
            ("Part One", 0, "first"),
        ]
    );
    assert_eq!(value["toc"][0]["block_id"], Value::Null);
    assert_eq!(
        value["toc"][1]["block_id"],
        value["chapters"][0]["document"]["blocks"][1]["id"]
    );
}

#[test]
fn wasm_epub_asset_api_returns_raw_bytes_for_the_stable_publication_id() {
    let epub = minimal_epub();
    let parsed = parse_epub(&epub).expect("parse EPUB");
    let value: Value = serde_json::from_str(&parsed).expect("EPUB JSON");
    let figure_id = value["assets"]
        .as_array()
        .expect("asset array")
        .iter()
        .find(|asset| asset["href"] == "oebps/figure.png")
        .and_then(|asset| asset["id"].as_str())
        .expect("figure asset ID");

    assert_eq!(
        extract_epub_asset(&epub, figure_id.to_string()).expect("figure bytes"),
        EPUB_FIGURE_BYTES
    );
}

#[test]
fn wasm_position_api_uses_the_shared_scalar_projection() {
    let document =
        parse_html_document("<p>A🙂中</p><p>B</p>".to_string(), None).expect("parse document");
    let projection = project_offset(
        document,
        r#"{"char_offset":2,"affinity":"forward"}"#.to_string(),
    )
    .expect("project position");
    let value: Value = serde_json::from_str(&projection).expect("projection JSON");

    assert_eq!(value["char_offset"], json!(2));
    assert_eq!(value["offset_in_block"], json!(2));
}

#[test]
fn wasm_position_api_resolves_a_block_anchor() {
    let document = parse_html_document("<p>A🙂</p><p>中B</p>".to_string(), None).expect("document");
    let document_value: Value = serde_json::from_str(&document).expect("document JSON");
    let request = json!({
        "block_id": document_value["blocks"][1]["id"],
        "offset_in_block": 1,
        "affinity": "forward",
    });

    let projection = resolve_block_offset(document, request.to_string()).expect("resolve anchor");
    let value: Value = serde_json::from_str(&projection).expect("projection JSON");

    assert_eq!(value["char_offset"], json!(4));
}

#[test]
fn wasm_position_api_restores_without_replacing_the_durable_offset() {
    let document =
        parse_html_document("<p>A🙂中</p><p>B</p>".to_string(), None).expect("parse document");
    let position = r#"{
        "item_id":"book-1",
        "document_revision":"stale",
        "char_offset":999,
        "affinity":"backward",
        "observed_at_unix_ms":10,
        "session_id":"s",
        "mutation_id":"m",
        "base_version":1
    }"#
    .to_string();
    let restored = restore_position(document, position).expect("restore position");
    let value: Value = serde_json::from_str(&restored).expect("restored JSON");

    assert_eq!(value["char_offset"], json!(5));
    assert_eq!(value["revision_changed"], json!(true));
}

#[test]
fn wasm_exposes_the_shared_locator_and_highlight_ranges() {
    let document = parse_html_document(
        "<p>First sentence.</p><p>Second bright sentence.</p>".to_string(),
        None,
    )
    .expect("document");
    let document_value: Value = serde_json::from_str(&document).expect("document JSON");
    let revision = document_value["revision"]["revision_digest"]
        .as_str()
        .expect("revision");
    let locator = create_text_locator(
        document.clone(),
        r#"{"char_offset":23,"affinity":"forward"}"#.to_string(),
    )
    .expect("locator");
    let restored = restore_text_locator(document.clone(), locator).expect("restored locator");
    let restored_value: Value = serde_json::from_str(&restored).expect("restored JSON");
    assert_eq!(restored_value["kind"], json!("exact_revision"));

    let index = index_document_text(document.clone()).expect("index");
    let target = resolve_highlight(
        document,
        index,
        json!({
            "document_revision": revision,
            "sentence_index": 1,
            "word_index": 1,
        })
        .to_string(),
    )
    .expect("target");
    let target_value: Value = serde_json::from_str(&target).expect("target JSON");
    assert_eq!(target_value["sentence_range"], json!([0, 23]));
    assert_eq!(target_value["word_range"], json!([7, 13]));
}

#[test]
fn wasm_search_api_returns_ranges_from_the_shared_core() {
    let document =
        parse_html_document("<p>ＣＩＴＹ and city</p>".to_string(), None).expect("document");
    let document_value: Value = serde_json::from_str(&document).expect("document JSON");
    let request = json!({
        "document_revision": document_value["revision"]["revision_digest"],
        "query": "city",
        "cursor": null,
        "limit": 10,
    });

    let result = search_document(document, request.to_string()).expect("search");
    let value: Value = serde_json::from_str(&result).expect("search JSON");

    assert_eq!(value["total_occurrences"], json!(2));
    assert_eq!(value["hits"][0]["char_range"], json!([0, 4]));
}

#[test]
fn wasm_bookmark_api_returns_the_shared_transition() {
    let document = parse_html_document("<p>Alpha beta.</p>".to_string(), None).expect("document");
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
        upsert_bookmark(document, "[]".to_string(), request.to_string()).expect("bookmark");
    let value: Value = serde_json::from_str(&transition).expect("bookmark JSON");

    assert_eq!(value["created"], json!(true));
    assert_eq!(value["bookmark"]["note"], json!("Alpha beta."));
}

#[test]
fn wasm_sdk_replica_exchanges_real_loro_updates() {
    let desktop = ReadioReplicaWasm::new();
    let web = ReadioReplicaWasm::new();
    let change = desktop
        .upsert(
            "bookmark".to_string(),
            "bookmark-1".to_string(),
            r#"{"item_id":"book-1","char_offset":12}"#.to_string(),
        )
        .expect("create bookmark update");

    web.import_change(&change).expect("import bookmark update");

    let state: Value = serde_json::from_str(&web.state_json()).expect("replica state JSON");
    assert_eq!(
        state["bookmarks"]["bookmark-1"],
        json!({ "item_id": "book-1", "char_offset": 12 })
    );

    let restored = ReadioReplicaWasm::from_snapshot(&web.snapshot().expect("snapshot"))
        .expect("restore snapshot");
    assert_eq!(restored.state_json(), web.state_json());
}

#[test]
fn wasm_sync_state_exposes_shared_local_deletion_semantics() {
    let mut state = ReadioSyncStateWasm::new();
    state
        .stage_local(
            json!([{
                "entityKind": "bookmark",
                "entityId": "book-1:bookmark-1",
                "payload": { "id": "bookmark-1", "item_id": "book-1" }
            }])
            .to_string(),
        )
        .expect("stage bookmark");

    state
        .delete_local(
            json!([{
                "entityKind": "bookmark",
                "entityId": "book-1:bookmark-1"
            }])
            .to_string(),
        )
        .expect("delete bookmark");

    let materialized: Value =
        serde_json::from_str(&state.state_json().expect("state JSON")).expect("valid JSON");
    assert_eq!(materialized["bookmarks"], json!({}));
}

#[test]
fn wasm_sync_state_projects_the_shared_bookmark_order() {
    let mut state = ReadioSyncStateWasm::new();
    state
        .stage_local(
            json!([
                {
                    "entityKind": "bookmark",
                    "entityId": "later",
                    "payload": {
                        "id": "later", "item_id": "book-1", "char_offset": 9,
                        "affinity": "forward", "note": "Later",
                        "created_at_unix_ms": 10, "updated_at_unix_ms": 10,
                        "version": 1
                    }
                },
                {
                    "entityKind": "bookmark",
                    "entityId": "earlier",
                    "payload": {
                        "id": "earlier", "item_id": "book-1", "char_offset": 2,
                        "affinity": "forward", "note": "Earlier",
                        "created_at_unix_ms": 10, "updated_at_unix_ms": 10,
                        "version": 1
                    }
                }
            ])
            .to_string(),
        )
        .expect("stage bookmarks");

    let projected: Value = serde_json::from_str(
        &state
            .bookmarks_json("book-1".to_string())
            .expect("bookmark projection"),
    )
    .expect("valid bookmark JSON");

    assert_eq!(projected[0]["id"], json!("earlier"));
    assert_eq!(projected[1]["id"], json!("later"));
}

#[test]
fn wasm_sync_state_projects_shared_settings() {
    let mut state = ReadioSyncStateWasm::new();
    state
        .stage_local(
            json!([{
                "entityKind": "setting",
                "entityId": "theme",
                "payload": { "value": "dark" }
            }])
            .to_string(),
        )
        .expect("stage setting");

    let settings: Value =
        serde_json::from_str(&state.settings_json().expect("settings projection"))
            .expect("valid settings JSON");

    assert_eq!(settings, json!({ "theme": "dark" }));
}

#[test]
fn wasm_sync_state_queries_the_shared_library_projection() {
    let mut state = ReadioSyncStateWasm::new();
    state
        .stage_local(
            json!([{
                "entityKind": "library_item",
                "entityId": "book-1",
                "payload": {
                    "id": "book-1", "title": "Local SQLite", "type": "txt",
                    "progress": 0, "date": "2026-01-01", "category": "imported",
                    "folder_id": "f1", "source": "upload:book.txt",
                    "file_path": "local:book-1", "cover_image": null,
                    "voice": null, "speed": null, "current_chapter": null,
                    "content_format": null
                }
            }])
            .to_string(),
        )
        .expect("stage library item");

    let page: Value = serde_json::from_str(
        &state
            .query_library_json(r#"{"page":1,"page_size":20}"#.to_string())
            .expect("query library"),
    )
    .expect("valid page JSON");
    let item: Value = serde_json::from_str(
        &state
            .library_item_json("book-1".to_string())
            .expect("read library item"),
    )
    .expect("valid item JSON");

    assert_eq!(page["items"][0]["title"], json!("Local SQLite"));
    assert_eq!(item["id"], json!("book-1"));
}

#[test]
fn wasm_sync_state_projects_the_shared_reading_position() {
    let mut state = ReadioSyncStateWasm::new();
    state
        .stage_local(
            json!([{
                "entityKind": "reading_position",
                "entityId": "book-1",
                "payload": {
                    "document_revision": "revision-1", "char_offset": 44,
                    "affinity": "forward", "observed_at_unix_ms": 1_700_000_000_000_i64,
                    "session_id": "session-a", "mutation_id": "mutation-a",
                    "base_version": 0, "version": 1
                }
            }])
            .to_string(),
        )
        .expect("stage reading position");

    let position: Value = serde_json::from_str(
        &state
            .reading_position_json("book-1".to_string())
            .expect("read position"),
    )
    .expect("valid position JSON");

    assert_eq!(position["char_offset"], json!(44));
    assert_eq!(position["version"], json!(1));
}

#[test]
fn wasm_sync_state_runs_the_rust_reading_position_transition() {
    let mut state = ReadioSyncStateWasm::new();
    let mutation = json!({
        "document_revision": "revision-1", "char_offset": 44,
        "affinity": "forward", "observed_at_unix_ms": 1_700_000_000_000_i64,
        "session_id": "session-a", "mutation_id": "mutation-a",
        "base_version": 0
    });

    let applied: Value = serde_json::from_str(
        &state
            .save_reading_position_json("book-1".to_string(), mutation.to_string())
            .expect("save reading position"),
    )
    .expect("valid transition JSON");
    let replay: Value = serde_json::from_str(
        &state
            .save_reading_position_json("book-1".to_string(), mutation.to_string())
            .expect("replay reading position"),
    )
    .expect("valid replay JSON");

    assert_eq!(applied["version"], json!(1));
    assert_eq!(replay["idempotent_replay"], json!(true));
    assert_eq!(
        serde_json::from_str::<Value>(&state.outbox_json().expect("outbox"))
            .expect("valid outbox")
            .as_array()
            .expect("outbox array")
            .len(),
        1
    );
}

fn minimal_epub() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut archive = zip::ZipWriter::new(&mut bytes);
        let options = zip::write::SimpleFileOptions::default();
        let entries = [
            ("mimetype", "application/epub+zip"),
            (
                "META-INF/container.xml",
                r#"<?xml version="1.0"?>
                <container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
                  <rootfiles><rootfile full-path="OEBPS/content.opf" /></rootfiles>
                </container>"#,
            ),
            (
                "OEBPS/content.opf",
                r#"<?xml version="1.0"?>
                <package xmlns="http://www.idpf.org/2007/opf" version="3.0">
                  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                    <dc:title>Portable EPUB</dc:title>
                    <dc:creator>Rust SDK</dc:creator>
                  </metadata>
                  <manifest>
                    <item id="first" href="first.xhtml" media-type="application/xhtml+xml" />
                    <item id="notes" href="notes.xhtml" media-type="application/xhtml+xml" />
                    <item id="second" href="second.xhtml" media-type="application/xhtml+xml" />
                    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav" />
                    <item id="figure" href="figure.png" media-type="image/png" />
                  </manifest>
                  <spine>
                    <itemref idref="second" />
                    <itemref idref="notes" linear="no" />
                    <itemref idref="first" />
                  </spine>
                </package>"#,
            ),
            (
                "OEBPS/first.xhtml",
                "<html><body><h1>First Chapter</h1><p>First body.</p></body></html>",
            ),
            (
                "OEBPS/notes.xhtml",
                "<html><body><h1>Notes</h1><p>Excluded note.</p></body></html>",
            ),
            (
                "OEBPS/second.xhtml",
                "<html><body><h1>Second Chapter</h1><p id=\"body\">Second <strong>body</strong> is <em>readable</em>.</p><img src=\"figure.png\" alt=\"Diagram\"></body></html>",
            ),
            (
                "OEBPS/nav.xhtml",
                r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
                  <body><nav epub:type="toc"><ol>
                    <li><a href="second.xhtml">Part Two</a><ol>
                      <li><a href="second.xhtml#body">Focused body</a></li>
                    </ol></li>
                    <li><a href="first.xhtml">Part One</a></li>
                  </ol></nav></body>
                </html>"#,
            ),
        ];
        for (name, contents) in entries {
            archive.start_file(name, options).expect("start EPUB entry");
            archive
                .write_all(contents.as_bytes())
                .expect("write EPUB entry");
        }
        archive
            .start_file("OEBPS/figure.png", options)
            .expect("start EPUB figure");
        archive
            .write_all(EPUB_FIGURE_BYTES)
            .expect("write EPUB figure");
        archive.finish().expect("finish EPUB");
    }
    bytes.into_inner()
}

fn minimal_text_pdf(lines: &[&str]) -> Vec<u8> {
    let mut text = String::from("BT\n/F1 12 Tf\n16 TL\n72 720 Td\n");
    for line in lines {
        let escaped = line
            .replace('\\', r"\\")
            .replace('(', r"\(")
            .replace(')', r"\)");
        text.push_str(&format!("({escaped}) Tj\nT*\n"));
    }
    text.push_str("ET\n");
    let stream = text.into_bytes();
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [5 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
        b"<< /Title (Readable PDF) >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents 6 0 R >>".to_vec(),
    ];
    let mut content = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
    content.extend_from_slice(&stream);
    content.extend_from_slice(b"\nendstream");
    objects.push(content);

    let mut output = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(output.len());
        output.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        output.extend_from_slice(object);
        output.extend_from_slice(b"\nendobj\n");
    }
    let xref = output.len();
    output.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    output.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    output.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 4 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    output
}
