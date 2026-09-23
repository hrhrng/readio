//! The product-facing Rust SDK shared by Readio's native, WebAssembly, TUI,
//! and Cloudflare adapters.
//!
//! Platform bindings depend on this crate rather than reaching into domain
//! modules directly. The SDK owns product semantics; adapters own physical I/O.

pub mod replica;
pub mod sync;

#[cfg(feature = "native-media")]
pub use readio_core::media;
pub use readio_core::{
    bookmarks, diagnostics, documents, highlight, imports, library, playback, position, reader, reading,
    samples, search, storage, tts,
};

use bookmarks::{Bookmark, BookmarkError, BookmarkTransition, BookmarkUpsertRequest};
use documents::{Document, DocumentError, HtmlParseOptions};
use highlight::{DocumentTextIndex, HighlightCursor, HighlightError, HighlightTarget};
use playback::{PlaybackEvent, PlaybackPolicy, PlaybackState, PlaybackTransition};
use position::{
    Affinity, LocatorError, LocatorResolution, PositionError, PositionProjection, ReadingPosition,
    RestoredPosition, TextLocator,
};
use reading::{
    ReadingPace, ReadingPreferences, ReadingProgressProjection, ReadingProgressRequest,
    ReadingSeekCursor, TextAdvanceEvent, TextAdvanceState, TextAdvanceTransition,
};
use replica::ReadioReplica;
use search::{SearchError, SearchRequest, SearchResultPage};
use storage::{StorageError, StoragePlan, StorageSnapshot, SyncPushBatch};

/// Stable entry point for every Readio product surface.
///
/// The façade is intentionally stateless. Stateful, local-first data lives in
/// [`ReadioReplica`], while audio, databases, files, clocks, and HTTP remain
/// injected platform capabilities.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReadioSdk {
    pub diagnostics: DiagnosticsSdk,
    pub documents: DocumentsSdk,
    pub imports: ImportsSdk,
    pub playback: PlaybackSdk,
    pub reading: ReadingSdk,
    pub samples: SamplesSdk,
    pub storage: StorageSdk,
    pub tts: TtsSdk,
    #[cfg(feature = "native-media")]
    pub media: MediaSdk,
}

impl ReadioSdk {
    pub const fn new() -> Self {
        Self {
            diagnostics: DiagnosticsSdk,
            documents: DocumentsSdk,
            imports: ImportsSdk,
            playback: PlaybackSdk,
            reading: ReadingSdk,
            samples: SamplesSdk,
            storage: StorageSdk,
            tts: TtsSdk,
            #[cfg(feature = "native-media")]
            media: MediaSdk,
        }
    }

    /// Starts an independent local-first replica with a fresh Loro peer ID.
    pub fn new_replica(self) -> ReadioReplica {
        ReadioReplica::new()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DiagnosticsSdk;

impl DiagnosticsSdk {
    pub fn build(
        self,
        request: diagnostics::SupportBundleRequest,
    ) -> diagnostics::SupportBundle {
        diagnostics::build_support_bundle(request)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TtsSdk;

impl TtsSdk {
    pub fn normalize_catalog(
        self,
        catalog: tts::TtsModelCatalog,
    ) -> Result<tts::TtsModelCatalog, tts::TtsCatalogError> {
        tts::normalize_tts_catalog(catalog)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SamplesSdk;

impl SamplesSdk {
    pub fn resolve(self, language_tag: &str) -> samples::BuiltinSample {
        samples::builtin_sample(language_tag)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ImportsSdk;

impl ImportsSdk {
    pub fn content_identity(self, bytes: &[u8]) -> documents::ContentIdentity {
        imports::content_identity(bytes)
    }

    pub fn plan(self, request: imports::ImportRequest) -> imports::ImportPlan {
        imports::plan_import(request)
    }

    pub fn forget(self, record: &imports::ImportRecord) -> Vec<imports::ImportEffect> {
        imports::plan_forget(record)
    }
}

#[cfg(feature = "native-media")]
#[derive(Clone, Copy, Debug, Default)]
pub struct MediaSdk;

#[cfg(feature = "native-media")]
impl MediaSdk {
    pub fn parse_epub(self, bytes: &[u8]) -> Result<media::ParsedEpub, media::MediaError> {
        media::parse_epub(bytes)
    }

    pub fn extract_epub_asset(
        self,
        bytes: &[u8],
        asset_id: &str,
    ) -> Result<Vec<u8>, media::MediaError> {
        media::extract_epub_asset(bytes, asset_id)
    }

    pub fn parse_pdf(self, bytes: &[u8]) -> Result<media::ParsedPdf, media::MediaError> {
        media::parse_pdf(bytes)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DocumentsSdk;

impl DocumentsSdk {
    pub fn parse_markdown_reader(
        self,
        markdown: &str,
        options: &HtmlParseOptions,
    ) -> Result<reader::ReaderIr, DocumentError> {
        reader::parse_markdown_reader(markdown, options)
    }

    pub fn parse_plain_text_reader(
        self,
        text: &str,
        options: &HtmlParseOptions,
    ) -> Result<reader::ReaderIr, DocumentError> {
        reader::parse_plain_text_reader(text, options)
    }

    pub fn parse_html_reader(
        self,
        html: &str,
        options: &HtmlParseOptions,
    ) -> Result<reader::ReaderIr, DocumentError> {
        reader::parse_html_reader(html, options)
    }

    pub fn parse_html(
        self,
        html: &str,
        options: &HtmlParseOptions,
    ) -> Result<Document, DocumentError> {
        documents::parse_html(html, options)
    }

    pub fn project_offset(
        self,
        document: &Document,
        char_offset: u64,
        affinity: Affinity,
    ) -> PositionProjection {
        position::project_offset(document, char_offset, affinity)
    }

    pub fn create_text_locator(
        self,
        document: &Document,
        char_offset: u64,
        affinity: Affinity,
    ) -> TextLocator {
        position::create_text_locator(document, char_offset, affinity)
    }

    pub fn restore_text_locator(
        self,
        locator: &TextLocator,
        document: &Document,
    ) -> Result<LocatorResolution, LocatorError> {
        position::restore_text_locator(locator, document)
    }

    pub fn index_text(self, document: &Document) -> DocumentTextIndex {
        highlight::index_document(document)
    }

    pub fn resolve_highlight(
        self,
        document: &Document,
        index: &DocumentTextIndex,
        cursor: HighlightCursor,
    ) -> Result<HighlightTarget, HighlightError> {
        highlight::resolve_highlight(document, index, cursor)
    }

    pub fn resolve_block_offset(
        self,
        document: &Document,
        block_id: &str,
        offset_in_block: u64,
        affinity: Affinity,
    ) -> Result<PositionProjection, PositionError> {
        position::resolve_block_offset(document, block_id, offset_in_block, affinity)
    }

    pub fn restore_position(
        self,
        position: &ReadingPosition,
        document: &Document,
    ) -> RestoredPosition {
        position::restore_position(position, document)
    }

    pub fn search(
        self,
        document: &Document,
        request: SearchRequest,
    ) -> Result<SearchResultPage, SearchError> {
        search::search_document(document, request)
    }

    pub fn upsert_bookmark(
        self,
        document: &Document,
        bookmarks: &[Bookmark],
        request: BookmarkUpsertRequest,
    ) -> Result<BookmarkTransition, BookmarkError> {
        bookmarks::upsert_bookmark(document, bookmarks, request)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PlaybackSdk;

impl PlaybackSdk {
    pub fn create_state(self, sentence_count: usize) -> PlaybackState {
        PlaybackState::new(sentence_count)
    }

    pub fn reduce(
        self,
        policy: PlaybackPolicy,
        state: PlaybackState,
        event: PlaybackEvent,
    ) -> PlaybackTransition {
        playback::reduce(&policy, state, event)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ReadingSdk;

impl ReadingSdk {
    pub fn resolve(self, preferences: ReadingPreferences) -> ReadingPace {
        reading::resolve_reading_pace(&preferences)
    }

    pub fn create_text_advance_state(self, sentence_count: usize) -> TextAdvanceState {
        TextAdvanceState::new(sentence_count)
    }

    pub fn project_progress(self, request: ReadingProgressRequest) -> ReadingProgressProjection {
        reading::project_reading_progress(request)
    }

    pub fn resolve_seek(
        self,
        segment_character_counts: &[u64],
        progress_basis_points: u16,
    ) -> ReadingSeekCursor {
        reading::resolve_reading_seek(segment_character_counts, progress_basis_points)
    }

    pub fn reduce_text_advance(
        self,
        state: TextAdvanceState,
        event: TextAdvanceEvent,
    ) -> TextAdvanceTransition {
        reading::reduce_text_advance(state, event)
    }
}

/// Cloud/local storage adapters execute this deterministic plan with their own
/// transaction primitive. User-visible sync convergence happens in Loro; this
/// planner is the persistence boundary for materialized projections.
#[derive(Clone, Copy, Debug, Default)]
pub struct StorageSdk;

impl StorageSdk {
    pub fn plan_sync_push(
        self,
        snapshot: &StorageSnapshot,
        batch: &SyncPushBatch,
        server_timestamp_unix_ms: u64,
        starting_cursor: u64,
    ) -> Result<StoragePlan, StorageError> {
        storage::plan_sync_push(snapshot, batch, server_timestamp_unix_ms, starting_cursor)
    }
}
