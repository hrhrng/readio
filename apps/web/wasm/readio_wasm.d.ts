/* tslint:disable */
/* eslint-disable */

/**
 * Stateful Loro replica used by the browser SDK. Desktop and TUI use the same
 * `readio_sdk::replica::ReadioReplica` directly through their native bindings.
 */
export class ReadioReplicaWasm {
    free(): void;
    [Symbol.dispose](): void;
    delete_entity(entity_kind: string, entity_id: string): Uint8Array;
    static from_snapshot(snapshot: Uint8Array): ReadioReplicaWasm;
    heads(): Uint8Array;
    import_change(change: Uint8Array): void;
    constructor();
    snapshot(): Uint8Array;
    state_json(): string;
    upsert(entity_kind: string, entity_id: string, payload_json: string): Uint8Array;
}

/**
 * Serializable Loro snapshot/outbox state. Browser storage and HTTP remain
 * JavaScript effects while every transition stays in the shared Rust SDK.
 */
export class ReadioSyncStateWasm {
    free(): void;
    [Symbol.dispose](): void;
    acknowledge(change_ids_json: string): void;
    bookmarks_json(item_id: string): string;
    delete_local(entities_json: string): void;
    static from_json(state_json: string): ReadioSyncStateWasm;
    heads(): Uint8Array;
    import_remote(changes_json: string): void;
    json(): string;
    library_item_json(item_id: string): string;
    constructor();
    outbox_json(): string;
    query_library_json(query_json: string): string;
    reading_position_json(item_id: string): string;
    save_reading_position_json(item_id: string, mutation_json: string): string;
    settings_json(): string;
    snapshot(): Uint8Array;
    stage_local(entities_json: string): void;
    state_json(): string;
}

export function build_support_bundle(request_json: string): string;

export function builtin_sample(language_tag: string): string;

export function content_identity(bytes: Uint8Array): string;

export function create_playback_state(sentence_count: number): string;

export function create_text_advance_state(sentence_count: number): string;

export function create_text_locator(document_json: string, request_json: string): string;

export function extract_epub_asset(bytes: Uint8Array, asset_id: string): Uint8Array;

export function index_document_text(document_json: string): string;

export function normalize_tts_model_catalog(catalog_json: string): string;

export function parse_epub(bytes: Uint8Array): string;

export function parse_html_document(html: string, options_json?: string | null): string;

export function parse_html_reader(html: string, options_json?: string | null): string;

export function parse_markdown_reader(markdown: string, options_json?: string | null): string;

export function parse_pdf(bytes: Uint8Array): string;

export function parse_plain_text_reader(text: string, options_json?: string | null): string;

export function plan_forget(record_json: string): string;

export function plan_import(request_json: string): string;

export function plan_sync_push(snapshot_json: string, batch_json: string, server_timestamp_unix_ms: bigint, starting_cursor: bigint): string;

export function project_offset(document_json: string, request_json: string): string;

export function project_reading_progress(request_json: string): string;

export function reduce_playback(policy_json: string | null | undefined, state_json: string, event_json: string): string;

export function reduce_text_advance(state_json: string, event_json: string): string;

export function resolve_block_offset(document_json: string, request_json: string): string;

export function resolve_highlight(document_json: string, index_json: string, cursor_json: string): string;

export function resolve_reading_pace(preferences_json: string): string;

export function resolve_reading_seek(request_json: string): string;

export function restore_position(document_json: string, position_json: string): string;

export function restore_text_locator(document_json: string, locator_json: string): string;

export function search_document(document_json: string, request_json: string): string;

export function upsert_bookmark(document_json: string, bookmarks_json: string, request_json: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_readioreplicawasm_free: (a: number, b: number) => void;
    readonly __wbg_readiosyncstatewasm_free: (a: number, b: number) => void;
    readonly build_support_bundle: (a: number, b: number) => [number, number, number, number];
    readonly builtin_sample: (a: number, b: number) => [number, number, number, number];
    readonly content_identity: (a: number, b: number) => [number, number, number, number];
    readonly create_playback_state: (a: number) => [number, number, number, number];
    readonly create_text_advance_state: (a: number) => [number, number, number, number];
    readonly create_text_locator: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly extract_epub_asset: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly index_document_text: (a: number, b: number) => [number, number, number, number];
    readonly normalize_tts_model_catalog: (a: number, b: number) => [number, number, number, number];
    readonly parse_epub: (a: number, b: number) => [number, number, number, number];
    readonly parse_html_document: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly parse_html_reader: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly parse_markdown_reader: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly parse_pdf: (a: number, b: number) => [number, number, number, number];
    readonly parse_plain_text_reader: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly plan_forget: (a: number, b: number) => [number, number, number, number];
    readonly plan_import: (a: number, b: number) => [number, number, number, number];
    readonly plan_sync_push: (a: number, b: number, c: number, d: number, e: bigint, f: bigint) => [number, number, number, number];
    readonly project_offset: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly project_reading_progress: (a: number, b: number) => [number, number, number, number];
    readonly readioreplicawasm_delete_entity: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly readioreplicawasm_from_snapshot: (a: number, b: number) => [number, number, number];
    readonly readioreplicawasm_heads: (a: number) => [number, number];
    readonly readioreplicawasm_import_change: (a: number, b: number, c: number) => [number, number];
    readonly readioreplicawasm_new: () => number;
    readonly readioreplicawasm_snapshot: (a: number) => [number, number, number, number];
    readonly readioreplicawasm_state_json: (a: number) => [number, number];
    readonly readioreplicawasm_upsert: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number, number, number];
    readonly readiosyncstatewasm_acknowledge: (a: number, b: number, c: number) => [number, number];
    readonly readiosyncstatewasm_bookmarks_json: (a: number, b: number, c: number) => [number, number, number, number];
    readonly readiosyncstatewasm_delete_local: (a: number, b: number, c: number) => [number, number];
    readonly readiosyncstatewasm_from_json: (a: number, b: number) => [number, number, number];
    readonly readiosyncstatewasm_heads: (a: number) => [number, number, number, number];
    readonly readiosyncstatewasm_import_remote: (a: number, b: number, c: number) => [number, number];
    readonly readiosyncstatewasm_json: (a: number) => [number, number, number, number];
    readonly readiosyncstatewasm_library_item_json: (a: number, b: number, c: number) => [number, number, number, number];
    readonly readiosyncstatewasm_new: () => number;
    readonly readiosyncstatewasm_outbox_json: (a: number) => [number, number, number, number];
    readonly readiosyncstatewasm_query_library_json: (a: number, b: number, c: number) => [number, number, number, number];
    readonly readiosyncstatewasm_reading_position_json: (a: number, b: number, c: number) => [number, number, number, number];
    readonly readiosyncstatewasm_save_reading_position_json: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly readiosyncstatewasm_settings_json: (a: number) => [number, number, number, number];
    readonly readiosyncstatewasm_snapshot: (a: number) => [number, number, number, number];
    readonly readiosyncstatewasm_stage_local: (a: number, b: number, c: number) => [number, number];
    readonly readiosyncstatewasm_state_json: (a: number) => [number, number, number, number];
    readonly reduce_playback: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly reduce_text_advance: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly resolve_block_offset: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly resolve_highlight: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly resolve_reading_pace: (a: number, b: number) => [number, number, number, number];
    readonly resolve_reading_seek: (a: number, b: number) => [number, number, number, number];
    readonly restore_position: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly restore_text_locator: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly search_document: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly upsert_bookmark: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
