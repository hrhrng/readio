import {
  LibraryItem,
  LibraryItemCategory,
  LibraryItemsResponse,
  LibraryItemType,
  TTSJobCreateParams,
  TTSJobStatusResponse,
  VoiceListResponse,
} from "./types";
import { getBrowserLocalReplicaStore } from "./browser-local-replica";
import type { LocalReplicaStore } from "./local-replica-store";
import { webReadioSdk } from "./readio-core-runtime";
import type { ImportRecord, RustCoreClient } from "./rust-core";
import { webDiagnostics, type WebDiagnosticLog } from "./diagnostics";

export {
  fetchReadingPosition,
  ReadingPositionConflictError,
  saveReadingPosition,
} from "./reading-position-api";

const BASE = "";
const BUILTIN_SAMPLE_ITEM_ID = "readio:sample:v1";

/** Wrapper around fetch that includes credentials for cookie-based auth. */
function apiFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  return fetch(input, { credentials: "include", ...init });
}

function responseErrorMessage(value: unknown): string | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const body = value as Record<string, unknown>;
  if (typeof body.detail === "string") return body.detail;
  const nested = body.error;
  if (!nested || typeof nested !== "object" || Array.isArray(nested)) return undefined;
  const message = (nested as Record<string, unknown>).message;
  return typeof message === "string" ? message : undefined;
}

interface FetchItemsParams {
  category?: string;
  search?: string;
  folder_id?: string;
  type?: string;
  page?: number;
  page_size?: number;
  sort_by?: string;
  sort_order?: string;
  progress_min?: number;
  progress_max?: number;
}

function libraryCategory(value?: string): LibraryItemCategory | undefined {
  if (value === undefined) return undefined;
  if (value === "imported" || value === "podcasts") return value;
  throw new Error(`Unsupported library category: ${value}`);
}

function libraryItemType(value?: string): LibraryItemType | undefined {
  if (value === undefined) return undefined;
  if (value === "web" || value === "txt" || value === "pdf" || value === "epub") {
    return value;
  }
  throw new Error(`Unsupported library item type: ${value}`);
}

function librarySortBy(
  value?: string
): "created_at" | "title" | "progress" | undefined {
  if (value === undefined) return undefined;
  if (value === "created_at" || value === "title" || value === "progress") {
    return value;
  }
  throw new Error(`Unsupported library sort: ${value}`);
}

function librarySortOrder(
  value?: string
): "ascending" | "descending" | undefined {
  if (value === undefined) return undefined;
  if (value === "asc" || value === "ascending") return "ascending";
  if (value === "desc" || value === "descending") return "descending";
  throw new Error(`Unsupported library sort order: ${value}`);
}

function importedFileType(fileName: string): LibraryItemType {
  const extension = fileName.split(".").pop()?.toLowerCase();
  if (extension === "pdf" || extension === "epub") return extension;
  if (extension === "txt" || extension === "md" || extension === "markdown") {
    return "txt";
  }
  throw new Error(`Unsupported local file type: ${extension ?? "unknown"}`);
}

function isMarkdownFileName(fileName: string): boolean {
  const extension = fileName.split(".").pop()?.toLowerCase();
  return extension === "md" || extension === "markdown";
}

function diagnosticFileFormat(
  fileName: string,
  type: LibraryItemType
): string {
  if (isMarkdownFileName(fileName)) return "markdown";
  return type === "txt" ? "plain_text" : type;
}

function titleFromFileName(fileName: string): string {
  const title = fileName.replace(/\.[^.]+$/, "").trim();
  return title || "Untitled";
}

function mediaTypeFor(type: LibraryItemType): string {
  if (type === "pdf") return "application/pdf";
  if (type === "epub") return "application/epub+zip";
  return "text/plain";
}

export async function fetchItems(
  params: FetchItemsParams = {},
  store?: LocalReplicaStore
): Promise<LibraryItemsResponse> {
  const local = store ?? (await getBrowserLocalReplicaStore());
  return local.queryLibrary({
    category: libraryCategory(params.category),
    search: params.search,
    folder_id: params.folder_id,
    type: libraryItemType(params.type),
    page: params.page,
    page_size: params.page_size,
    sort_by: librarySortBy(params.sort_by),
    sort_order: librarySortOrder(params.sort_order),
    progress_min: params.progress_min,
    progress_max: params.progress_max,
  });
}

export async function fetchItem(
  id: string,
  store?: LocalReplicaStore,
  core: Pick<RustCoreClient, "builtinSample"> = webReadioSdk,
  languageTag = preferredLanguageTag()
): Promise<LibraryItem> {
  if (id === BUILTIN_SAMPLE_ITEM_ID) {
    const sample = await core.builtinSample(languageTag);
    if (sample.item_id !== BUILTIN_SAMPLE_ITEM_ID) {
      throw new Error(`Rust Core returned an unexpected sample: ${sample.item_id}`);
    }
    return {
      id: sample.item_id,
      legacy_item_ids: sample.legacy_item_ids,
      locale: sample.locale,
      title: sample.title,
      author: sample.author,
      content: sample.content,
      type: "txt",
      progress: 0,
      date: "",
      category: "imported",
      folder_id: "",
      source: sample.source,
      file_path: null,
      cover_image: null,
      voice: null,
      speed: null,
      current_chapter: null,
      content_format: sample.content_format,
      markdown_source: sample.markdown_source,
      is_builtin_sample: sample.is_builtin_sample,
    };
  }

  const local = store ?? (await getBrowserLocalReplicaStore());
  const [item, content] = await Promise.all([
    local.libraryItem(id),
    local.libraryContent(id),
  ]);
  if (!item) throw new Error(`Library item not found: ${id}`);
  return { ...item, content: content?.content ?? "" };
}

function preferredLanguageTag(): string {
  if (typeof navigator === "undefined") return "en";
  return navigator.languages[0] || navigator.language || "en";
}

export async function deleteItem(
  id: string,
  store?: LocalReplicaStore,
  core: Pick<RustCoreClient, "planForget"> = webReadioSdk
): Promise<void> {
  const local = store ?? (await getBrowserLocalReplicaStore());
  const item = await local.libraryItem(id);
  const effects = item?.import_record
    ? await core.planForget(item.import_record)
    : [{ type: "delete_owned_content" as const, storage_key: `local:${id}` }];
  await local.delete([{ entityKind: "library_item", entityId: id }]);
  if (effects.some((effect) => effect.type === "delete_owned_content")) {
    // Browser-owned content is keyed by item identity; storage_key remains the
    // portable address used by Rust and other platform adapters.
    await local.deleteLibraryContent(id);
  }
}

export async function searchItems(
  query: string,
  store?: LocalReplicaStore
): Promise<LibraryItemsResponse> {
  return fetchItems({ search: query, page_size: 50 }, store);
}

export type ImportProgressStage =
  | "reading_source"
  | "acquiring"
  | "identifying"
  | "parsing"
  | "saving"
  | "complete";

export interface ImportProgress {
  stage: ImportProgressStage;
  completed: number;
  total: number;
}

interface ImportOptions {
  folder_id?: string;
  category?: string;
  title?: string;
  diagnostics?: Pick<WebDiagnosticLog, "beginOperation" | "record">;
  signal?: AbortSignal;
  onProgress?: (progress: ImportProgress) => void;
}

function throwIfImportCancelled(signal?: AbortSignal): void {
  if (signal?.aborted) {
    throw new DOMException("Import cancelled", "AbortError");
  }
}

function reportImportProgress(
  options: ImportOptions,
  stage: ImportProgressStage,
  completed: number,
  total: number
): void {
  throwIfImportCancelled(options.signal);
  options.onProgress?.({ stage, completed, total });
  // A progress callback may synchronously cancel the operation. Observe that
  // before starting the next expensive or durable phase.
  throwIfImportCancelled(options.signal);
}

export async function importFile(
  file: File,
  options: ImportOptions = {},
  store?: LocalReplicaStore,
  core: Pick<
    RustCoreClient,
    "contentIdentity" | "parseEPUB" | "parsePDF" | "planImport"
  > = webReadioSdk
): Promise<LibraryItem> {
  const diagnostics = options.diagnostics ?? webDiagnostics;
  const operationId = diagnostics.beginOperation();
  const local = store ?? (await getBrowserLocalReplicaStore());
  reportImportProgress(options, "reading_source", 0, 4);
  const type = importedFileType(file.name);
  const bytes = new Uint8Array(await file.arrayBuffer());
  reportImportProgress(options, "identifying", 1, 4);
  const identity = await core.contentIdentity(bytes);
  throwIfImportCancelled(options.signal);
  const ownedStorageKey = `local:${identity.digest}`;
  const existingItem = await local.libraryItem(identity.digest);
  const existing: ImportRecord | null = existingItem
    ? {
        item_id: existingItem.id,
        content_identity: identity,
        ownership: "copy",
        source_key: existingItem.source,
        storage_key: existingItem.file_path ?? ownedStorageKey,
      }
    : null;
  const plan = await core.planImport({
    content_identity: identity,
    ownership: "copy",
    source_key: `browser-file:${file.name}`,
    owned_storage_key: ownedStorageKey,
    existing,
  });
  throwIfImportCancelled(options.signal);
  if (plan.outcome === "reused") {
    diagnostics.record({
      operationId,
      component: "web-library",
      category: "import",
      name: "import_completed",
      fields: {
        source_kind: "file",
        document_format: diagnosticFileFormat(file.name, type),
        byte_count: bytes.byteLength,
        duplicate: true,
        outcome: "reused",
      },
    });
    const item = await fetchItem(plan.record.item_id, local);
    reportImportProgress(options, "complete", 4, 4);
    return item;
  }

  const id = plan.record.item_id;
  let content: string;
  let contentFormat: string;
  let inferredTitle: string | null = null;
  let inferredAuthor: string | null = null;
  let inferredCoverImage: string | null = null;
  reportImportProgress(options, "parsing", 2, 4);
  if (type === "txt") {
    content = new TextDecoder().decode(bytes);
    contentFormat = isMarkdownFileName(file.name) ? "markdown" : "plaintext";
  } else if (type === "pdf") {
    const parsed = await core.parsePDF(bytes);
    throwIfImportCancelled(options.signal);
    content = JSON.stringify(parsed);
    contentFormat = "readio-pdf-v1";
    inferredTitle = parsed.title?.trim() || null;
    inferredAuthor = parsed.author?.trim() || null;
    diagnostics.record({
      operationId,
      component: "web-reader-parser",
      category: "parsing",
      name: "parse_completed",
      fields: {
        document_format: "pdf",
        byte_count: bytes.byteLength,
        block_count: parsed.chapters.reduce(
          (count, chapter) => count + chapter.document.blocks.length,
          0
        ),
        chapter_count: parsed.chapters.length,
        outcome: "completed",
      },
    });
  } else {
    const publication = await core.parseEPUB(bytes);
    throwIfImportCancelled(options.signal);
    content = JSON.stringify(publication);
    contentFormat = "readio-epub-v1";
    inferredTitle = publication.title?.trim() || null;
    inferredAuthor = publication.author?.trim() || null;
    inferredCoverImage = publication.cover_asset_id
      ? `readio-epub-asset:${publication.cover_asset_id}`
      : null;
    diagnostics.record({
      operationId,
      component: "web-reader-parser",
      category: "parsing",
      name: "parse_completed",
      fields: {
        document_format: "epub",
        byte_count: bytes.byteLength,
        block_count: publication.chapters.reduce(
          (count, chapter) => count + chapter.document.blocks.length,
          0
        ),
        chapter_count: publication.chapters.length,
        outcome: "completed",
      },
    });
  }
  const item: LibraryItem = {
    id,
    title:
      options.title?.trim() || inferredTitle || titleFromFileName(file.name),
    ...(inferredAuthor ? { author: inferredAuthor } : {}),
    content,
    type,
    progress: 0,
    date: new Date().toISOString(),
    category: libraryCategory(options.category) ?? "imported",
    folder_id: options.folder_id ?? "f1",
    source: file.name,
    file_path: plan.record.storage_key,
    cover_image: inferredCoverImage,
    voice: null,
    speed: null,
    current_chapter: null,
    content_format: contentFormat,
    import_record: plan.record,
  };

  reportImportProgress(options, "saving", 3, 4);

  for (const effect of plan.effects) {
    if (effect.type !== "write_owned_content") continue;
    await local.saveLibraryContent({
      itemId: id,
      content,
      fileBytes: bytes,
      mediaType: file.type || mediaTypeFor(type),
      fileName: file.name,
    });
    throwIfImportCancelled(options.signal);
  }
  try {
    const { content: _content, ...metadata } = item;
    await local.upsert([
      { entityKind: "library_item", entityId: id, payload: metadata },
    ]);
    throwIfImportCancelled(options.signal);
  } catch (error) {
    await local.deleteLibraryContent(id);
    throw error;
  }
  diagnostics.record({
    operationId,
    component: "web-library",
    category: "import",
    name: "import_completed",
    fields: {
      source_kind: "file",
      document_format: diagnosticFileFormat(file.name, type),
      byte_count: bytes.byteLength,
      duplicate: false,
      outcome: plan.outcome,
    },
  });
  reportImportProgress(options, "complete", 4, 4);
  return item;
}

export async function importUrl(
  url: string,
  options: ImportOptions = {},
  store?: LocalReplicaStore,
  core: Pick<
    RustCoreClient,
    "contentIdentity" | "parseHTML" | "planImport"
  > = webReadioSdk
): Promise<LibraryItem> {
  const diagnostics = options.diagnostics ?? webDiagnostics;
  const operationId = diagnostics.beginOperation();
  reportImportProgress(options, "acquiring", 0, 4);
  const res = await apiFetch(`${BASE}/api/cloud/v1/acquire/url`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ url }),
    signal: options.signal,
  });
  throwIfImportCancelled(options.signal);
  if (!res.ok) {
    const body: unknown = await res.json().catch(() => null);
    throw new Error(responseErrorMessage(body) ?? `Import failed: ${res.status}`);
  }
  const acquired = (await res.json()) as {
    final_url: string;
    media_type: string;
    html: string;
  };
  reportImportProgress(options, "parsing", 1, 4);
  const local = store ?? (await getBrowserLocalReplicaStore());
  const sourceBytes = new TextEncoder().encode(acquired.html);
  const document = await core.parseHTML(acquired.html, {
    max_input_bytes: sourceBytes.byteLength,
    max_blocks: 100_000,
    base_url: acquired.final_url,
  });
  throwIfImportCancelled(options.signal);
  diagnostics.record({
    operationId,
    component: "web-reader-parser",
    category: "parsing",
    name: "parse_completed",
    fields: {
      document_format: "html",
      byte_count: sourceBytes.byteLength,
      block_count: document.blocks.length,
      chapter_count: 1,
      outcome: "completed",
    },
  });
  const content = document.blocks.map((block) => block.text).join("\n");
  if (!content.trim()) throw new Error("No readable content was found at this URL");
  reportImportProgress(options, "identifying", 2, 4);
  const identity = await core.contentIdentity(new TextEncoder().encode(content));
  throwIfImportCancelled(options.signal);
  const ownedStorageKey = `local:${identity.digest}`;
  const existingItem = await local.libraryItem(identity.digest);
  const existing: ImportRecord | null = existingItem
    ? existingItem.import_record ?? {
        item_id: existingItem.id,
        content_identity: identity,
        ownership: "copy",
        source_key: existingItem.source,
        storage_key: existingItem.file_path ?? ownedStorageKey,
      }
    : null;
  const plan = await core.planImport({
    content_identity: identity,
    ownership: "copy",
    source_key: `url:${acquired.final_url}`,
    owned_storage_key: ownedStorageKey,
    existing,
  });
  throwIfImportCancelled(options.signal);
  if (plan.outcome === "reused") {
    diagnostics.record({
      operationId,
      component: "web-library",
      category: "import",
      name: "import_completed",
      fields: {
        source_kind: "url",
        document_format: "html",
        byte_count: sourceBytes.byteLength,
        duplicate: true,
        outcome: "reused",
      },
    });
    const item = await fetchItem(plan.record.item_id, local);
    reportImportProgress(options, "complete", 4, 4);
    return item;
  }

  const heading = document.blocks.find((block) => block.kind.type === "heading")?.text;
  const item: LibraryItem = {
    id: plan.record.item_id,
    title:
      options.title?.trim() ||
      heading?.trim() ||
      new URL(acquired.final_url).hostname ||
      "Untitled",
    content,
    type: "web",
    progress: 0,
    date: new Date().toISOString(),
    category: libraryCategory(options.category) ?? "imported",
    folder_id: options.folder_id ?? "f1",
    source: acquired.final_url,
    file_path: plan.record.storage_key,
    cover_image: null,
    voice: null,
    speed: null,
    current_chapter: null,
    content_format: "plaintext",
    import_record: plan.record,
  };
  reportImportProgress(options, "saving", 3, 4);
  for (const effect of plan.effects) {
    if (effect.type !== "write_owned_content") continue;
    await local.saveLibraryContent({
      itemId: item.id,
      content,
      fileBytes: sourceBytes,
      mediaType: acquired.media_type || "text/html",
      fileName: null,
    });
    throwIfImportCancelled(options.signal);
  }
  try {
    const { content: _content, ...metadata } = item;
    await local.upsert([
      {
        entityKind: "library_item",
        entityId: plan.record.item_id,
        payload: metadata,
      },
    ]);
    throwIfImportCancelled(options.signal);
  } catch (error) {
    await local.deleteLibraryContent(plan.record.item_id);
    throw error;
  }
  diagnostics.record({
    operationId,
    component: "web-library",
    category: "import",
    name: "import_completed",
    fields: {
      source_kind: "url",
      document_format: "html",
      byte_count: sourceBytes.byteLength,
      duplicate: false,
      outcome: plan.outcome,
    },
  });
  reportImportProgress(options, "complete", 4, 4);
  return item;
}

export async function fetchItemFile(
  id: string,
  store?: LocalReplicaStore
): Promise<ArrayBuffer> {
  const local = store ?? (await getBrowserLocalReplicaStore());
  const content = await local.libraryContent(id);
  if (!content?.fileBytes) {
    throw new Error(`Library item file not found: ${id}`);
  }
  return content.fileBytes.slice().buffer as ArrayBuffer;
}

export async function createTTSJob(
  params: TTSJobCreateParams
): Promise<TTSJobStatusResponse> {
  const res = await apiFetch(`${BASE}/api/tts/jobs`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(params),
  });
  if (!res.ok) throw new Error(`Failed to create TTS job: ${res.status}`);
  return res.json();
}

export async function pollTTSJob(
  jobId: string,
  includeAudio: boolean = false
): Promise<TTSJobStatusResponse> {
  const url = new URL(`/api/tts/jobs/${jobId}`, window.location.origin);
  if (includeAudio) url.searchParams.set("include_audio", "true");
  const res = await apiFetch(url.toString());
  if (!res.ok) throw new Error(`Failed to poll TTS job: ${res.status}`);
  return res.json();
}

export async function cancelTTSSession(
  sessionId: string,
  keepItemId?: string,
  keepChapterId?: string
): Promise<{ cancelled_job_ids: string[] }> {
  const res = await apiFetch(`${BASE}/api/tts/sessions/${sessionId}/cancel`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      keep_item_id: keepItemId ?? null,
      keep_chapter_id: keepChapterId ?? null,
    }),
  });
  if (!res.ok)
    throw new Error(`Failed to cancel TTS session: ${res.status}`);
  return res.json();
}

export async function updateProgress(
  id: string,
  progress: number,
  store?: LocalReplicaStore
): Promise<void> {
  await updateLocalLibraryItem(id, { progress }, store);
}

export async function updateVoice(
  id: string,
  voice: string | null,
  store?: LocalReplicaStore
): Promise<void> {
  await updateLocalLibraryItem(id, { voice }, store);
}

export async function updateSpeed(
  id: string,
  speed: number | null,
  store?: LocalReplicaStore
): Promise<void> {
  await updateLocalLibraryItem(id, { speed }, store);
}

export async function updateChapter(
  id: string,
  chapter: string | null,
  store?: LocalReplicaStore
): Promise<void> {
  await updateLocalLibraryItem(id, { current_chapter: chapter }, store);
}

async function updateLocalLibraryItem(
  id: string,
  patch: Partial<
    Pick<LibraryItem, "progress" | "voice" | "speed" | "current_chapter">
  >,
  store?: LocalReplicaStore
): Promise<void> {
  const local = store ?? (await getBrowserLocalReplicaStore());
  const item = await local.libraryItem(id);
  if (!item) throw new Error(`Library item not found: ${id}`);
  await local.upsert([
    {
      entityKind: "library_item",
      entityId: id,
      payload: { ...item, ...patch },
    },
  ]);
}

export async function fetchVoices(): Promise<VoiceListResponse> {
  const res = await apiFetch(`${BASE}/api/tts/voices`);
  if (!res.ok) throw new Error(`Failed to fetch voices: ${res.status}`);
  return res.json();
}

export async function fetchVoicesByProvider(provider: string): Promise<VoiceListResponse> {
  const res = await apiFetch(`${BASE}/api/tts/voices?provider=${encodeURIComponent(provider)}`);
  if (!res.ok) {
    const body = await res.json().catch(() => null);
    throw new Error(body?.detail ?? `Failed to fetch voices: ${res.status}`);
  }
  return res.json();
}

export interface ModelInfo {
  model_id: string;
  label: string;
  description: string | null;
}

export interface ModelListResponse {
  models: ModelInfo[];
  default_model_id: string;
}

export async function fetchModels(provider: string): Promise<ModelListResponse> {
  const res = await apiFetch(`${BASE}/api/tts/models?provider=${encodeURIComponent(provider)}`);
  if (!res.ok) throw new Error(`Failed to fetch models: ${res.status}`);
  return res.json();
}

// ── User settings persistence ────────────────────────────────────────────

export async function fetchSettings(
  store?: LocalReplicaStore
): Promise<Record<string, string>> {
  return (store ?? (await getBrowserLocalReplicaStore())).settings();
}

export async function patchSettings(
  data: Record<string, string | null>,
  store?: LocalReplicaStore
): Promise<void> {
  const local = store ?? (await getBrowserLocalReplicaStore());
  const upserts = Object.entries(data)
    .filter((entry): entry is [string, string] => entry[1] !== null)
    .map(([entityId, value]) => ({
      entityKind: "setting" as const,
      entityId,
      payload: { value },
    }));
  const deletions = Object.entries(data)
    .filter(([, value]) => value === null)
    .map(([entityId]) => ({ entityKind: "setting" as const, entityId }));
  await local.patch(upserts, deletions);
}

// ── API Keys management ──────────────────────────────────────────────────

export interface ApiKeyStatus {
  configured: boolean;
  masked: string | null;
  has_global_fallback: boolean;
}

export async function fetchApiKeys(): Promise<Record<string, ApiKeyStatus>> {
  const res = await apiFetch(`${BASE}/api/settings/api-keys`);
  if (!res.ok) throw new Error(`Failed to fetch API keys: ${res.status}`);
  return res.json();
}
