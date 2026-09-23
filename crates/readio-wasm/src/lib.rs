#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn builtin_sample(language_tag: String) -> Result<String, String> {
    readio_bridge::builtin_sample_json(&language_tag).map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn build_support_bundle(request_json: String) -> Result<String, String> {
    readio_bridge::build_support_bundle_json(&request_json).map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn normalize_tts_model_catalog(catalog_json: String) -> Result<String, String> {
    readio_bridge::normalize_tts_catalog_json(&catalog_json).map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn create_playback_state(sentence_count: u32) -> Result<String, String> {
    readio_bridge::create_playback_state_json(sentence_count as usize)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn reduce_playback(
    policy_json: Option<String>,
    state_json: String,
    event_json: String,
) -> Result<String, String> {
    readio_bridge::reduce_playback_json(policy_json.as_deref(), &state_json, &event_json)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn resolve_reading_pace(preferences_json: String) -> Result<String, String> {
    readio_bridge::resolve_reading_pace_json(&preferences_json).map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn project_reading_progress(request_json: String) -> Result<String, String> {
    readio_bridge::project_reading_progress_json(&request_json).map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn resolve_reading_seek(request_json: String) -> Result<String, String> {
    readio_bridge::resolve_reading_seek_json(&request_json).map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn create_text_advance_state(sentence_count: u32) -> Result<String, String> {
    readio_bridge::create_text_advance_state_json(sentence_count as usize)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn reduce_text_advance(state_json: String, event_json: String) -> Result<String, String> {
    readio_bridge::reduce_text_advance_json(&state_json, &event_json)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn content_identity(bytes: &[u8]) -> Result<String, String> {
    readio_bridge::content_identity_json(bytes).map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn plan_import(request_json: String) -> Result<String, String> {
    readio_bridge::plan_import_json(&request_json).map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn plan_forget(record_json: String) -> Result<String, String> {
    readio_bridge::plan_forget_json(&record_json).map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn parse_html_document(html: String, options_json: Option<String>) -> Result<String, String> {
    readio_bridge::parse_html_json(&html, options_json.as_deref())
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn parse_html_reader(html: String, options_json: Option<String>) -> Result<String, String> {
    readio_bridge::parse_html_reader_json(&html, options_json.as_deref())
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn parse_markdown_reader(
    markdown: String,
    options_json: Option<String>,
) -> Result<String, String> {
    readio_bridge::parse_markdown_reader_json(&markdown, options_json.as_deref())
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn parse_plain_text_reader(
    text: String,
    options_json: Option<String>,
) -> Result<String, String> {
    readio_bridge::parse_plain_text_reader_json(&text, options_json.as_deref())
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn parse_pdf(bytes: &[u8]) -> Result<String, String> {
    let parsed = readio_sdk::ReadioSdk::new()
        .media
        .parse_pdf(bytes)
        .map_err(|error| error.to_string())?;
    serde_json::to_string(&parsed).map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn parse_epub(bytes: &[u8]) -> Result<String, String> {
    let parsed = readio_sdk::ReadioSdk::new()
        .media
        .parse_epub(bytes)
        .map_err(|error| error.to_string())?;
    serde_json::to_string(&parsed).map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn extract_epub_asset(bytes: &[u8], asset_id: String) -> Result<Vec<u8>, String> {
    readio_sdk::ReadioSdk::new()
        .media
        .extract_epub_asset(bytes, &asset_id)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn project_offset(document_json: String, request_json: String) -> Result<String, String> {
    readio_bridge::project_offset_json(&document_json, &request_json)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn create_text_locator(document_json: String, request_json: String) -> Result<String, String> {
    readio_bridge::create_text_locator_json(&document_json, &request_json)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn restore_text_locator(document_json: String, locator_json: String) -> Result<String, String> {
    readio_bridge::restore_text_locator_json(&document_json, &locator_json)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn index_document_text(document_json: String) -> Result<String, String> {
    readio_bridge::index_document_text_json(&document_json).map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn resolve_highlight(
    document_json: String,
    index_json: String,
    cursor_json: String,
) -> Result<String, String> {
    readio_bridge::resolve_highlight_json(&document_json, &index_json, &cursor_json)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn resolve_block_offset(document_json: String, request_json: String) -> Result<String, String> {
    readio_bridge::resolve_block_offset_json(&document_json, &request_json)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn restore_position(document_json: String, position_json: String) -> Result<String, String> {
    readio_bridge::restore_position_json(&document_json, &position_json)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn search_document(document_json: String, request_json: String) -> Result<String, String> {
    readio_bridge::search_document_json(&document_json, &request_json)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn upsert_bookmark(
    document_json: String,
    bookmarks_json: String,
    request_json: String,
) -> Result<String, String> {
    readio_bridge::upsert_bookmark_json(&document_json, &bookmarks_json, &request_json)
        .map_err(|error| error.to_string())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn plan_sync_push(
    snapshot_json: String,
    batch_json: String,
    server_timestamp_unix_ms: u64,
    starting_cursor: u64,
) -> Result<String, String> {
    readio_bridge::plan_sync_push_json(
        &snapshot_json,
        &batch_json,
        server_timestamp_unix_ms,
        starting_cursor,
    )
    .map_err(|error| error.to_string())
}

/// Stateful Loro replica used by the browser SDK. Desktop and TUI use the same
/// `readio_sdk::replica::ReadioReplica` directly through their native bindings.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub struct ReadioReplicaWasm {
    inner: readio_sdk::replica::ReadioReplica,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl ReadioReplicaWasm {
    #[cfg_attr(
        target_arch = "wasm32",
        wasm_bindgen::prelude::wasm_bindgen(constructor)
    )]
    pub fn new() -> Self {
        Self {
            inner: readio_sdk::ReadioSdk::new().new_replica(),
        }
    }

    pub fn from_snapshot(snapshot: &[u8]) -> Result<ReadioReplicaWasm, String> {
        readio_sdk::replica::ReadioReplica::from_snapshot(snapshot)
            .map(|inner| Self { inner })
            .map_err(|error| error.to_string())
    }

    pub fn upsert(
        &self,
        entity_kind: String,
        entity_id: String,
        payload_json: String,
    ) -> Result<Vec<u8>, String> {
        let kind = parse_replica_entity_kind(&entity_kind)?;
        let payload = serde_json::from_str(&payload_json)
            .map_err(|error| format!("invalid replica JSON: {error}"))?;
        self.inner
            .upsert(kind, &entity_id, payload)
            .map_err(|error| error.to_string())
    }

    pub fn delete_entity(&self, entity_kind: String, entity_id: String) -> Result<Vec<u8>, String> {
        self.inner
            .delete(parse_replica_entity_kind(&entity_kind)?, &entity_id)
            .map_err(|error| error.to_string())
    }

    pub fn import_change(&self, change: &[u8]) -> Result<(), String> {
        self.inner.import(change).map_err(|error| error.to_string())
    }

    pub fn snapshot(&self) -> Result<Vec<u8>, String> {
        self.inner.snapshot().map_err(|error| error.to_string())
    }

    pub fn heads(&self) -> Vec<u8> {
        self.inner.heads()
    }

    pub fn state_json(&self) -> String {
        self.inner.state().to_string()
    }
}

impl Default for ReadioReplicaWasm {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable Loro snapshot/outbox state. Browser storage and HTTP remain
/// JavaScript effects while every transition stays in the shared Rust SDK.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub struct ReadioSyncStateWasm {
    inner: readio_sdk::sync::ReplicaSyncState,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl ReadioSyncStateWasm {
    #[cfg_attr(
        target_arch = "wasm32",
        wasm_bindgen::prelude::wasm_bindgen(constructor)
    )]
    pub fn new() -> Self {
        Self {
            inner: readio_sdk::sync::ReplicaSyncState::default(),
        }
    }

    pub fn from_json(state_json: String) -> Result<ReadioSyncStateWasm, String> {
        serde_json::from_str(&state_json)
            .map(|inner| Self { inner })
            .map_err(|error| format!("invalid replica sync state JSON: {error}"))
    }

    pub fn stage_local(&mut self, entities_json: String) -> Result<(), String> {
        let entities: Vec<readio_sdk::sync::LocalReplicaEntity> =
            serde_json::from_str(&entities_json)
                .map_err(|error| format!("invalid local replica entities JSON: {error}"))?;
        self.inner
            .stage_local(&entities)
            .map_err(|error| error.to_string())
    }

    pub fn delete_local(&mut self, entities_json: String) -> Result<(), String> {
        let entities: Vec<readio_sdk::sync::LocalReplicaEntityRef> =
            serde_json::from_str(&entities_json)
                .map_err(|error| format!("invalid local replica entity refs JSON: {error}"))?;
        self.inner
            .delete_local(&entities)
            .map_err(|error| error.to_string())
    }

    pub fn import_remote(&mut self, changes_json: String) -> Result<(), String> {
        let changes: Vec<readio_sdk::sync::PendingReplicaChange> =
            serde_json::from_str(&changes_json)
                .map_err(|error| format!("invalid remote replica changes JSON: {error}"))?;
        self.inner
            .import_remote(&changes)
            .map_err(|error| error.to_string())
    }

    pub fn acknowledge(&mut self, change_ids_json: String) -> Result<(), String> {
        let change_ids: Vec<String> = serde_json::from_str(&change_ids_json)
            .map_err(|error| format!("invalid replica acknowledgement JSON: {error}"))?;
        self.inner.acknowledge(&change_ids);
        Ok(())
    }

    pub fn json(&self) -> Result<String, String> {
        serde_json::to_string(&self.inner).map_err(|error| error.to_string())
    }

    pub fn outbox_json(&self) -> Result<String, String> {
        serde_json::to_string(self.inner.outbox()).map_err(|error| error.to_string())
    }

    pub fn snapshot(&self) -> Result<Vec<u8>, String> {
        self.inner
            .snapshot_bytes()
            .map_err(|error| error.to_string())
    }

    pub fn heads(&self) -> Result<Vec<u8>, String> {
        self.inner.heads().map_err(|error| error.to_string())
    }

    pub fn state_json(&self) -> Result<String, String> {
        self.inner
            .materialized_state()
            .map(|state| state.to_string())
            .map_err(|error| error.to_string())
    }

    pub fn bookmarks_json(&self, item_id: String) -> Result<String, String> {
        let bookmarks = self
            .inner
            .bookmarks_for_item(&item_id)
            .map_err(|error| error.to_string())?;
        serde_json::to_string(&bookmarks).map_err(|error| error.to_string())
    }

    pub fn settings_json(&self) -> Result<String, String> {
        let settings = self.inner.settings().map_err(|error| error.to_string())?;
        serde_json::to_string(&settings).map_err(|error| error.to_string())
    }

    pub fn query_library_json(&self, query_json: String) -> Result<String, String> {
        let query: readio_sdk::library::LibraryQuery = serde_json::from_str(&query_json)
            .map_err(|error| format!("invalid library query JSON: {error}"))?;
        let page = self
            .inner
            .query_library(&query)
            .map_err(|error| error.to_string())?;
        serde_json::to_string(&page).map_err(|error| error.to_string())
    }

    pub fn library_item_json(&self, item_id: String) -> Result<String, String> {
        let item = self
            .inner
            .library_item(&item_id)
            .map_err(|error| error.to_string())?;
        serde_json::to_string(&item).map_err(|error| error.to_string())
    }

    pub fn reading_position_json(&self, item_id: String) -> Result<String, String> {
        let position = self
            .inner
            .reading_position(&item_id)
            .map_err(|error| error.to_string())?;
        serde_json::to_string(&position).map_err(|error| error.to_string())
    }

    pub fn save_reading_position_json(
        &mut self,
        item_id: String,
        mutation_json: String,
    ) -> Result<String, String> {
        let mutation: readio_sdk::sync::ReadingPositionMutation =
            serde_json::from_str(&mutation_json)
                .map_err(|error| format!("invalid reading position mutation JSON: {error}"))?;
        let response = self
            .inner
            .save_reading_position(&item_id, &mutation)
            .map_err(|error| error.to_string())?;
        serde_json::to_string(&response).map_err(|error| error.to_string())
    }
}

impl Default for ReadioSyncStateWasm {
    fn default() -> Self {
        Self::new()
    }
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
