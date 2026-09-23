use std::ffi::{CStr, CString, c_char};

use base64::{Engine as _, engine::general_purpose::STANDARD};

#[repr(C)]
pub struct ReadioFfiResult {
    pub value: *mut c_char,
    pub error: *mut c_char,
}

#[repr(C)]
pub struct ReadioFfiBytesResult {
    pub value: *mut u8,
    pub length: usize,
    pub error: *mut c_char,
}

#[repr(C)]
pub struct ReadioReplicaHandle {
    replica: readio_sdk::replica::ReadioReplica,
}

#[repr(C)]
pub struct ReadioFfiReplicaResult {
    pub value: *mut ReadioReplicaHandle,
    pub error: *mut c_char,
}

#[unsafe(no_mangle)]
pub extern "C" fn readio_create_playback_state(sentence_count: u32) -> ReadioFfiResult {
    ffi_result(readio_bridge::create_playback_state_json(
        sentence_count as usize,
    ))
}

#[unsafe(no_mangle)]
/// Reduces a playback event encoded as JSON through the shared Rust state machine.
///
/// # Safety
///
/// `state_json` and `event_json` must point to valid, NUL-terminated UTF-8 strings.
/// `policy_json` may be null; when non-null it must follow the same string contract.
pub unsafe extern "C" fn readio_reduce_playback(
    policy_json: *const c_char,
    state_json: *const c_char,
    event_json: *const c_char,
) -> ReadioFfiResult {
    let policy = unsafe { read_optional_string(policy_json) };
    let state = unsafe { read_required_string(state_json, "state_json") };
    let event = unsafe { read_required_string(event_json, "event_json") };
    let result = policy.and_then(|policy| {
        state.and_then(|state| {
            event.and_then(|event| {
                readio_bridge::reduce_playback_json(policy.as_deref(), &state, &event)
                    .map_err(|error| error.to_string())
            })
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Resolves reading mode and six-level effort preferences into text and audio pace.
///
/// # Safety
///
/// `preferences_json` must point to a valid, NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn readio_resolve_reading_pace(
    preferences_json: *const c_char,
) -> ReadioFfiResult {
    let result = unsafe { read_required_string(preferences_json, "preferences_json") }.and_then(
        |preferences| {
            readio_bridge::resolve_reading_pace_json(&preferences)
                .map_err(|error| error.to_string())
        },
    );
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Projects scalar reading progress and evidence-backed remaining time.
///
/// # Safety
///
/// `request_json` must point to a valid, NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn readio_project_reading_progress(
    request_json: *const c_char,
) -> ReadioFfiResult {
    let result =
        unsafe { read_required_string(request_json, "request_json") }.and_then(|request| {
            readio_bridge::project_reading_progress_json(&request)
                .map_err(|error| error.to_string())
        });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Resolves a progress-bar target through canonical scalar segment lengths.
///
/// # Safety
///
/// `request_json` must point to a valid, NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn readio_resolve_reading_seek(
    request_json: *const c_char,
) -> ReadioFfiResult {
    let result =
        unsafe { read_required_string(request_json, "request_json") }.and_then(|request| {
            readio_bridge::resolve_reading_seek_json(&request).map_err(|error| error.to_string())
        });
    ffi_result(result)
}

#[unsafe(no_mangle)]
pub extern "C" fn readio_create_text_advance_state(sentence_count: u32) -> ReadioFfiResult {
    ffi_result(readio_bridge::create_text_advance_state_json(
        sentence_count as usize,
    ))
}

#[unsafe(no_mangle)]
/// Resolves the virtual built-in sample for a BCP 47 language tag.
///
/// # Safety
///
/// `language_tag` must point to a valid, NUL-terminated UTF-8 string.
pub unsafe extern "C" fn readio_builtin_sample(language_tag: *const c_char) -> ReadioFfiResult {
    let result =
        unsafe { read_required_string(language_tag, "language_tag") }.and_then(|language_tag| {
            readio_bridge::builtin_sample_json(&language_tag).map_err(|error| error.to_string())
        });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Builds a bounded, redacted diagnostics payload for Apple support export.
///
/// # Safety
///
/// `request_json` must point to a valid NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn readio_build_support_bundle(
    request_json: *const c_char,
) -> ReadioFfiResult {
    ffi_result(unsafe { read_required_string(request_json, "request_json") }.and_then(|request| {
        readio_bridge::build_support_bundle_json(&request).map_err(|error| error.to_string())
    }))
}

#[unsafe(no_mangle)]
/// Computes the shared full-byte content identity for an import candidate.
///
/// # Safety
///
/// `bytes` must reference `length` readable bytes. It may be null only when
/// `length` is zero.
pub unsafe extern "C" fn readio_content_identity(
    bytes: *const u8,
    length: usize,
) -> ReadioFfiResult {
    let result = unsafe { read_bytes(bytes, length) }.and_then(|bytes| {
        readio_bridge::content_identity_json(bytes).map_err(|error| error.to_string())
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Plans copy/link/move ownership and de-duplication effects.
///
/// # Safety
///
/// `request_json` must point to a valid NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn readio_plan_import(request_json: *const c_char) -> ReadioFfiResult {
    let result =
        unsafe { read_required_string(request_json, "request_json") }.and_then(|request| {
            readio_bridge::plan_import_json(&request).map_err(|error| error.to_string())
        });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Plans safe content cleanup when forgetting a library item.
///
/// # Safety
///
/// `record_json` must point to a valid NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn readio_plan_forget(record_json: *const c_char) -> ReadioFfiResult {
    let result = unsafe { read_required_string(record_json, "record_json") }.and_then(|record| {
        readio_bridge::plan_forget_json(&record).map_err(|error| error.to_string())
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Reduces one automatic text-advance event through the shared Rust state machine.
///
/// # Safety
///
/// Both arguments must point to valid, NUL-terminated UTF-8 JSON strings.
pub unsafe extern "C" fn readio_reduce_text_advance(
    state_json: *const c_char,
    event_json: *const c_char,
) -> ReadioFfiResult {
    let state = unsafe { read_required_string(state_json, "state_json") };
    let event = unsafe { read_required_string(event_json, "event_json") };
    let result = state.and_then(|state| {
        event.and_then(|event| {
            readio_bridge::reduce_text_advance_json(&state, &event)
                .map_err(|error| error.to_string())
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Parses HTML into the platform-neutral Document IR encoded as JSON.
///
/// # Safety
///
/// `html` must point to a valid, NUL-terminated UTF-8 string. `options_json` may
/// be null; when non-null it must follow the same string contract.
pub unsafe extern "C" fn readio_parse_html(
    html: *const c_char,
    options_json: *const c_char,
) -> ReadioFfiResult {
    let html = unsafe { read_required_string(html, "html") };
    let options = unsafe { read_optional_string(options_json) };
    let result = html.and_then(|html| {
        options.and_then(|options| {
            readio_bridge::parse_html_json(&html, options.as_deref())
                .map_err(|error| error.to_string())
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Parses HTML into the complete versioned Reader IR encoded as JSON.
///
/// # Safety
///
/// `html` must point to a valid, NUL-terminated UTF-8 string. `options_json` may
/// be null; when non-null it must follow the same string contract.
pub unsafe extern "C" fn readio_parse_html_reader(
    html: *const c_char,
    options_json: *const c_char,
) -> ReadioFfiResult {
    let html = unsafe { read_required_string(html, "html") };
    let options = unsafe { read_optional_string(options_json) };
    let result = html.and_then(|html| {
        options.and_then(|options| {
            readio_bridge::parse_html_reader_json(&html, options.as_deref())
                .map_err(|error| error.to_string())
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Parses Markdown into the complete versioned Reader IR encoded as JSON.
///
/// # Safety
///
/// `markdown` must point to a valid, NUL-terminated UTF-8 string. `options_json`
/// may be null; when non-null it must follow the same string contract.
pub unsafe extern "C" fn readio_parse_markdown_reader(
    markdown: *const c_char,
    options_json: *const c_char,
) -> ReadioFfiResult {
    let markdown = unsafe { read_required_string(markdown, "markdown") };
    let options = unsafe { read_optional_string(options_json) };
    let result = markdown.and_then(|markdown| {
        options.and_then(|options| {
            readio_bridge::parse_markdown_reader_json(&markdown, options.as_deref())
                .map_err(|error| error.to_string())
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Parses plain text into the complete versioned Reader IR encoded as JSON.
///
/// # Safety
///
/// `text` must point to a valid, NUL-terminated UTF-8 string. `options_json` may
/// be null; when non-null it must follow the same string contract.
pub unsafe extern "C" fn readio_parse_plain_text_reader(
    text: *const c_char,
    options_json: *const c_char,
) -> ReadioFfiResult {
    let text = unsafe { read_required_string(text, "text") };
    let options = unsafe { read_optional_string(options_json) };
    let result = text.and_then(|text| {
        options.and_then(|options| {
            readio_bridge::parse_plain_text_reader_json(&text, options.as_deref())
                .map_err(|error| error.to_string())
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Parses a text-layer PDF into canonical reader text encoded as JSON.
///
/// # Safety
///
/// `bytes` must reference `length` readable bytes. It may be null only when
/// `length` is zero; malformed and scanned PDFs return a normal FFI error.
pub unsafe extern "C" fn readio_parse_pdf(bytes: *const u8, length: usize) -> ReadioFfiResult {
    let result: Result<String, String> = (|| {
        let bytes = unsafe { read_bytes(bytes, length) }?;
        let parsed = readio_sdk::ReadioSdk::new()
            .media
            .parse_pdf(bytes)
            .map_err(|error| error.to_string())?;
        serde_json::to_string(&parsed).map_err(|error| error.to_string())
    })();
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Parses an EPUB archive into ordered chapter Document IR encoded as JSON.
///
/// # Safety
///
/// `bytes` must reference `length` readable bytes. It may be null only when
/// `length` is zero; malformed EPUBs return a normal FFI error.
pub unsafe extern "C" fn readio_parse_epub(bytes: *const u8, length: usize) -> ReadioFfiResult {
    let result: Result<String, String> = (|| {
        let bytes = unsafe { read_bytes(bytes, length) }?;
        let parsed = readio_sdk::ReadioSdk::new()
            .media
            .parse_epub(bytes)
            .map_err(|error| error.to_string())?;
        serde_json::to_string(&parsed).map_err(|error| error.to_string())
    })();
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Extracts one package-declared EPUB asset into an owned byte buffer.
///
/// # Safety
///
/// `bytes` must reference `length` readable bytes. `asset_id` must point to a
/// valid NUL-terminated UTF-8 string. A successful buffer must be released once
/// with [`readio_bytes_free`].
pub unsafe extern "C" fn readio_extract_epub_asset(
    bytes: *const u8,
    length: usize,
    asset_id: *const c_char,
) -> ReadioFfiBytesResult {
    let bytes = unsafe { read_bytes(bytes, length) };
    let asset_id = unsafe { read_required_string(asset_id, "asset_id") };
    let result = bytes.and_then(|bytes| {
        asset_id.and_then(|asset_id| {
            readio_sdk::ReadioSdk::new()
                .media
                .extract_epub_asset(bytes, &asset_id)
                .map_err(|error| error.to_string())
        })
    });
    ffi_bytes_result(result)
}

#[unsafe(no_mangle)]
/// Projects a durable scalar offset against a Document IR JSON value.
///
/// # Safety
///
/// Both arguments must point to valid, NUL-terminated UTF-8 JSON strings.
pub unsafe extern "C" fn readio_project_offset(
    document_json: *const c_char,
    request_json: *const c_char,
) -> ReadioFfiResult {
    let document = unsafe { read_required_string(document_json, "document_json") };
    let request = unsafe { read_required_string(request_json, "request_json") };
    let result = document.and_then(|document| {
        request.and_then(|request| {
            readio_bridge::project_offset_json(&document, &request)
                .map_err(|error| error.to_string())
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Creates a versioned, renderer-independent locator from a scalar offset.
///
/// # Safety
///
/// Both arguments must point to valid, NUL-terminated UTF-8 JSON strings.
pub unsafe extern "C" fn readio_create_text_locator(
    document_json: *const c_char,
    request_json: *const c_char,
) -> ReadioFfiResult {
    let document = unsafe { read_required_string(document_json, "document_json") };
    let request = unsafe { read_required_string(request_json, "request_json") };
    ffi_result(document.and_then(|document| {
        request.and_then(|request| {
            readio_bridge::create_text_locator_json(&document, &request)
                .map_err(|error| error.to_string())
        })
    }))
}

#[unsafe(no_mangle)]
/// Restores a locator and reports which recovery tier was used.
///
/// # Safety
///
/// Both arguments must point to valid, NUL-terminated UTF-8 JSON strings.
pub unsafe extern "C" fn readio_restore_text_locator(
    document_json: *const c_char,
    locator_json: *const c_char,
) -> ReadioFfiResult {
    let document = unsafe { read_required_string(document_json, "document_json") };
    let locator = unsafe { read_required_string(locator_json, "locator_json") };
    ffi_result(document.and_then(|document| {
        locator.and_then(|locator| {
            readio_bridge::restore_text_locator_json(&document, &locator)
                .map_err(|error| error.to_string())
        })
    }))
}

#[unsafe(no_mangle)]
/// Builds the shared sentence/word scalar index for a Document IR.
///
/// # Safety
///
/// `document_json` must point to a valid, NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn readio_index_document_text(
    document_json: *const c_char,
) -> ReadioFfiResult {
    let document = unsafe { read_required_string(document_json, "document_json") };
    ffi_result(document.and_then(|document| {
        readio_bridge::index_document_text_json(&document).map_err(|error| error.to_string())
    }))
}

#[unsafe(no_mangle)]
/// Maps a playback cursor to renderer-local sentence and word ranges.
///
/// # Safety
///
/// All arguments must point to valid, NUL-terminated UTF-8 JSON strings.
pub unsafe extern "C" fn readio_resolve_highlight(
    document_json: *const c_char,
    index_json: *const c_char,
    cursor_json: *const c_char,
) -> ReadioFfiResult {
    let document = unsafe { read_required_string(document_json, "document_json") };
    let index = unsafe { read_required_string(index_json, "index_json") };
    let cursor = unsafe { read_required_string(cursor_json, "cursor_json") };
    ffi_result(document.and_then(|document| {
        index.and_then(|index| {
            cursor.and_then(|cursor| {
                readio_bridge::resolve_highlight_json(&document, &index, &cursor)
                    .map_err(|error| error.to_string())
            })
        })
    }))
}

#[unsafe(no_mangle)]
/// Resolves a block-local anchor into the durable scalar coordinate.
///
/// # Safety
///
/// Both arguments must point to valid, NUL-terminated UTF-8 JSON strings.
pub unsafe extern "C" fn readio_resolve_block_offset(
    document_json: *const c_char,
    request_json: *const c_char,
) -> ReadioFfiResult {
    let document = unsafe { read_required_string(document_json, "document_json") };
    let request = unsafe { read_required_string(request_json, "request_json") };
    let result = document.and_then(|document| {
        request.and_then(|request| {
            readio_bridge::resolve_block_offset_json(&document, &request)
                .map_err(|error| error.to_string())
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Restores a durable position against the current Document IR JSON value.
///
/// # Safety
///
/// Both arguments must point to valid, NUL-terminated UTF-8 JSON strings.
pub unsafe extern "C" fn readio_restore_position(
    document_json: *const c_char,
    position_json: *const c_char,
) -> ReadioFfiResult {
    let document = unsafe { read_required_string(document_json, "document_json") };
    let position = unsafe { read_required_string(position_json, "position_json") };
    let result = document.and_then(|document| {
        position.and_then(|position| {
            readio_bridge::restore_position_json(&document, &position)
                .map_err(|error| error.to_string())
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Searches a Document IR using the shared folded-range implementation.
///
/// # Safety
///
/// Both arguments must point to valid, NUL-terminated UTF-8 JSON strings.
pub unsafe extern "C" fn readio_search_document(
    document_json: *const c_char,
    request_json: *const c_char,
) -> ReadioFfiResult {
    let document = unsafe { read_required_string(document_json, "document_json") };
    let request = unsafe { read_required_string(request_json, "request_json") };
    let result = document.and_then(|document| {
        request.and_then(|request| {
            readio_bridge::search_document_json(&document, &request)
                .map_err(|error| error.to_string())
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Applies shared same-offset bookmark semantics.
///
/// # Safety
///
/// All arguments must point to valid, NUL-terminated UTF-8 JSON strings.
pub unsafe extern "C" fn readio_upsert_bookmark(
    document_json: *const c_char,
    bookmarks_json: *const c_char,
    request_json: *const c_char,
) -> ReadioFfiResult {
    let document = unsafe { read_required_string(document_json, "document_json") };
    let bookmarks = unsafe { read_required_string(bookmarks_json, "bookmarks_json") };
    let request = unsafe { read_required_string(request_json, "request_json") };
    let result = document.and_then(|document| {
        bookmarks.and_then(|bookmarks| {
            request.and_then(|request| {
                readio_bridge::upsert_bookmark_json(&document, &bookmarks, &request)
                    .map_err(|error| error.to_string())
            })
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Computes a deterministic materialized-storage plan through the shared SDK.
///
/// # Safety
///
/// Both JSON arguments must point to valid, NUL-terminated UTF-8 strings.
pub unsafe extern "C" fn readio_plan_sync_push(
    snapshot_json: *const c_char,
    batch_json: *const c_char,
    server_timestamp_unix_ms: u64,
    starting_cursor: u64,
) -> ReadioFfiResult {
    let snapshot = unsafe { read_required_string(snapshot_json, "snapshot_json") };
    let batch = unsafe { read_required_string(batch_json, "batch_json") };
    let result = snapshot.and_then(|snapshot| {
        batch.and_then(|batch| {
            readio_bridge::plan_sync_push_json(
                &snapshot,
                &batch,
                server_timestamp_unix_ms,
                starting_cursor,
            )
            .map_err(|error| error.to_string())
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Applies one durable sync event and returns the next serialized Rust state.
///
/// # Safety
///
/// Both arguments must point to valid, NUL-terminated UTF-8 JSON strings.
pub unsafe extern "C" fn readio_sync_reduce(
    state_json: *const c_char,
    event_json: *const c_char,
) -> ReadioFfiResult {
    let state = unsafe { read_required_string(state_json, "state_json") };
    let event = unsafe { read_required_string(event_json, "event_json") };
    let result: Result<String, String> = state.and_then(|state| {
        event.and_then(|event| {
            let mut state: readio_sdk::sync::ReplicaSyncState = serde_json::from_str(&state)
                .map_err(|error| format!("invalid replica sync state JSON: {error}"))?;
            let event = serde_json::from_str(&event)
                .map_err(|error| format!("invalid replica sync event JSON: {error}"))?;
            state.apply(event).map_err(|error| error.to_string())?;
            serde_json::to_string(&state).map_err(|error| error.to_string())
        })
    });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Projects materialized state, outbox, snapshot, and heads from sync state.
///
/// # Safety
///
/// `state_json` must point to a valid, NUL-terminated UTF-8 JSON string.
pub unsafe extern "C" fn readio_sync_view(state_json: *const c_char) -> ReadioFfiResult {
    let result: Result<String, String> = unsafe { read_required_string(state_json, "state_json") }
        .and_then(|state| {
            let state: readio_sdk::sync::ReplicaSyncState = serde_json::from_str(&state)
                .map_err(|error| format!("invalid replica sync state JSON: {error}"))?;
            let view = state.view().map_err(|error| error.to_string())?;
            serde_json::to_string(&view).map_err(|error| error.to_string())
        });
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Queries the Rust-owned library projection from a serialized sync state.
///
/// # Safety
///
/// Both arguments must point to valid, NUL-terminated UTF-8 strings.
pub unsafe extern "C" fn readio_sync_query_library(
    state_json: *const c_char,
    query_json: *const c_char,
) -> ReadioFfiResult {
    let result: Result<String, String> = (|| {
        let state = unsafe { read_sync_state(state_json) }?;
        let query = unsafe { read_required_string(query_json, "query_json") }?;
        let query: readio_sdk::library::LibraryQuery = serde_json::from_str(&query)
            .map_err(|error| format!("invalid library query JSON: {error}"))?;
        let page = state
            .query_library(&query)
            .map_err(|error| error.to_string())?;
        serde_json::to_string(&page).map_err(|error| error.to_string())
    })();
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Returns one Rust-projected library metadata record or JSON `null`.
///
/// # Safety
///
/// Both arguments must point to valid, NUL-terminated UTF-8 strings.
pub unsafe extern "C" fn readio_sync_library_item(
    state_json: *const c_char,
    item_id: *const c_char,
) -> ReadioFfiResult {
    let result: Result<String, String> = (|| {
        let state = unsafe { read_sync_state(state_json) }?;
        let item_id = unsafe { read_required_string(item_id, "item_id") }?;
        let item = state
            .library_item(&item_id)
            .map_err(|error| error.to_string())?;
        serde_json::to_string(&item).map_err(|error| error.to_string())
    })();
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Returns the portable settings projection from a serialized sync state.
///
/// # Safety
///
/// `state_json` must point to a valid, NUL-terminated UTF-8 string.
pub unsafe extern "C" fn readio_sync_settings(state_json: *const c_char) -> ReadioFfiResult {
    let result: Result<String, String> = (|| {
        let state = unsafe { read_sync_state(state_json) }?;
        let settings = state.settings().map_err(|error| error.to_string())?;
        serde_json::to_string(&settings).map_err(|error| error.to_string())
    })();
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Returns bookmarks for one item, ordered by the shared Rust projection.
///
/// # Safety
///
/// Both arguments must point to valid, NUL-terminated UTF-8 strings.
pub unsafe extern "C" fn readio_sync_bookmarks(
    state_json: *const c_char,
    item_id: *const c_char,
) -> ReadioFfiResult {
    let result: Result<String, String> = (|| {
        let state = unsafe { read_sync_state(state_json) }?;
        let item_id = unsafe { read_required_string(item_id, "item_id") }?;
        let bookmarks = state
            .bookmarks_for_item(&item_id)
            .map_err(|error| error.to_string())?;
        serde_json::to_string(&bookmarks).map_err(|error| error.to_string())
    })();
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Returns one versioned reading position or JSON `null`.
///
/// # Safety
///
/// Both arguments must point to valid, NUL-terminated UTF-8 strings.
pub unsafe extern "C" fn readio_sync_reading_position(
    state_json: *const c_char,
    item_id: *const c_char,
) -> ReadioFfiResult {
    let result: Result<String, String> = (|| {
        let state = unsafe { read_sync_state(state_json) }?;
        let item_id = unsafe { read_required_string(item_id, "item_id") }?;
        let position = state
            .reading_position(&item_id)
            .map_err(|error| error.to_string())?;
        serde_json::to_string(&position).map_err(|error| error.to_string())
    })();
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Runs the Rust reading-position transition and returns its next state plus response.
///
/// # Safety
///
/// All arguments must point to valid, NUL-terminated UTF-8 strings.
pub unsafe extern "C" fn readio_sync_save_reading_position(
    state_json: *const c_char,
    item_id: *const c_char,
    mutation_json: *const c_char,
) -> ReadioFfiResult {
    let result: Result<String, String> = (|| {
        let mut state = unsafe { read_sync_state(state_json) }?;
        let item_id = unsafe { read_required_string(item_id, "item_id") }?;
        let mutation = unsafe { read_required_string(mutation_json, "mutation_json") }?;
        let mutation: readio_sdk::sync::ReadingPositionMutation =
            serde_json::from_str(&mutation)
                .map_err(|error| format!("invalid reading position mutation JSON: {error}"))?;
        let response = state
            .save_reading_position(&item_id, &mutation)
            .map_err(|error| error.to_string())?;
        let state_json = serde_json::to_string(&state).map_err(|error| error.to_string())?;
        serde_json::to_string(&serde_json::json!({
            "stateJSON": state_json,
            "response": response,
        }))
        .map_err(|error| error.to_string())
    })();
    ffi_result(result)
}

#[unsafe(no_mangle)]
pub extern "C" fn readio_replica_new() -> *mut ReadioReplicaHandle {
    Box::into_raw(Box::new(ReadioReplicaHandle {
        replica: readio_sdk::ReadioSdk::new().new_replica(),
    }))
}

#[unsafe(no_mangle)]
/// Restores a replica from a Base64-encoded Loro snapshot.
///
/// # Safety
///
/// `snapshot_base64` must point to a valid, NUL-terminated UTF-8 string.
pub unsafe extern "C" fn readio_replica_from_snapshot(
    snapshot_base64: *const c_char,
) -> ReadioFfiReplicaResult {
    let result: Result<readio_sdk::replica::ReadioReplica, String> = (|| {
        let snapshot = unsafe { read_required_string(snapshot_base64, "snapshot_base64") }?;
        let bytes = STANDARD
            .decode(snapshot)
            .map_err(|error| format!("invalid replica snapshot Base64: {error}"))?;
        let replica = readio_sdk::replica::ReadioReplica::from_snapshot(&bytes)
            .map_err(|error| error.to_string())?;
        Ok(replica)
    })();
    ffi_replica_result(result)
}

#[unsafe(no_mangle)]
/// Applies a local CRDT entity update and returns its Base64-encoded Loro change.
///
/// # Safety
///
/// `replica` must be a live handle returned by this library. String arguments
/// must point to valid, NUL-terminated UTF-8 strings.
pub unsafe extern "C" fn readio_replica_upsert(
    replica: *mut ReadioReplicaHandle,
    entity_kind: *const c_char,
    entity_id: *const c_char,
    payload_json: *const c_char,
) -> ReadioFfiResult {
    let result: Result<String, String> = (|| {
        let replica = unsafe { read_replica(replica) }?;
        let kind = unsafe { read_required_string(entity_kind, "entity_kind") }?;
        let entity_id = unsafe { read_required_string(entity_id, "entity_id") }?;
        let payload = unsafe { read_required_string(payload_json, "payload_json") }?;
        let payload = serde_json::from_str(&payload)
            .map_err(|error| format!("invalid replica JSON: {error}"))?;
        let change = replica
            .upsert(parse_replica_entity_kind(&kind)?, &entity_id, payload)
            .map_err(|error| error.to_string())?;
        Ok(STANDARD.encode(change))
    })();
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Imports a Base64-encoded Loro update into a live replica.
///
/// # Safety
///
/// `replica` must be a live handle returned by this library and
/// `change_base64` must be a valid, NUL-terminated UTF-8 string.
pub unsafe extern "C" fn readio_replica_import_change(
    replica: *mut ReadioReplicaHandle,
    change_base64: *const c_char,
) -> ReadioFfiResult {
    let result: Result<String, String> = (|| {
        let replica = unsafe { read_replica(replica) }?;
        let change = unsafe { read_required_string(change_base64, "change_base64") }?;
        let bytes = STANDARD
            .decode(change)
            .map_err(|error| format!("invalid replica change Base64: {error}"))?;
        replica.import(&bytes).map_err(|error| error.to_string())?;
        Ok("null".to_string())
    })();
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Returns a Base64-encoded compact Loro snapshot.
///
/// # Safety
///
/// `replica` must be a live handle returned by this library.
pub unsafe extern "C" fn readio_replica_snapshot(
    replica: *mut ReadioReplicaHandle,
) -> ReadioFfiResult {
    let result: Result<String, String> = (|| {
        let replica = unsafe { read_replica(replica) }?;
        let snapshot = replica.snapshot().map_err(|error| error.to_string())?;
        Ok(STANDARD.encode(snapshot))
    })();
    ffi_result(result)
}

#[unsafe(no_mangle)]
/// Returns the current Loro frontier as Base64 acknowledgement metadata.
///
/// # Safety
///
/// `replica` must be a live handle returned by this library.
pub unsafe extern "C" fn readio_replica_heads(
    replica: *mut ReadioReplicaHandle,
) -> ReadioFfiResult {
    ffi_result(unsafe { read_replica(replica) }.map(|replica| STANDARD.encode(replica.heads())))
}

#[unsafe(no_mangle)]
/// Returns the current materialized replica state as JSON.
///
/// # Safety
///
/// `replica` must be a live handle returned by this library.
pub unsafe extern "C" fn readio_replica_state(
    replica: *mut ReadioReplicaHandle,
) -> ReadioFfiResult {
    ffi_result(unsafe { read_replica(replica) }.map(|replica| replica.state().to_string()))
}

#[unsafe(no_mangle)]
/// Releases a live replica handle.
///
/// # Safety
///
/// `replica` must be null or a live handle returned by this library that has
/// not already been released.
pub unsafe extern "C" fn readio_replica_free(replica: *mut ReadioReplicaHandle) {
    if !replica.is_null() {
        unsafe {
            drop(Box::from_raw(replica));
        }
    }
}

#[unsafe(no_mangle)]
/// Releases a string returned in either field of [`ReadioFfiResult`].
///
/// # Safety
///
/// `pointer` must be null or a pointer returned by this library that has not
/// already been released. Passing memory owned by another allocator is invalid.
pub unsafe extern "C" fn readio_string_free(pointer: *mut c_char) {
    if !pointer.is_null() {
        unsafe {
            drop(CString::from_raw(pointer));
        }
    }
}

#[unsafe(no_mangle)]
/// Releases a byte buffer returned by [`readio_extract_epub_asset`].
///
/// # Safety
///
/// `pointer` and `length` must be the unchanged ownership tuple returned by
/// this library and must not have been released already.
pub unsafe extern "C" fn readio_bytes_free(pointer: *mut u8, length: usize) {
    if !pointer.is_null() {
        let slice = std::ptr::slice_from_raw_parts_mut(pointer, length);
        unsafe {
            drop(Box::from_raw(slice));
        }
    }
}

fn failure(message: &str) -> ReadioFfiResult {
    ReadioFfiResult {
        value: std::ptr::null_mut(),
        error: CString::new(message)
            .expect("static error has no NUL")
            .into_raw(),
    }
}

fn success(value: String) -> ReadioFfiResult {
    let value = CString::new(value).expect("JSON serialization escapes NUL bytes");
    ReadioFfiResult {
        value: value.into_raw(),
        error: std::ptr::null_mut(),
    }
}

fn ffi_result<E: std::fmt::Display>(result: Result<String, E>) -> ReadioFfiResult {
    match result {
        Ok(value) => success(value),
        Err(error) => failure(&error.to_string()),
    }
}

fn ffi_bytes_result<E: std::fmt::Display>(result: Result<Vec<u8>, E>) -> ReadioFfiBytesResult {
    match result {
        Ok(bytes) => {
            let length = bytes.len();
            let value = Box::into_raw(bytes.into_boxed_slice()).cast::<u8>();
            ReadioFfiBytesResult {
                value,
                length,
                error: std::ptr::null_mut(),
            }
        }
        Err(error) => ReadioFfiBytesResult {
            value: std::ptr::null_mut(),
            length: 0,
            error: CString::new(error.to_string())
                .unwrap_or_else(|_| {
                    CString::new("FFI byte extraction failed").expect("static error")
                })
                .into_raw(),
        },
    }
}

fn ffi_replica_result(
    result: Result<readio_sdk::replica::ReadioReplica, String>,
) -> ReadioFfiReplicaResult {
    match result {
        Ok(replica) => ReadioFfiReplicaResult {
            value: Box::into_raw(Box::new(ReadioReplicaHandle { replica })),
            error: std::ptr::null_mut(),
        },
        Err(error) => ReadioFfiReplicaResult {
            value: std::ptr::null_mut(),
            error: CString::new(error)
                .expect("replica errors have no NUL")
                .into_raw(),
        },
    }
}

unsafe fn read_replica<'a>(
    pointer: *mut ReadioReplicaHandle,
) -> Result<&'a readio_sdk::replica::ReadioReplica, String> {
    unsafe { pointer.as_ref() }
        .map(|handle| &handle.replica)
        .ok_or_else(|| "replica must not be null".to_string())
}

fn parse_replica_entity_kind(
    value: &str,
) -> Result<readio_sdk::replica::ReplicaEntityKind, String> {
    use readio_sdk::replica::ReplicaEntityKind;

    match value {
        "library_item" => Ok(ReplicaEntityKind::LibraryItem),
        "reading_position" => Ok(ReplicaEntityKind::ReadingPosition),
        "bookmark" => Ok(ReplicaEntityKind::Bookmark),
        "setting" => Ok(ReplicaEntityKind::Setting),
        _ => Err(format!("unsupported replica entity kind: {value}")),
    }
}

unsafe fn read_sync_state(
    pointer: *const c_char,
) -> Result<readio_sdk::sync::ReplicaSyncState, String> {
    let state = unsafe { read_required_string(pointer, "state_json") }?;
    serde_json::from_str(&state)
        .map_err(|error| format!("invalid replica sync state JSON: {error}"))
}

unsafe fn read_optional_string(pointer: *const c_char) -> Result<Option<String>, String> {
    if pointer.is_null() {
        return Ok(None);
    }
    unsafe { read_string(pointer) }.map(Some)
}

unsafe fn read_required_string(pointer: *const c_char, field: &str) -> Result<String, String> {
    if pointer.is_null() {
        return Err(format!("{field} must not be null"));
    }
    unsafe { read_string(pointer) }
}

unsafe fn read_string(pointer: *const c_char) -> Result<String, String> {
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .map(str::to_owned)
        .map_err(|error| format!("FFI input is not UTF-8: {error}"))
}

unsafe fn read_bytes<'a>(pointer: *const u8, length: usize) -> Result<&'a [u8], String> {
    if length == 0 {
        return Ok(&[]);
    }
    if pointer.is_null() {
        return Err("bytes must not be null when length is non-zero".to_string());
    }
    Ok(unsafe { std::slice::from_raw_parts(pointer, length) })
}
