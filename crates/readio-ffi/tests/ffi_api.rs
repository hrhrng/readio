use std::{
    ffi::{CStr, CString, c_char},
    io::{Cursor, Write},
};

use readio_ffi::{
    ReadioFfiBytesResult, ReadioFfiReplicaResult, ReadioFfiResult, ReadioReplicaHandle,
    readio_build_support_bundle, readio_builtin_sample, readio_bytes_free, readio_content_identity,
    readio_create_playback_state, readio_create_text_advance_state, readio_extract_epub_asset,
    readio_parse_epub, readio_parse_html, readio_parse_html_reader, readio_parse_markdown_reader,
    readio_parse_plain_text_reader, readio_plan_forget, readio_plan_import,
    readio_plan_sync_push, readio_project_offset,
    readio_project_reading_progress, readio_reduce_playback, readio_reduce_text_advance,
    readio_replica_free, readio_replica_from_snapshot, readio_replica_heads,
    readio_replica_import_change, readio_replica_new, readio_replica_snapshot,
    readio_replica_state, readio_replica_upsert, readio_resolve_block_offset,
    readio_resolve_reading_pace, readio_resolve_reading_seek, readio_restore_position,
    readio_search_document, readio_string_free, readio_sync_bookmarks, readio_sync_library_item,
    readio_sync_query_library, readio_sync_reading_position, readio_sync_reduce,
    readio_sync_save_reading_position, readio_sync_settings, readio_sync_view,
    readio_upsert_bookmark,
};
use serde_json::{Value, json};

const EPUB_ASSET_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nffi-figure";

#[test]
fn c_abi_resolves_the_virtual_builtin_sample_for_apple_clients() {
    let language_tag = CString::new("zh-Hans-CN").expect("language tag CString");

    let sample =
        unsafe { take_result(readio_builtin_sample(language_tag.as_ptr())) }.expect("sample JSON");
    let value: Value = serde_json::from_str(&sample).expect("decode sample");

    assert_eq!(value["item_id"], json!("readio:sample:v1"));
    assert_eq!(value["locale"], json!("zh"));
    assert_eq!(value["source"], json!("readio://sample"));
    assert_eq!(value["is_builtin_sample"], json!(true));
    assert!(
        value["content"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
}

#[test]
fn c_abi_returns_the_same_redacted_support_bundle_to_apple() {
    let request = CString::new(
        json!({
            "generated_at_unix_ms": 100,
            "platform": "apple",
            "app_version": "0.2.0",
            "events": [{
                "timestamp_unix_ms": 90,
                "operation_id": "op-shared",
                "component": "apple-player",
                "category": "playback",
                "name": "clock_sample",
                "fields": {"media_position_ms": 500, "book_text": "Private words"}
            }]
        })
        .to_string(),
    )
    .expect("request CString");

    let bundle = unsafe { take_result(readio_build_support_bundle(request.as_ptr())) }
        .expect("support bundle JSON");
    let value: Value = serde_json::from_str(&bundle).expect("decode bundle");

    assert_eq!(value["events"][0]["fields"]["media_position_ms"], json!(500));
    assert!(!bundle.contains("Private words"));
}

#[test]
fn c_abi_runs_the_shared_playback_reducer_and_transfers_owned_strings() {
    let state = take_result(readio_create_playback_state(2)).expect("state JSON");
    let state = CString::new(state).expect("state CString");
    let event = CString::new(r#"{"type":"play_requested"}"#).expect("event CString");

    let transition = unsafe {
        take_result(readio_reduce_playback(
            std::ptr::null(),
            state.as_ptr(),
            event.as_ptr(),
        ))
    }
    .expect("transition JSON");
    let value: Value = serde_json::from_str(&transition).expect("decode transition");

    assert_eq!(value["state"]["status"], json!("loading"));
    assert_eq!(value["effects"][0]["type"], json!("fetch_audio"));
}

#[test]
fn c_abi_exposes_the_shared_reading_mode_and_effort_projection() {
    let preferences = CString::new(
        r#"{"mode":"read_aloud","base_text_characters_per_second":60,"effort":"max"}"#,
    )
    .expect("preferences CString");

    let pace = unsafe { take_result(readio_resolve_reading_pace(preferences.as_ptr())) }
        .expect("pace JSON");
    let value: Value = serde_json::from_str(&pace).expect("decode pace");

    assert_eq!(value["mode"], json!("read_aloud"));
    assert_eq!(value["effort"], json!("max"));
    assert_eq!(value["multiplier"], json!(0.7));
    assert_eq!(value["text_characters_per_second"], json!(42.0));
    assert_eq!(value["audio_playback_rate"], json!(0.7));
    assert_eq!(value["uses_audio"], json!(true));
}

#[test]
fn c_abi_exposes_honest_reading_progress_and_seek_projection() {
    let request = CString::new(
        r#"{"segment_character_counts":[2,8],"current_segment":1,"current_segment_progress_basis_points":5000,"timing":{"type":"unknown"}}"#,
    )
    .expect("progress CString");
    let projection = unsafe { take_result(readio_project_reading_progress(request.as_ptr())) }
        .expect("reading projection");
    let projection: Value = serde_json::from_str(&projection).expect("projection JSON");
    assert_eq!(projection["char_offset"], json!(7));
    assert_eq!(projection["progress_basis_points"], json!(6_363));
    assert_eq!(projection["remaining_time"]["seconds"], Value::Null);

    let request =
        CString::new(r#"{"segment_character_counts":[1,9],"progress_basis_points":5000}"#)
            .expect("seek CString");
    let cursor = unsafe { take_result(readio_resolve_reading_seek(request.as_ptr())) }
        .expect("reading seek");
    let cursor: Value = serde_json::from_str(&cursor).expect("cursor JSON");
    assert_eq!(cursor["segment"], json!(1));
    assert_eq!(cursor["segment_progress_basis_points"], json!(3_333));
}

#[test]
fn c_abi_exposes_the_shared_text_advance_reducer() {
    let state = take_result(readio_create_text_advance_state(2)).expect("text state");
    let state = CString::new(state).expect("state CString");
    let event = CString::new(
        r#"{"type":"start_requested","character_count":80,"text_characters_per_second":40,"monotonic_ms":10000}"#,
    )
    .expect("event CString");

    let transition =
        unsafe { take_result(readio_reduce_text_advance(state.as_ptr(), event.as_ptr())) }
            .expect("text transition");
    let value: Value = serde_json::from_str(&transition).expect("decode transition");

    assert_eq!(value["state"]["status"], json!("running"));
    assert_eq!(value["effects"][0]["after_ms"], json!(2_000));
}

#[test]
fn c_abi_exposes_the_shared_document_parser() {
    let html = CString::new("<h2>Native core</h2>").expect("HTML CString");

    let document = unsafe { take_result(readio_parse_html(html.as_ptr(), std::ptr::null())) }
        .expect("document JSON");
    let value: Value = serde_json::from_str(&document).expect("decode document");

    assert_eq!(value["ir_version"], json!(1));
    assert_eq!(value["blocks"][0]["kind"]["type"], json!("heading"));
    assert_eq!(value["blocks"][0]["text"], json!("Native core"));
}

#[test]
fn c_abi_exposes_complete_html_reader_ir() {
    let html = CString::new("<h2>Native reader</h2><p>One. Two.</p>").expect("HTML CString");

    let reader = unsafe { take_result(readio_parse_html_reader(html.as_ptr(), std::ptr::null())) }
        .expect("reader JSON");
    let value: Value = serde_json::from_str(&reader).expect("decode reader");

    assert_eq!(
        value["document"]["blocks"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        value["text_index"]["sentences"].as_array().map(Vec::len),
        Some(3)
    );
    assert_eq!(value["navigation"][0]["title"], json!("Native reader"));
}

#[test]
fn c_abi_exposes_complete_plain_text_reader_ir() {
    let text = CString::new("One. Two.\n\n三。").expect("text CString");

    let reader = unsafe {
        take_result(readio_parse_plain_text_reader(
            text.as_ptr(),
            std::ptr::null(),
        ))
    }
    .expect("reader JSON");
    let value: Value = serde_json::from_str(&reader).expect("decode reader");

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
fn c_abi_exposes_complete_markdown_reader_ir() {
    let markdown = CString::new(
        "# Native Markdown\n\nRead **one thing** well.\n\n> A shared quote.\n\n```rs\nlet calm = true;\n```",
    )
    .expect("Markdown CString");

    let reader = unsafe {
        take_result(readio_parse_markdown_reader(
            markdown.as_ptr(),
            std::ptr::null(),
        ))
    }
    .expect("reader JSON");
    let value: Value = serde_json::from_str(&reader).expect("decode reader");

    assert_eq!(value["document"]["blocks"][0]["kind"]["type"], json!("heading"));
    assert_eq!(value["document"]["blocks"][1]["text"], json!("Read one thing well."));
    assert_eq!(
        value["document"]["blocks"][1]["inline_spans"][0]["range"],
        json!([5, 14])
    );
    assert_eq!(value["document"]["blocks"][2]["kind"]["type"], json!("quote"));
    assert_eq!(value["document"]["blocks"][3]["kind"]["type"], json!("code"));
    assert_eq!(value["navigation"][0]["title"], json!("Native Markdown"));
}

#[test]
fn c_abi_transfers_epub_asset_bytes_without_base64() {
    let epub = epub_asset_fixture();
    let publication = unsafe { take_result(readio_parse_epub(epub.as_ptr(), epub.len())) }
        .expect("publication JSON");
    let publication: Value = serde_json::from_str(&publication).expect("publication value");
    let asset_id = publication["assets"][0]["id"].as_str().expect("asset ID");
    let asset_id = CString::new(asset_id).expect("asset ID CString");

    let bytes = unsafe {
        take_bytes_result(readio_extract_epub_asset(
            epub.as_ptr(),
            epub.len(),
            asset_id.as_ptr(),
        ))
    }
    .expect("asset bytes");

    assert_eq!(bytes, EPUB_ASSET_BYTES);
}

#[test]
fn c_abi_exposes_shared_import_identity_ownership_and_forget_plans() {
    let bytes = b"same book";
    let identity = unsafe { take_result(readio_content_identity(bytes.as_ptr(), bytes.len())) }
        .expect("content identity");
    let identity: Value = serde_json::from_str(&identity).expect("identity JSON");
    let request = CString::new(
        json!({
            "content_identity": identity,
            "ownership": "link",
            "source_key": "file:///book.txt",
            "owned_storage_key": "content/book.txt",
            "existing": null,
        })
        .to_string(),
    )
    .expect("import request CString");
    let plan = unsafe { take_result(readio_plan_import(request.as_ptr())) }.expect("import plan");
    let plan: Value = serde_json::from_str(&plan).expect("plan JSON");
    assert_eq!(plan["outcome"], json!("created"));
    assert_eq!(plan["record"]["ownership"], json!("link"));
    assert_eq!(plan["effects"], json!([]));

    let record = CString::new(plan["record"].to_string()).expect("record CString");
    let forget = unsafe { take_result(readio_plan_forget(record.as_ptr())) }.expect("forget plan");
    assert_eq!(serde_json::from_str::<Value>(&forget).unwrap(), json!([]));
}

#[test]
fn c_abi_projects_and_restores_the_shared_scalar_position() {
    let html = CString::new("<p>A🙂中</p><p>B</p>").expect("HTML CString");
    let document = unsafe { take_result(readio_parse_html(html.as_ptr(), std::ptr::null())) }
        .expect("document JSON");
    let document = CString::new(document).expect("document CString");
    let request =
        CString::new(r#"{"char_offset":2,"affinity":"forward"}"#).expect("request CString");

    let projection =
        unsafe { take_result(readio_project_offset(document.as_ptr(), request.as_ptr())) }
            .expect("projection JSON");
    let value: Value = serde_json::from_str(&projection).expect("decode projection");
    assert_eq!(value["char_offset"], json!(2));
    assert_eq!(value["offset_in_block"], json!(2));

    let document_value: Value =
        serde_json::from_str(document.to_str().expect("document UTF-8")).expect("document value");
    let anchor = CString::new(
        json!({
            "block_id": document_value["blocks"][1]["id"],
            "offset_in_block": 0,
            "affinity": "forward",
        })
        .to_string(),
    )
    .expect("anchor CString");
    let resolved = unsafe {
        take_result(readio_resolve_block_offset(
            document.as_ptr(),
            anchor.as_ptr(),
        ))
    }
    .expect("resolved anchor JSON");
    let resolved: Value = serde_json::from_str(&resolved).expect("resolved value");
    assert_eq!(resolved["char_offset"], json!(4));

    let position = CString::new(
        r#"{"item_id":"book-1","document_revision":"stale","char_offset":999,"affinity":"backward","observed_at_unix_ms":10,"session_id":"s","mutation_id":"m","base_version":1}"#,
    )
    .expect("position CString");
    let restored = unsafe {
        take_result(readio_restore_position(
            document.as_ptr(),
            position.as_ptr(),
        ))
    }
    .expect("restored JSON");
    let value: Value = serde_json::from_str(&restored).expect("decode restored position");
    assert_eq!(value["char_offset"], json!(5));
    assert_eq!(value["revision_changed"], json!(true));
}

#[test]
fn c_abi_exposes_shared_search_and_bookmark_semantics() {
    let html = CString::new("<p>ＣＩＴＹ and city</p>").expect("HTML CString");
    let document = unsafe { take_result(readio_parse_html(html.as_ptr(), std::ptr::null())) }
        .expect("document JSON");
    let document_value: Value = serde_json::from_str(&document).expect("document value");
    let document = CString::new(document).expect("document CString");
    let search_request = CString::new(
        json!({
            "document_revision": document_value["revision"]["revision_digest"],
            "query": "city",
            "cursor": null,
            "limit": 10,
        })
        .to_string(),
    )
    .expect("search CString");
    let search = unsafe {
        take_result(readio_search_document(
            document.as_ptr(),
            search_request.as_ptr(),
        ))
    }
    .expect("search JSON");
    let search: Value = serde_json::from_str(&search).expect("search value");
    assert_eq!(search["total_occurrences"], json!(2));

    let bookmarks = CString::new("[]").expect("bookmarks CString");
    let bookmark_request = CString::new(
        json!({
            "bookmark_id": "bookmark-1",
            "item_id": "book-1",
            "char_offset": 6,
            "affinity": "forward",
            "note": "saved",
            "created_at_unix_ms": 10,
            "updated_at_unix_ms": 10,
            "base_version": 0,
        })
        .to_string(),
    )
    .expect("bookmark CString");
    let bookmark = unsafe {
        take_result(readio_upsert_bookmark(
            document.as_ptr(),
            bookmarks.as_ptr(),
            bookmark_request.as_ptr(),
        ))
    }
    .expect("bookmark JSON");
    let bookmark: Value = serde_json::from_str(&bookmark).expect("bookmark value");
    assert_eq!(bookmark["created"], json!(true));
    assert_eq!(bookmark["bookmark"]["note"], json!("saved"));
}

#[test]
fn c_abi_exposes_loro_replica_updates_and_snapshot_recovery() {
    let desktop = readio_replica_new();
    let web = readio_replica_new();
    assert!(!desktop.is_null());
    assert!(!web.is_null());

    let kind = CString::new("bookmark").expect("kind CString");
    let entity_id = CString::new("bookmark-1").expect("entity CString");
    let payload =
        CString::new(r#"{"item_id":"book-1","char_offset":18}"#).expect("payload CString");
    let change = unsafe {
        take_result(readio_replica_upsert(
            desktop,
            kind.as_ptr(),
            entity_id.as_ptr(),
            payload.as_ptr(),
        ))
    }
    .expect("Loro update");
    let change = CString::new(change).expect("change CString");
    unsafe {
        take_result(readio_replica_import_change(web, change.as_ptr())).expect("import update");
    }

    let state = unsafe { take_result(readio_replica_state(web)) }.expect("replica state");
    let state: Value = serde_json::from_str(&state).expect("state JSON");
    assert_eq!(
        state["bookmarks"]["bookmark-1"],
        json!({ "item_id": "book-1", "char_offset": 18 })
    );

    let heads = unsafe { take_result(readio_replica_heads(web)) }.expect("replica heads");
    assert!(!heads.is_empty());

    let snapshot = unsafe { take_result(readio_replica_snapshot(web)) }.expect("snapshot");
    let snapshot = CString::new(snapshot).expect("snapshot CString");
    let restored = unsafe { take_replica_result(readio_replica_from_snapshot(snapshot.as_ptr())) }
        .expect("restore replica");
    let restored_state =
        unsafe { take_result(readio_replica_state(restored)) }.expect("restored state");
    assert_eq!(
        restored_state,
        serde_json::to_string(&state).expect("canonical state")
    );

    unsafe {
        readio_replica_free(desktop);
        readio_replica_free(web);
        readio_replica_free(restored);
    }
}

#[test]
fn c_abi_reduces_and_projects_the_shared_replica_sync_state() {
    let state = CString::new("{}").expect("state CString");
    let event = CString::new(
        r#"{"type":"stage_local","entities":[{"entityKind":"reading_position","entityId":"book-1","payload":{"char_offset":42}}]}"#,
    )
    .expect("event CString");
    let staged = unsafe { take_result(readio_sync_reduce(state.as_ptr(), event.as_ptr())) }
        .expect("stage sync state");
    let staged = CString::new(staged).expect("staged CString");

    let view = unsafe { take_result(readio_sync_view(staged.as_ptr())) }.expect("sync view");
    let view: Value = serde_json::from_str(&view).expect("view JSON");
    assert_eq!(view["outbox"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        view["state"]["reading_positions"]["book-1"],
        json!({ "char_offset": 42 })
    );
    assert_ne!(view["headsBase64"], json!(""));
}

#[test]
fn c_abi_exposes_the_same_sync_projections_and_position_transition_to_apple() {
    let state = CString::new("{}").expect("state CString");
    let event = CString::new(
        json!({
            "type": "stage_local",
            "entities": [
                {
                    "entityKind": "library_item", "entityId": "book-1",
                    "payload": {
                        "id": "book-1", "title": "Native Local", "type": "txt",
                        "progress": 0, "date": "2026-01-01", "category": "imported",
                        "folder_id": "f1", "source": "local:book.txt", "file_path": "local:book-1",
                        "cover_image": null, "voice": null, "speed": null,
                        "current_chapter": null, "content_format": "plaintext"
                    }
                },
                {
                    "entityKind": "setting", "entityId": "theme",
                    "payload": { "value": "dark" }
                },
                {
                    "entityKind": "bookmark", "entityId": "bookmark-1",
                    "payload": {
                        "id": "bookmark-1", "item_id": "book-1", "char_offset": 7,
                        "affinity": "forward", "note": "saved", "created_at_unix_ms": 10,
                        "updated_at_unix_ms": 10, "version": 1
                    }
                }
            ]
        })
        .to_string(),
    )
    .expect("event CString");
    let staged = unsafe { take_result(readio_sync_reduce(state.as_ptr(), event.as_ptr())) }
        .expect("stage sync state");
    let staged = CString::new(staged).expect("staged CString");
    let query = CString::new(r#"{"page":1,"page_size":20}"#).expect("query CString");
    let item_id = CString::new("book-1").expect("item id CString");

    let page = unsafe { take_result(readio_sync_query_library(staged.as_ptr(), query.as_ptr())) }
        .expect("library page");
    let item = unsafe { take_result(readio_sync_library_item(staged.as_ptr(), item_id.as_ptr())) }
        .expect("library item");
    let settings = unsafe { take_result(readio_sync_settings(staged.as_ptr())) }.expect("settings");
    let bookmarks =
        unsafe { take_result(readio_sync_bookmarks(staged.as_ptr(), item_id.as_ptr())) }
            .expect("bookmarks");

    assert_eq!(
        serde_json::from_str::<Value>(&page).expect("page JSON")["items"][0]["title"],
        json!("Native Local")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&item).expect("item JSON")["id"],
        json!("book-1")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&settings).expect("settings JSON")["theme"],
        json!("dark")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&bookmarks).expect("bookmarks JSON")[0]["id"],
        json!("bookmark-1")
    );

    let mutation = CString::new(
        r#"{"document_revision":"revision-1","char_offset":42,"affinity":"forward","observed_at_unix_ms":1700000000000,"session_id":"apple-a","mutation_id":"mutation-a","base_version":0}"#,
    )
    .expect("mutation CString");
    let transition = unsafe {
        take_result(readio_sync_save_reading_position(
            staged.as_ptr(),
            item_id.as_ptr(),
            mutation.as_ptr(),
        ))
    }
    .expect("position transition");
    let transition: Value = serde_json::from_str(&transition).expect("transition JSON");
    let next_state = CString::new(
        transition["stateJSON"]
            .as_str()
            .expect("serialized next state"),
    )
    .expect("next state CString");
    let position = unsafe {
        take_result(readio_sync_reading_position(
            next_state.as_ptr(),
            item_id.as_ptr(),
        ))
    }
    .expect("reading position");

    assert_eq!(transition["response"]["version"], json!(1));
    assert_eq!(
        serde_json::from_str::<Value>(&position).expect("position JSON")["char_offset"],
        json!(42)
    );
}

#[test]
fn c_abi_exposes_the_sdk_storage_projection_plan() {
    let snapshot = CString::new(r#"{"entities":[],"mutations":[]}"#).expect("snapshot");
    let batch = CString::new(
        r#"{"device_id":"apple-a","mutations":[{"mutation_id":"m-1","entity_kind":"setting","entity_id":"theme","operation":"upsert","base_version":0,"client_timestamp_unix_ms":1,"payload":{"value":"dark"}}]}"#,
    )
    .expect("batch");
    let plan = unsafe {
        take_result(readio_plan_sync_push(
            snapshot.as_ptr(),
            batch.as_ptr(),
            10,
            0,
        ))
    }
    .expect("storage plan");
    let plan: Value = serde_json::from_str(&plan).expect("plan JSON");

    assert_eq!(plan["cursor"], json!(1));
    assert_eq!(plan["results"][0]["status"], json!("applied"));
}

fn take_result(result: ReadioFfiResult) -> Result<String, String> {
    let value = take_string(result.value);
    let error = take_string(result.error);
    match (value, error) {
        (Some(value), None) => Ok(value),
        (None, Some(error)) => Err(error),
        _ => Err("FFI returned an invalid ownership pair".to_string()),
    }
}

unsafe fn take_bytes_result(result: ReadioFfiBytesResult) -> Result<Vec<u8>, String> {
    let error = take_string(result.error);
    match (result.value.is_null(), result.length, error) {
        (false, length, None) => {
            let bytes = unsafe { std::slice::from_raw_parts(result.value, length) }.to_vec();
            unsafe { readio_bytes_free(result.value, length) };
            Ok(bytes)
        }
        (true, 0, Some(error)) => Err(error),
        _ => Err("FFI returned an invalid byte ownership tuple".to_string()),
    }
}

unsafe fn take_replica_result(
    result: ReadioFfiReplicaResult,
) -> Result<*mut ReadioReplicaHandle, String> {
    let error = take_string(result.error);
    match (result.value.is_null(), error) {
        (false, None) => Ok(result.value),
        (true, Some(error)) => Err(error),
        _ => Err("FFI returned an invalid replica ownership pair".to_string()),
    }
}

fn take_string(pointer: *mut c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    let value = unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned();
    unsafe { readio_string_free(pointer) };
    Some(value)
}

fn epub_asset_fixture() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut archive = zip::ZipWriter::new(&mut bytes);
        let options = zip::write::SimpleFileOptions::default();
        let entries = [
            ("mimetype", "application/epub+zip"),
            (
                "META-INF/container.xml",
                r#"<container><rootfiles><rootfile full-path="OPS/content.opf" /></rootfiles></container>"#,
            ),
            (
                "OPS/content.opf",
                r#"<package><metadata><meta name="cover" content="figure" /></metadata><manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml" /><item id="figure" href="figure.png" media-type="image/png" /></manifest><spine><itemref idref="chapter" /></spine></package>"#,
            ),
            (
                "OPS/chapter.xhtml",
                "<html><body><h1>FFI</h1><img src=\"figure.png\" alt=\"Figure\"></body></html>",
            ),
        ];
        for (name, contents) in entries {
            archive.start_file(name, options).expect("start EPUB entry");
            archive
                .write_all(contents.as_bytes())
                .expect("write EPUB entry");
        }
        archive
            .start_file("OPS/figure.png", options)
            .expect("start EPUB asset");
        archive
            .write_all(EPUB_ASSET_BYTES)
            .expect("write EPUB asset");
        archive.finish().expect("finish EPUB");
    }
    bytes.into_inner()
}
