"use client"

import {
  createReadioSdkClient,
  type ReadioSdkWasmModule,
} from "./readio-sdk.ts"
import init, {
  ReadioReplicaWasm,
  ReadioSyncStateWasm,
  type InitInput,
  build_support_bundle,
  builtin_sample,
  content_identity,
  create_playback_state,
  create_text_advance_state,
  create_text_locator,
  extract_epub_asset,
  index_document_text,
  normalize_tts_model_catalog,
  parse_epub,
  parse_html_document,
  parse_html_reader,
  parse_markdown_reader,
  parse_pdf,
  parse_plain_text_reader,
  plan_forget,
  plan_import,
  project_offset,
  project_reading_progress,
  reduce_playback,
  reduce_text_advance,
  resolve_block_offset,
  resolve_highlight,
  resolve_reading_pace,
  resolve_reading_seek,
  restore_position,
  restore_text_locator,
  search_document,
  upsert_bookmark,
} from "../wasm/readio_wasm.js"
import {
  WebPlaybackExecutor,
  type WebPlaybackPlatform,
} from "./web-playback-executor.ts"
import {
  WebTextAdvanceExecutor,
  type WebTextAdvancePlatform,
} from "./web-text-advance-executor.ts"
import type { RustCoreClient } from "./rust-core.ts"
import type { WebDiagnosticLog } from "./diagnostics.ts"

const generatedModule: ReadioSdkWasmModule = {
  ReadioReplicaWasm,
  ReadioSyncStateWasm,
  build_support_bundle,
  builtin_sample,
  content_identity,
  create_playback_state,
  create_text_advance_state,
  create_text_locator,
  extract_epub_asset,
  index_document_text,
  normalize_tts_model_catalog,
  parse_epub,
  parse_html_document,
  parse_html_reader,
  parse_markdown_reader,
  parse_pdf,
  parse_plain_text_reader,
  plan_forget,
  plan_import,
  project_offset,
  project_reading_progress,
  resolve_block_offset,
  resolve_highlight,
  reduce_playback,
  reduce_text_advance,
  restore_position,
  restore_text_locator,
  resolve_reading_pace,
  resolve_reading_seek,
  search_document,
  upsert_bookmark,
}

/**
 * Initialize the checked-in bindgen artifact once. Tests may supply bytes;
 * browsers use the generated module-relative URL so Next can emit the asset.
 */
export function createReadioWasmLoader(input?: InitInput) {
  let pending: Promise<ReadioSdkWasmModule> | undefined

  return async (): Promise<ReadioSdkWasmModule> => {
    pending ??= init(
      input === undefined ? undefined : { module_or_path: input }
    ).then(() => generatedModule)
    return pending
  }
}

export const loadReadioWasm = createReadioWasmLoader()
export const webReadioSdk = createReadioSdkClient(loadReadioWasm)

// Transitional alias for components that only consume document APIs. New code
// should use the SDK name so product logic never grows another platform core.
export const webRustCore = webReadioSdk

export function createWebPlaybackExecutor(
  platform: WebPlaybackPlatform,
  core: RustCoreClient = webRustCore,
  diagnostics?: Pick<WebDiagnosticLog, "beginOperation" | "record">
): WebPlaybackExecutor {
  return new WebPlaybackExecutor(core, platform, diagnostics)
}

export function createWebTextAdvanceExecutor(
  platform: WebTextAdvancePlatform,
  core: RustCoreClient = webRustCore
): WebTextAdvanceExecutor {
  return new WebTextAdvanceExecutor(core, platform)
}
