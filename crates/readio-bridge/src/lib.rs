#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeError {
    message: String,
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for BridgeError {}

pub fn builtin_sample_json(language_tag: &str) -> Result<String, BridgeError> {
    encode(&ReadioSdk::new().samples.resolve(language_tag))
}

pub fn build_support_bundle_json(request_json: &str) -> Result<String, BridgeError> {
    let request: readio_sdk::diagnostics::SupportBundleRequest = decode(request_json)?;
    encode(&ReadioSdk::new().diagnostics.build(request))
}

pub fn normalize_tts_catalog_json(catalog_json: &str) -> Result<String, BridgeError> {
    let catalog: readio_sdk::tts::TtsModelCatalog = decode(catalog_json)?;
    let normalized = ReadioSdk::new()
        .tts
        .normalize_catalog(catalog)
        .map_err(|error| domain_error(error.to_string()))?;
    encode(&normalized)
}

pub fn create_playback_state_json(sentence_count: usize) -> Result<String, BridgeError> {
    encode(&ReadioSdk::new().playback.create_state(sentence_count))
}

pub fn reduce_playback_json(
    policy_json: Option<&str>,
    state_json: &str,
    event_json: &str,
) -> Result<String, BridgeError> {
    let policy: PlaybackPolicy = decode_or_default(policy_json)?;
    let state: PlaybackState = decode(state_json)?;
    let event: PlaybackEvent = decode(event_json)?;
    encode(&ReadioSdk::new().playback.reduce(policy, state, event))
}

pub fn resolve_reading_pace_json(preferences_json: &str) -> Result<String, BridgeError> {
    let preferences = decode(preferences_json)?;
    encode(&ReadioSdk::new().reading.resolve(preferences))
}

pub fn project_reading_progress_json(request_json: &str) -> Result<String, BridgeError> {
    let request = decode(request_json)?;
    encode(&ReadioSdk::new().reading.project_progress(request))
}

#[derive(Debug, serde::Deserialize)]
struct ReadingSeekRequest {
    segment_character_counts: Vec<u64>,
    progress_basis_points: u16,
}

pub fn resolve_reading_seek_json(request_json: &str) -> Result<String, BridgeError> {
    let request: ReadingSeekRequest = decode(request_json)?;
    encode(&ReadioSdk::new().reading.resolve_seek(
        &request.segment_character_counts,
        request.progress_basis_points,
    ))
}

pub fn create_text_advance_state_json(sentence_count: usize) -> Result<String, BridgeError> {
    encode(
        &ReadioSdk::new()
            .reading
            .create_text_advance_state(sentence_count),
    )
}

pub fn reduce_text_advance_json(state_json: &str, event_json: &str) -> Result<String, BridgeError> {
    let state: TextAdvanceState = decode(state_json)?;
    let event: TextAdvanceEvent = decode(event_json)?;
    encode(&ReadioSdk::new().reading.reduce_text_advance(state, event))
}

pub fn content_identity_json(bytes: &[u8]) -> Result<String, BridgeError> {
    encode(&ReadioSdk::new().imports.content_identity(bytes))
}

pub fn plan_import_json(request_json: &str) -> Result<String, BridgeError> {
    let request: readio_sdk::imports::ImportRequest = decode(request_json)?;
    encode(&ReadioSdk::new().imports.plan(request))
}

pub fn plan_forget_json(record_json: &str) -> Result<String, BridgeError> {
    let record: readio_sdk::imports::ImportRecord = decode(record_json)?;
    encode(&ReadioSdk::new().imports.forget(&record))
}

pub fn parse_html_json(html: &str, options_json: Option<&str>) -> Result<String, BridgeError> {
    let options: HtmlParseOptions = decode_or_default(options_json)?;
    let document = ReadioSdk::new()
        .documents
        .parse_html(html, &options)
        .map_err(document_error)?;
    encode(&document)
}

pub fn parse_html_reader_json(
    html: &str,
    options_json: Option<&str>,
) -> Result<String, BridgeError> {
    let options: HtmlParseOptions = decode_or_default(options_json)?;
    let reader = ReadioSdk::new()
        .documents
        .parse_html_reader(html, &options)
        .map_err(document_error)?;
    encode(&reader)
}

pub fn parse_markdown_reader_json(
    markdown: &str,
    options_json: Option<&str>,
) -> Result<String, BridgeError> {
    let options: HtmlParseOptions = decode_or_default(options_json)?;
    let reader = ReadioSdk::new()
        .documents
        .parse_markdown_reader(markdown, &options)
        .map_err(document_error)?;
    encode(&reader)
}

pub fn parse_plain_text_reader_json(
    text: &str,
    options_json: Option<&str>,
) -> Result<String, BridgeError> {
    let options: HtmlParseOptions = decode_or_default(options_json)?;
    let reader = ReadioSdk::new()
        .documents
        .parse_plain_text_reader(text, &options)
        .map_err(document_error)?;
    encode(&reader)
}

#[derive(Debug, serde::Deserialize)]
struct OffsetRequest {
    char_offset: u64,
    affinity: Affinity,
}

#[derive(Debug, serde::Deserialize)]
struct BlockOffsetRequest {
    block_id: String,
    offset_in_block: u64,
    affinity: Affinity,
}

pub fn project_offset_json(document_json: &str, request_json: &str) -> Result<String, BridgeError> {
    let document = decode(document_json)?;
    let request: OffsetRequest = decode(request_json)?;
    encode(&ReadioSdk::new().documents.project_offset(
        &document,
        request.char_offset,
        request.affinity,
    ))
}

pub fn create_text_locator_json(
    document_json: &str,
    request_json: &str,
) -> Result<String, BridgeError> {
    let document = decode(document_json)?;
    let request: OffsetRequest = decode(request_json)?;
    encode(&ReadioSdk::new().documents.create_text_locator(
        &document,
        request.char_offset,
        request.affinity,
    ))
}

pub fn restore_text_locator_json(
    document_json: &str,
    locator_json: &str,
) -> Result<String, BridgeError> {
    let document = decode(document_json)?;
    let locator: readio_sdk::position::TextLocator = decode(locator_json)?;
    let restored = ReadioSdk::new()
        .documents
        .restore_text_locator(&locator, &document)
        .map_err(|error| domain_error(error.to_string()))?;
    encode(&restored)
}

pub fn index_document_text_json(document_json: &str) -> Result<String, BridgeError> {
    let document = decode(document_json)?;
    encode(&ReadioSdk::new().documents.index_text(&document))
}

pub fn resolve_highlight_json(
    document_json: &str,
    index_json: &str,
    cursor_json: &str,
) -> Result<String, BridgeError> {
    let document = decode(document_json)?;
    let index: readio_sdk::highlight::DocumentTextIndex = decode(index_json)?;
    let cursor: readio_sdk::highlight::HighlightCursor = decode(cursor_json)?;
    let target = ReadioSdk::new()
        .documents
        .resolve_highlight(&document, &index, cursor)
        .map_err(|error| domain_error(error.to_string()))?;
    encode(&target)
}

pub fn resolve_block_offset_json(
    document_json: &str,
    request_json: &str,
) -> Result<String, BridgeError> {
    let document = decode(document_json)?;
    let request: BlockOffsetRequest = decode(request_json)?;
    let projection = ReadioSdk::new()
        .documents
        .resolve_block_offset(
            &document,
            &request.block_id,
            request.offset_in_block,
            request.affinity,
        )
        .map_err(|error| domain_error(error.to_string()))?;
    encode(&projection)
}

pub fn restore_position_json(
    document_json: &str,
    position_json: &str,
) -> Result<String, BridgeError> {
    let document = decode(document_json)?;
    let position: ReadingPosition = decode(position_json)?;
    encode(
        &ReadioSdk::new()
            .documents
            .restore_position(&position, &document),
    )
}

pub fn search_document_json(
    document_json: &str,
    request_json: &str,
) -> Result<String, BridgeError> {
    let document = decode(document_json)?;
    let request = decode(request_json)?;
    let result = ReadioSdk::new()
        .documents
        .search(&document, request)
        .map_err(|error| domain_error(error.to_string()))?;
    encode(&result)
}

pub fn upsert_bookmark_json(
    document_json: &str,
    bookmarks_json: &str,
    request_json: &str,
) -> Result<String, BridgeError> {
    let document = decode(document_json)?;
    let bookmarks: Vec<Bookmark> = decode(bookmarks_json)?;
    let request = decode(request_json)?;
    let result = ReadioSdk::new()
        .documents
        .upsert_bookmark(&document, &bookmarks, request)
        .map_err(|error| domain_error(error.to_string()))?;
    encode(&result)
}

pub fn plan_sync_push_json(
    snapshot_json: &str,
    batch_json: &str,
    server_timestamp_unix_ms: u64,
    starting_cursor: u64,
) -> Result<String, BridgeError> {
    let snapshot = decode(snapshot_json)?;
    let batch = decode(batch_json)?;
    let plan = ReadioSdk::new()
        .storage
        .plan_sync_push(&snapshot, &batch, server_timestamp_unix_ms, starting_cursor)
        .map_err(|error| domain_error(error.to_string()))?;
    encode(&plan)
}

fn encode<T: serde::Serialize>(value: &T) -> Result<String, BridgeError> {
    serde_json::to_string(value).map_err(json_error)
}

fn decode<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, BridgeError> {
    serde_json::from_str(json).map_err(json_error)
}

fn decode_or_default<T: serde::de::DeserializeOwned + Default>(
    json: Option<&str>,
) -> Result<T, BridgeError> {
    match json {
        Some(json) => decode(json),
        None => Ok(T::default()),
    }
}

fn json_error(error: serde_json::Error) -> BridgeError {
    BridgeError {
        message: format!("invalid bridge JSON: {error}"),
    }
}

fn domain_error(message: String) -> BridgeError {
    BridgeError { message }
}

fn document_error(error: DocumentError) -> BridgeError {
    let message = match error {
        DocumentError::InputTooLarge { actual, limit } => {
            format!("document input uses {actual} bytes; limit is {limit}")
        }
        DocumentError::TooManyBlocks { limit } => {
            format!("document exceeds the {limit}-block limit")
        }
    };
    BridgeError { message }
}
use readio_sdk::{
    ReadioSdk,
    bookmarks::Bookmark,
    documents::{DocumentError, HtmlParseOptions},
    playback::{PlaybackEvent, PlaybackPolicy, PlaybackState},
    position::{Affinity, ReadingPosition},
    reading::{TextAdvanceEvent, TextAdvanceState},
};
