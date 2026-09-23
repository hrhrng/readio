/** Portable typed facade over the generated Readio WASM exports. */
export type PlaybackStatus =
  | "idle"
  | "loading"
  | "playing"
  | "paused"
  | "failed"
  | "completed"

export interface TTSModelVoice {
  id: string
  label: string
}

export interface TTSModel {
  id: string
  provider: string
  label: string
  languages: string[]
  voices: TTSModelVoice[]
  defaultVoiceId: string
}

export interface TTSModelCatalog {
  schemaVersion: 1
  defaultModelId: string
  models: TTSModel[]
}

export type DiagnosticCategory =
  | "import"
  | "parsing"
  | "sync"
  | "tts"
  | "fallback"
  | "playback"

export type DiagnosticFieldValue = string | number | boolean | null

export interface DiagnosticEventInput {
  timestamp_unix_ms: number
  operation_id: string
  component: string
  category: DiagnosticCategory
  name: string
  fields: Record<string, DiagnosticFieldValue>
}

export interface SupportBundleRequest {
  generated_at_unix_ms: number
  platform: string
  app_version: string
  events: DiagnosticEventInput[]
}

export interface DiagnosticEvent extends DiagnosticEventInput {}

export interface SupportBundle {
  schema_version: 1
  generated_at_unix_ms: number
  platform: string
  app_version: string
  redaction: {
    content_included: false
    secrets_included: false
    dropped_field_count: number
    dropped_event_count: number
  }
  events: DiagnosticEvent[]
}

export interface PlaybackState {
  status: PlaybackStatus
  current_sentence: number
  sentence_count: number
  generation: number
  current_duration_ms: number | null
  current_position_ms: number
  last_tick_monotonic_ms: number | null
  sleep_deadline_unix_ms: number | null
  error: string | null
}

export interface PlaybackPolicy {
  tick_interval_ms: number
  prefetch_window: number
}

export type PlaybackEvent =
  | { type: "play_requested" }
  | { type: "audio_source_changed" }
  | {
      type: "seek_requested"
      sentence: number
      position_ms: number
      autoplay: boolean
    }
  | { type: "pause_requested"; audio_position_ms: number }
  | {
      type: "audio_ready"
      sentence: number
      generation: number
      duration_ms: number
    }
  | {
      type: "audio_failed"
      sentence: number
      generation: number
      message: string
    }
  | {
      type: "clock_tick"
      generation: number
      monotonic_ms: number
      unix_ms: number
      audio_position_ms: number
    }
  | { type: "sleep_timer_set"; deadline_unix_ms: number }
  | { type: "sleep_timer_cleared" }
  | { type: "audio_ended"; sentence: number; generation: number }

export type PlaybackEffect =
  | {
      type: "fetch_audio"
      sentence: number
      generation: number
      priority: "user"
    }
  | {
      type: "play_audio"
      sentence: number
      generation: number
      offset_ms: number
    }
  | { type: "schedule_tick"; generation: number; after_ms: number }
  | { type: "pause_audio" }
  | { type: "persist_progress"; sentence: number; position_ms: number }
  | { type: "prefetch_audio"; sentences: number[] }
  | { type: "playback_failed"; sentence: number; message: string }
  | { type: "playback_completed" }

export interface PlaybackTransition {
  state: PlaybackState
  effects: PlaybackEffect[]
}

export type ReadingMode = "manual" | "auto" | "read_aloud"
export type EffortLevel =
  | "minimal"
  | "low"
  | "medium"
  | "high"
  | "xhigh"
  | "max"

export interface EffortLadder {
  minimal: number
  low: number
  medium: number
  high: number
  xhigh: number
  max: number
}

export interface ReadingPreferences {
  mode: ReadingMode
  base_text_characters_per_second: number
  effort: EffortLevel
  multipliers?: Partial<EffortLadder>
}

export interface ReadingPace {
  mode: ReadingMode
  effort: EffortLevel
  multiplier: number
  text_characters_per_second: number
  audio_playback_rate: number
  advances_automatically: boolean
  uses_audio: boolean
}

export interface AudioDurationObservation {
  segment: number
  duration_ms: number
}

export type ReadingTimingEvidence =
  | { type: "unknown" }
  | { type: "text_rate"; characters_per_second: number }
  | {
      type: "observed_audio"
      playback_rate: number
      current_position_ms: number
      durations: AudioDurationObservation[]
    }

export interface ReadingProgressRequest {
  segment_character_counts: number[]
  current_segment: number
  current_segment_progress_basis_points: number
  timing: ReadingTimingEvidence
}

export interface ReadingProgressProjection {
  char_offset: number
  char_count: number
  progress_basis_points: number
  remaining_characters: number
  remaining_time: {
    kind: "unknown" | "text_rate" | "observed_audio"
    seconds: number | null
    sample_count: number
  }
}

export interface ReadingSeekCursor {
  segment: number
  segment_progress_basis_points: number
  char_offset: number
}

export type TextAdvanceStatus = "idle" | "running" | "paused" | "completed"

export interface TextAdvanceState {
  status: TextAdvanceStatus
  current_sentence: number
  sentence_count: number
  generation: number
  deadline_monotonic_ms: number | null
  remaining_ms: number | null
}

export type TextAdvanceEvent =
  | {
      type: "start_requested"
      character_count: number
      text_characters_per_second: number
      monotonic_ms: number
    }
  | { type: "pause_requested"; monotonic_ms: number }
  | {
      type: "seek_requested"
      sentence: number
      character_count: number
      text_characters_per_second: number
      monotonic_ms: number
      autoplay: boolean
    }
  | {
      type: "advance_due"
      generation: number
      monotonic_ms: number
      next_character_count: number
      text_characters_per_second: number
    }

export type TextAdvanceEffect =
  | { type: "schedule_advance"; generation: number; after_ms: number }
  | { type: "cancel_scheduled_advance" }
  | { type: "persist_progress"; sentence: number }
  | { type: "text_advance_completed" }

export interface TextAdvanceTransition {
  state: TextAdvanceState
  effects: TextAdvanceEffect[]
}

export interface HtmlParseOptions {
  max_input_bytes: number
  max_blocks: number
  base_url: string | null
}

export interface ContentIdentity {
  algorithm: string
  byte_length: number
  digest: string
}

export type ImportOwnership = "copy" | "link" | "move"

export interface ImportRecord {
  item_id: string
  content_identity: ContentIdentity
  ownership: ImportOwnership
  source_key: string
  storage_key: string
}

export interface ImportRequest {
  content_identity: ContentIdentity
  ownership: ImportOwnership
  source_key: string
  owned_storage_key: string
  replace_ownership?: boolean
  existing: ImportRecord | null
}

export type ImportEffect =
  | { type: "write_owned_content"; source_key: string; storage_key: string }
  | { type: "delete_source_after_commit"; source_key: string }
  | { type: "delete_owned_content"; storage_key: string }

export interface ImportPlan {
  outcome: "created" | "reused" | "updated"
  record: ImportRecord
  effects: ImportEffect[]
}

export interface BuiltinSample {
  item_id: string
  legacy_item_ids: string[]
  locale: string
  title: string
  author: string
  source: string
  content_format: string
  content: string
  markdown_source: string
  is_builtin_sample: boolean
}

export interface DocumentRevision {
  content_identity: ContentIdentity
  ir_version: number
  parser_version: string
  revision_digest: string
  char_count: number
}

export type PositionAffinity = "forward" | "backward"

export interface ReadingPosition {
  item_id: string
  document_revision: string
  char_offset: number
  affinity: PositionAffinity
  observed_at_unix_ms: number
  session_id: string
  mutation_id: string
  base_version: number
  locator?: TextLocator | null
}

export interface PositionProjection {
  char_offset: number
  block_id: string | null
  offset_in_block: number
  chapter_id: string | null
  sentence_index: number | null
  progress_basis_points: number
}

export interface RestoredPosition {
  char_offset: number
  affinity: PositionAffinity
  revision_changed: boolean
  projection: PositionProjection
  locator_resolution?: LocatorResolutionKind | null
}

export interface TextQuote {
  prefix: string
  exact: string
  suffix: string
  offset_in_exact: number
}

export interface TextLocator {
  version: number
  document_revision: string
  char_offset: number
  block_id: string
  offset_in_block: number
  affinity: PositionAffinity
  quote: TextQuote
}

export type LocatorResolutionKind =
  | "exact_revision"
  | "stable_block"
  | "text_quote"
  | "fallback_offset"

export interface LocatorResolution {
  kind: LocatorResolutionKind
  projection: PositionProjection
}

export interface WordRange {
  index: number
  range: [number, number]
  char_range: [number, number]
  text: string
}

export interface SentenceRange {
  id: string
  index: number
  block_id: string
  range: [number, number]
  char_range: [number, number]
  text: string
  words: WordRange[]
}

export interface DocumentTextIndex {
  version: number
  document_revision: string
  sentences: SentenceRange[]
}

export interface HighlightCursor {
  document_revision: string
  sentence_index: number
  word_index?: number | null
  word_progress_basis_points?: number | null
}

export interface HighlightTarget {
  sentence_id: string
  sentence_index: number
  block_id: string
  sentence_range: [number, number]
  word_range: [number, number] | null
  sentence_char_range: [number, number]
  word_char_range: [number, number] | null
}

export interface SearchRequest {
  document_revision: string
  query: string
  cursor: string | null
  limit: number
}

export interface SearchHit {
  hit_id: string
  char_range: [number, number]
  block_id: string
  snippet: string
  snippet_match_range: [number, number]
}

export interface SearchResultPage {
  folded_query: string
  total_occurrences: number
  total_blocks: number
  hits: SearchHit[]
  next_cursor: string | null
}

export interface Bookmark {
  id: string
  item_id: string
  char_offset: number
  affinity: PositionAffinity
  note: string
  created_at_unix_ms: number
  updated_at_unix_ms: number
  version: number
}

export interface BookmarkUpsertRequest {
  bookmark_id: string
  item_id: string
  char_offset: number
  affinity: PositionAffinity
  note: string
  created_at_unix_ms: number
  updated_at_unix_ms: number
  base_version: number
}

export interface BookmarkTransition {
  bookmark: Bookmark
  created: boolean
}

export type DocumentBlockKind =
  | { type: "heading"; level: number }
  | { type: "paragraph" }
  | { type: "quote" }
  | { type: "code" }
  | { type: "image"; asset_id: string }

export interface DocumentInlineSpan {
  range: [number, number]
  bold: boolean
  italic: boolean
}

export interface DocumentBlock {
  id: string
  kind: DocumentBlockKind
  text: string
  inline_spans?: DocumentInlineSpan[]
}

export interface DocumentAsset {
  id: string
  kind: "image"
  source: string
  alt: string
}

export interface DocumentAnchor {
  source_id: string
  block_id: string
}

export interface DocumentIR {
  ir_version: number
  revision?: DocumentRevision
  blocks: DocumentBlock[]
  assets: DocumentAsset[]
  anchors?: DocumentAnchor[]
}

/**
 * Versioned, parser-owned publication model consumed by every reader GUI.
 * Ranges use half-open Unicode-scalar offsets; clients only project them.
 */
export interface ReaderIR {
  ir_version: number
  document: DocumentIR
  text_index: DocumentTextIndex
  chapters: ReaderChapter[]
  navigation: ReaderNavigation[]
}

export interface ReaderChapter {
  id: string
  title: string
  block_range: [number, number]
  sentence_range: [number, number]
  char_range: [number, number]
}

export interface ReaderNavigation {
  id: string
  title: string
  depth: number
  chapter_id: string
  block_id: string | null
  sentence_index: number | null
}

export interface ParsedPDF {
  title: string | null
  author: string | null
  canonical_text: string
  page_count: number
  chapters: ParsedPDFChapter[]
  reader: ReaderIR
}

export interface ParsedPDFChapter {
  title: string | null
  page_start: number
  page_end: number
  document: DocumentIR
  block_provenance: ParsedPDFBlockProvenance[]
}

export interface ParsedPDFBlockProvenance {
  block_id: string
  page_start: number
  page_end: number
}

export interface ParsedEPUBChapter {
  id: string
  href: string
  title: string
  document: DocumentIR
}

export interface ParsedEPUBTocEntry {
  id: string
  title: string
  depth: number
  href: string
  chapter_id: string
  block_id: string | null
}

export interface ParsedEPUBAsset {
  id: string
  href: string
  media_type: string
  byte_length: number
}

export interface ParsedEPUB {
  title: string | null
  author: string | null
  cover_asset_id?: string | null
  assets?: ParsedEPUBAsset[]
  toc?: ParsedEPUBTocEntry[]
  chapters: ParsedEPUBChapter[]
  reader: ReaderIR
}

export interface RustCoreWasmModule {
  build_support_bundle?: (requestJSON: string) => string
  builtin_sample?: (languageTag: string) => string
  normalize_tts_model_catalog?: (catalogJSON: string) => string
  create_playback_state(sentenceCount: number): string
  reduce_playback(
    policyJSON: string | undefined,
    stateJSON: string,
    eventJSON: string
  ): string
  resolve_reading_pace?: (preferencesJSON: string) => string
  project_reading_progress?: (requestJSON: string) => string
  resolve_reading_seek?: (requestJSON: string) => string
  create_text_advance_state?: (sentenceCount: number) => string
  reduce_text_advance?: (stateJSON: string, eventJSON: string) => string
  content_identity?: (bytes: Uint8Array) => string
  plan_import?: (requestJSON: string) => string
  plan_forget?: (recordJSON: string) => string
  parse_html_document(html: string, optionsJSON: string | undefined): string
  parse_html_reader?: (html: string, optionsJSON: string | undefined) => string
  parse_markdown_reader?: (
    markdown: string,
    optionsJSON: string | undefined
  ) => string
  parse_plain_text_reader?: (
    text: string,
    optionsJSON: string | undefined
  ) => string
  parse_pdf?: (bytes: Uint8Array) => string
  parse_epub?: (bytes: Uint8Array) => string
  extract_epub_asset?: (bytes: Uint8Array, assetId: string) => Uint8Array
  project_offset?: (documentJSON: string, requestJSON: string) => string
  create_text_locator?: (documentJSON: string, requestJSON: string) => string
  restore_text_locator?: (documentJSON: string, locatorJSON: string) => string
  index_document_text?: (documentJSON: string) => string
  resolve_highlight?: (
    documentJSON: string,
    indexJSON: string,
    cursorJSON: string
  ) => string
  resolve_block_offset?: (documentJSON: string, requestJSON: string) => string
  restore_position?: (documentJSON: string, positionJSON: string) => string
  search_document?: (documentJSON: string, requestJSON: string) => string
  upsert_bookmark?: (
    documentJSON: string,
    bookmarksJSON: string,
    requestJSON: string
  ) => string
}

export type RustCoreWasmLoader = () => Promise<RustCoreWasmModule>

export interface RustCoreClient {
  buildSupportBundle(request: SupportBundleRequest): Promise<SupportBundle>
  builtinSample(languageTag: string): Promise<BuiltinSample>
  normalizeTTSModelCatalog(catalog: unknown): Promise<TTSModelCatalog>
  createPlaybackState(sentenceCount: number): Promise<PlaybackState>
  reducePlayback(
    state: PlaybackState,
    event: PlaybackEvent,
    policy?: PlaybackPolicy
  ): Promise<PlaybackTransition>
  resolveReadingPace(preferences: ReadingPreferences): Promise<ReadingPace>
  projectReadingProgress(
    request: ReadingProgressRequest
  ): Promise<ReadingProgressProjection>
  resolveReadingSeek(
    segmentCharacterCounts: number[],
    progressBasisPoints: number
  ): Promise<ReadingSeekCursor>
  createTextAdvanceState(sentenceCount: number): Promise<TextAdvanceState>
  reduceTextAdvance(
    state: TextAdvanceState,
    event: TextAdvanceEvent
  ): Promise<TextAdvanceTransition>
  contentIdentity(bytes: Uint8Array): Promise<ContentIdentity>
  planImport(request: ImportRequest): Promise<ImportPlan>
  planForget(record: ImportRecord): Promise<ImportEffect[]>
  parseHTML(html: string, options?: HtmlParseOptions): Promise<DocumentIR>
  parseHTMLReader(html: string, options?: HtmlParseOptions): Promise<ReaderIR>
  parseMarkdownReader(
    markdown: string,
    options?: HtmlParseOptions
  ): Promise<ReaderIR>
  parsePlainTextReader(
    text: string,
    options?: HtmlParseOptions
  ): Promise<ReaderIR>
  parsePDF(bytes: Uint8Array): Promise<ParsedPDF>
  parseEPUB(bytes: Uint8Array): Promise<ParsedEPUB>
  extractEPUBAsset(bytes: Uint8Array, assetId: string): Promise<Uint8Array>
  projectOffset(
    document: DocumentIR,
    charOffset: number,
    affinity: PositionAffinity
  ): Promise<PositionProjection>
  createTextLocator(
    document: DocumentIR,
    charOffset: number,
    affinity: PositionAffinity
  ): Promise<TextLocator>
  restoreTextLocator(
    locator: TextLocator,
    document: DocumentIR
  ): Promise<LocatorResolution>
  indexDocumentText(document: DocumentIR): Promise<DocumentTextIndex>
  resolveHighlight(
    document: DocumentIR,
    index: DocumentTextIndex,
    cursor: HighlightCursor
  ): Promise<HighlightTarget>
  resolveBlockOffset(
    document: DocumentIR,
    blockId: string,
    offsetInBlock: number,
    affinity: PositionAffinity
  ): Promise<PositionProjection>
  restorePosition(
    position: ReadingPosition,
    document: DocumentIR
  ): Promise<RestoredPosition>
  searchDocument(
    document: DocumentIR,
    request: SearchRequest
  ): Promise<SearchResultPage>
  upsertBookmark(
    document: DocumentIR,
    bookmarks: Bookmark[],
    request: BookmarkUpsertRequest
  ): Promise<BookmarkTransition>
}

export function createRustCoreClient(
  loadModule: RustCoreWasmLoader
): RustCoreClient {
  return {
    async buildSupportBundle(request) {
      const wasm = await loadModule()
      if (typeof wasm.build_support_bundle !== "function") {
        throw new Error("Rust Core WASM does not expose build_support_bundle")
      }
      return JSON.parse(
        wasm.build_support_bundle(JSON.stringify(request))
      ) as SupportBundle
    },
    async builtinSample(languageTag) {
      const wasm = await loadModule()
      if (typeof wasm.builtin_sample !== "function") {
        throw new Error("Rust Core WASM does not expose builtin_sample")
      }
      return JSON.parse(wasm.builtin_sample(languageTag)) as BuiltinSample
    },
    async normalizeTTSModelCatalog(catalog) {
      const wasm = await loadModule()
      if (typeof wasm.normalize_tts_model_catalog !== "function") {
        throw new Error("Rust Core WASM does not expose normalize_tts_model_catalog")
      }
      return JSON.parse(
        wasm.normalize_tts_model_catalog(JSON.stringify(catalog))
      ) as TTSModelCatalog
    },
    async createPlaybackState(sentenceCount) {
      const wasm = await loadModule()
      return JSON.parse(wasm.create_playback_state(sentenceCount)) as PlaybackState
    },
    async reducePlayback(state, event, policy) {
      const wasm = await loadModule()
      const transition = wasm.reduce_playback(
        policy === undefined ? undefined : JSON.stringify(policy),
        JSON.stringify(state),
        JSON.stringify(event)
      )
      return JSON.parse(transition) as PlaybackTransition
    },
    async resolveReadingPace(preferences) {
      const wasm = await loadModule()
      if (typeof wasm.resolve_reading_pace !== "function") {
        throw new Error("Rust Core WASM does not expose resolve_reading_pace")
      }
      return JSON.parse(
        wasm.resolve_reading_pace(JSON.stringify(preferences))
      ) as ReadingPace
    },
    async projectReadingProgress(request) {
      const wasm = await loadModule()
      if (typeof wasm.project_reading_progress !== "function") {
        throw new Error("Rust Core WASM does not expose project_reading_progress")
      }
      return JSON.parse(
        wasm.project_reading_progress(JSON.stringify(request))
      ) as ReadingProgressProjection
    },
    async resolveReadingSeek(segmentCharacterCounts, progressBasisPoints) {
      const wasm = await loadModule()
      if (typeof wasm.resolve_reading_seek !== "function") {
        throw new Error("Rust Core WASM does not expose resolve_reading_seek")
      }
      return JSON.parse(
        wasm.resolve_reading_seek(
          JSON.stringify({
            segment_character_counts: segmentCharacterCounts,
            progress_basis_points: progressBasisPoints,
          })
        )
      ) as ReadingSeekCursor
    },
    async createTextAdvanceState(sentenceCount) {
      const wasm = await loadModule()
      if (typeof wasm.create_text_advance_state !== "function") {
        throw new Error("Rust Core WASM does not expose create_text_advance_state")
      }
      return JSON.parse(
        wasm.create_text_advance_state(sentenceCount)
      ) as TextAdvanceState
    },
    async reduceTextAdvance(state, event) {
      const wasm = await loadModule()
      if (typeof wasm.reduce_text_advance !== "function") {
        throw new Error("Rust Core WASM does not expose reduce_text_advance")
      }
      return JSON.parse(
        wasm.reduce_text_advance(JSON.stringify(state), JSON.stringify(event))
      ) as TextAdvanceTransition
    },
    async contentIdentity(bytes) {
      const wasm = await loadModule()
      if (typeof wasm.content_identity !== "function") {
        throw new Error("Rust Core WASM does not expose content_identity")
      }
      return JSON.parse(wasm.content_identity(bytes)) as ContentIdentity
    },
    async planImport(request) {
      const wasm = await loadModule()
      if (typeof wasm.plan_import !== "function") {
        throw new Error("Rust Core WASM does not expose plan_import")
      }
      return JSON.parse(wasm.plan_import(JSON.stringify(request))) as ImportPlan
    },
    async planForget(record) {
      const wasm = await loadModule()
      if (typeof wasm.plan_forget !== "function") {
        throw new Error("Rust Core WASM does not expose plan_forget")
      }
      return JSON.parse(wasm.plan_forget(JSON.stringify(record))) as ImportEffect[]
    },
    async parseHTML(html, options) {
      const wasm = await loadModule()
      const document = wasm.parse_html_document(
        html,
        options === undefined ? undefined : JSON.stringify(options)
      )
      return JSON.parse(document) as DocumentIR
    },
    async parseHTMLReader(html, options) {
      const wasm = await loadModule()
      if (typeof wasm.parse_html_reader !== "function") {
        throw new Error("Rust Core WASM does not expose parse_html_reader")
      }
      return JSON.parse(
        wasm.parse_html_reader(
          html,
          options === undefined ? undefined : JSON.stringify(options)
        )
      ) as ReaderIR
    },
    async parseMarkdownReader(markdown, options) {
      const wasm = await loadModule()
      if (typeof wasm.parse_markdown_reader !== "function") {
        throw new Error("Rust Core WASM does not expose parse_markdown_reader")
      }
      return JSON.parse(
        wasm.parse_markdown_reader(
          markdown,
          options === undefined ? undefined : JSON.stringify(options)
        )
      ) as ReaderIR
    },
    async parsePlainTextReader(text, options) {
      const wasm = await loadModule()
      if (typeof wasm.parse_plain_text_reader !== "function") {
        throw new Error("Rust Core WASM does not expose parse_plain_text_reader")
      }
      return JSON.parse(
        wasm.parse_plain_text_reader(
          text,
          options === undefined ? undefined : JSON.stringify(options)
        )
      ) as ReaderIR
    },
    async parsePDF(bytes) {
      const wasm = await loadModule()
      if (typeof wasm.parse_pdf !== "function") {
        throw new Error("Rust Core WASM does not expose parse_pdf")
      }
      return JSON.parse(wasm.parse_pdf(bytes)) as ParsedPDF
    },
    async parseEPUB(bytes) {
      const wasm = await loadModule()
      if (typeof wasm.parse_epub !== "function") {
        throw new Error("Rust Core WASM does not expose parse_epub")
      }
      return JSON.parse(wasm.parse_epub(bytes)) as ParsedEPUB
    },
    async extractEPUBAsset(bytes, assetId) {
      const wasm = await loadModule()
      if (typeof wasm.extract_epub_asset !== "function") {
        throw new Error("Rust Core WASM does not expose extract_epub_asset")
      }
      return wasm.extract_epub_asset(bytes, assetId)
    },
    async projectOffset(document, charOffset, affinity) {
      const wasm = await loadModule()
      if (typeof wasm.project_offset !== "function") {
        throw new Error("Rust Core WASM does not expose project_offset")
      }
      const projection = wasm.project_offset(
        JSON.stringify(document),
        JSON.stringify({ char_offset: charOffset, affinity })
      )
      return JSON.parse(projection) as PositionProjection
    },
    async createTextLocator(document, charOffset, affinity) {
      const wasm = await loadModule()
      if (typeof wasm.create_text_locator !== "function") {
        throw new Error("Rust Core WASM does not expose create_text_locator")
      }
      const locator = wasm.create_text_locator(
        JSON.stringify(document),
        JSON.stringify({ char_offset: charOffset, affinity })
      )
      return JSON.parse(locator) as TextLocator
    },
    async restoreTextLocator(locator, document) {
      const wasm = await loadModule()
      if (typeof wasm.restore_text_locator !== "function") {
        throw new Error("Rust Core WASM does not expose restore_text_locator")
      }
      const restored = wasm.restore_text_locator(
        JSON.stringify(document),
        JSON.stringify(locator)
      )
      return JSON.parse(restored) as LocatorResolution
    },
    async indexDocumentText(document) {
      const wasm = await loadModule()
      if (typeof wasm.index_document_text !== "function") {
        throw new Error("Rust Core WASM does not expose index_document_text")
      }
      return JSON.parse(
        wasm.index_document_text(JSON.stringify(document))
      ) as DocumentTextIndex
    },
    async resolveHighlight(document, index, cursor) {
      const wasm = await loadModule()
      if (typeof wasm.resolve_highlight !== "function") {
        throw new Error("Rust Core WASM does not expose resolve_highlight")
      }
      return JSON.parse(
        wasm.resolve_highlight(
          JSON.stringify(document),
          JSON.stringify(index),
          JSON.stringify(cursor)
        )
      ) as HighlightTarget
    },
    async resolveBlockOffset(document, blockId, offsetInBlock, affinity) {
      const wasm = await loadModule()
      if (typeof wasm.resolve_block_offset !== "function") {
        throw new Error("Rust Core WASM does not expose resolve_block_offset")
      }
      const projection = wasm.resolve_block_offset(
        JSON.stringify(document),
        JSON.stringify({
          block_id: blockId,
          offset_in_block: offsetInBlock,
          affinity,
        })
      )
      return JSON.parse(projection) as PositionProjection
    },
    async restorePosition(position, document) {
      const wasm = await loadModule()
      if (typeof wasm.restore_position !== "function") {
        throw new Error("Rust Core WASM does not expose restore_position")
      }
      const restored = wasm.restore_position(
        JSON.stringify(document),
        JSON.stringify(position)
      )
      return JSON.parse(restored) as RestoredPosition
    },
    async searchDocument(document, request) {
      const wasm = await loadModule()
      if (typeof wasm.search_document !== "function") {
        throw new Error("Rust Core WASM does not expose search_document")
      }
      const result = wasm.search_document(
        JSON.stringify(document),
        JSON.stringify(request)
      )
      return JSON.parse(result) as SearchResultPage
    },
    async upsertBookmark(document, bookmarks, request) {
      const wasm = await loadModule()
      if (typeof wasm.upsert_bookmark !== "function") {
        throw new Error("Rust Core WASM does not expose upsert_bookmark")
      }
      const transition = wasm.upsert_bookmark(
        JSON.stringify(document),
        JSON.stringify(bookmarks),
        JSON.stringify(request)
      )
      return JSON.parse(transition) as BookmarkTransition
    },
  }
}
