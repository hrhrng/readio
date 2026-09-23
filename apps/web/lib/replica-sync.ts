import type {
  LocalLibraryContentPersistence,
  LocalLibraryContentRecord,
} from "./local-replica-store.ts"
import type {
  LocalReplicaEntity,
  PendingReplicaChange,
  ReadioSdkClient,
  ReplicaState,
} from "./readio-sdk.ts"
import { webDiagnostics, type WebDiagnosticLog } from "./diagnostics.ts"

export interface ReplicaStoredChange extends PendingReplicaChange {
  cursor: number
  deviceId: string
}

export interface ReplicaPullPage {
  cursor: number
  hasMore: boolean
  changes: ReplicaStoredChange[]
}

export interface ReplicaPushRequest {
  deviceId: string
  ackCursor: number
  headsBase64: string
  changes: PendingReplicaChange[]
}

export interface ReplicaPushResult {
  cursor: number
  acceptedChangeIds: string[]
}

export interface ReplicaSyncRecord {
  cursor: number
  stateJSON: string
}

export interface ReplicaSyncPersistence {
  load(): Promise<ReplicaSyncRecord | null>
  save(record: ReplicaSyncRecord): Promise<void>
}

export interface ReplicaTransport {
  pull(after: number, limit: number): Promise<ReplicaPullPage>
  push(request: ReplicaPushRequest): Promise<ReplicaPushResult>
  uploadSnapshot(
    bytes: Uint8Array,
    deviceId: string,
    cursor: number
  ): Promise<void>
}

export type LibraryContentCloudVariant = "reader" | "source"

export interface LibraryContentCloudMetadata {
  itemId: string
  variant: LibraryContentCloudVariant
  byteLength: number
  contentType: string
  sha256Base64: string
  etag: string
}

export interface LibraryContentCloudDownload {
  metadata: LibraryContentCloudMetadata
  data: Uint8Array
}

export interface LibraryContentCloudTransport {
  head(
    itemId: string,
    variant: LibraryContentCloudVariant
  ): Promise<LibraryContentCloudMetadata | null>
  upload(
    itemId: string,
    variant: LibraryContentCloudVariant,
    data: Uint8Array,
    contentType: string,
    sha256Base64: string
  ): Promise<LibraryContentCloudMetadata>
  download(
    itemId: string,
    variant: LibraryContentCloudVariant
  ): Promise<LibraryContentCloudDownload | null>
}

export interface LibraryContentCloudReconciling {
  reconcile(state: ReplicaState): Promise<void>
}

export class IndexedDBReplicaSyncPersistence
  implements ReplicaSyncPersistence
{
  private readonly databaseName: string
  private readonly indexedDB: IDBFactory

  constructor(options: { databaseName?: string; indexedDB?: IDBFactory } = {}) {
    this.databaseName = options.databaseName ?? "readio-local"
    this.indexedDB = options.indexedDB ?? globalThis.indexedDB
    if (!this.indexedDB) {
      throw new Error("IndexedDB is unavailable in this browser")
    }
  }

  async load(): Promise<ReplicaSyncRecord | null> {
    const database = await this.open()
    try {
      const transaction = database.transaction(REPLICA_STORE, "readonly")
      const record = await idbRequest<ReplicaSyncRecord | undefined>(
        transaction.objectStore(REPLICA_STORE).get(REPLICA_RECORD_KEY)
      )
      await idbTransaction(transaction)
      return record ?? null
    } finally {
      database.close()
    }
  }

  async save(record: ReplicaSyncRecord): Promise<void> {
    const database = await this.open()
    try {
      const transaction = database.transaction(REPLICA_STORE, "readwrite")
      transaction.objectStore(REPLICA_STORE).put(record, REPLICA_RECORD_KEY)
      await idbTransaction(transaction)
    } finally {
      database.close()
    }
  }

  private open(): Promise<IDBDatabase> {
    return openReadioIndexedDB(this.indexedDB, this.databaseName)
  }
}

export class IndexedDBLibraryContentPersistence
  implements LocalLibraryContentPersistence
{
  private readonly databaseName: string
  private readonly indexedDB: IDBFactory

  constructor(options: { databaseName?: string; indexedDB?: IDBFactory } = {}) {
    this.databaseName = options.databaseName ?? "readio-local"
    this.indexedDB = options.indexedDB ?? globalThis.indexedDB
    if (!this.indexedDB) {
      throw new Error("IndexedDB is unavailable in this browser")
    }
  }

  async load(itemId: string): Promise<LocalLibraryContentRecord | null> {
    const database = await openReadioIndexedDB(
      this.indexedDB,
      this.databaseName
    )
    try {
      const transaction = database.transaction(CONTENT_STORE, "readonly")
      const record = await idbRequest<
        LocalLibraryContentRecord | undefined
      >(transaction.objectStore(CONTENT_STORE).get(itemId))
      await idbTransaction(transaction)
      return record ?? null
    } finally {
      database.close()
    }
  }

  async save(record: LocalLibraryContentRecord): Promise<void> {
    const database = await openReadioIndexedDB(
      this.indexedDB,
      this.databaseName
    )
    try {
      const transaction = database.transaction(CONTENT_STORE, "readwrite")
      transaction.objectStore(CONTENT_STORE).put(record, record.itemId)
      await idbTransaction(transaction)
    } finally {
      database.close()
    }
  }

  async delete(itemId: string): Promise<void> {
    const database = await openReadioIndexedDB(
      this.indexedDB,
      this.databaseName
    )
    try {
      const transaction = database.transaction(CONTENT_STORE, "readwrite")
      transaction.objectStore(CONTENT_STORE).delete(itemId)
      await idbTransaction(transaction)
    } finally {
      database.close()
    }
  }
}

export class FetchReplicaTransport implements ReplicaTransport {
  private readonly baseURL: URL
  private readonly accessToken?: string
  private readonly fetch: typeof fetch

  constructor(options: {
    baseURL: URL
    accessToken?: string
    fetch?: typeof fetch
  }) {
    this.baseURL = options.baseURL
    this.accessToken = options.accessToken
    this.fetch = options.fetch ?? globalThis.fetch.bind(globalThis)
  }

  async pull(after: number, limit: number): Promise<ReplicaPullPage> {
    const url = this.endpoint("/v1/replica/pull")
    url.searchParams.set("cursor", String(after))
    url.searchParams.set("limit", String(limit))
    return this.json<ReplicaPullPage>(url, {
      method: "GET",
      credentials: "include",
      headers: this.headers(),
    })
  }

  async push(request: ReplicaPushRequest): Promise<ReplicaPushResult> {
    return this.json<ReplicaPushResult>(this.endpoint("/v1/replica/push"), {
      method: "POST",
      credentials: "include",
      headers: this.headers({ "content-type": "application/json" }),
      body: JSON.stringify(request),
    })
  }

  async uploadSnapshot(
    bytes: Uint8Array,
    deviceId: string,
    cursor: number
  ): Promise<void> {
    const response = await this.fetch(this.endpoint("/v1/replica/snapshot"), {
      method: "PUT",
      credentials: "include",
      headers: this.headers({
        "content-type": "application/octet-stream",
        "x-readio-device-id": deviceId,
        "x-readio-replica-cursor": String(cursor),
      }),
      // Blob gives browsers a known request length without attempting to set
      // the forbidden Content-Length header from application code.
      body: new Blob([Uint8Array.from(bytes).buffer], {
        type: "application/octet-stream",
      }),
    })
    assertSuccessfulResponse(response)
  }

  private endpoint(path: string): URL {
    return cloudEndpoint(this.baseURL, path)
  }

  private headers(initial?: HeadersInit): Headers {
    const headers = new Headers(initial)
    if (this.accessToken) {
      headers.set("authorization", `Bearer ${this.accessToken}`)
    }
    return headers
  }

  private async json<T>(url: URL, init: RequestInit): Promise<T> {
    const response = await this.fetch(url, init)
    assertSuccessfulResponse(response)
    return (await response.json()) as T
  }
}

export class FetchLibraryContentTransport
  implements LibraryContentCloudTransport
{
  private readonly baseURL: URL
  private readonly accessToken?: string
  private readonly fetch: typeof fetch

  constructor(options: {
    baseURL: URL
    accessToken?: string
    fetch?: typeof fetch
  }) {
    this.baseURL = options.baseURL
    this.accessToken = options.accessToken
    this.fetch = options.fetch ?? globalThis.fetch.bind(globalThis)
  }

  async head(
    itemId: string,
    variant: LibraryContentCloudVariant
  ): Promise<LibraryContentCloudMetadata | null> {
    const response = await this.fetch(this.endpoint(itemId, variant), {
      method: "HEAD",
      credentials: "include",
      headers: this.headers(),
    })
    if (response.status === 404) return null
    assertSuccessfulResponse(response)
    return contentMetadataFromHeaders(response, itemId, variant)
  }

  async upload(
    itemId: string,
    variant: LibraryContentCloudVariant,
    data: Uint8Array,
    contentType: string,
    sha256Base64: string
  ): Promise<LibraryContentCloudMetadata> {
    const response = await this.fetch(this.endpoint(itemId, variant), {
      method: "PUT",
      credentials: "include",
      headers: this.headers({
        "content-type": contentType,
        "x-readio-content-sha256": sha256Base64,
      }),
      body: new Blob([Uint8Array.from(data).buffer], { type: contentType }),
    })
    assertSuccessfulResponse(response)
    const value: unknown = await response.json()
    if (!isLibraryContentCloudMetadata(value, itemId, variant)) {
      throw new Error("Cloud returned invalid library content metadata")
    }
    return value
  }

  async download(
    itemId: string,
    variant: LibraryContentCloudVariant
  ): Promise<LibraryContentCloudDownload | null> {
    const response = await this.fetch(this.endpoint(itemId, variant), {
      method: "GET",
      credentials: "include",
      headers: this.headers(),
    })
    if (response.status === 404) return null
    assertSuccessfulResponse(response)
    const metadata = contentMetadataFromHeaders(response, itemId, variant)
    const data = new Uint8Array(await response.arrayBuffer())
    if (data.byteLength !== metadata.byteLength) {
      throw new Error("Cloud library content length does not match its metadata")
    }
    return { metadata, data }
  }

  private endpoint(
    itemId: string,
    variant: LibraryContentCloudVariant
  ): URL {
    return cloudEndpoint(
      this.baseURL,
      `/v1/library/content/${encodeURIComponent(itemId)}/${variant}`,
    )
  }

  private headers(initial?: HeadersInit): Headers {
    const headers = new Headers(initial)
    if (this.accessToken) {
      headers.set("authorization", `Bearer ${this.accessToken}`)
    }
    return headers
  }
}

export class WebLibraryContentCloudReconciler
  implements LibraryContentCloudReconciling
{
  private readonly contentPersistence: LocalLibraryContentPersistence
  private readonly transport: LibraryContentCloudTransport

  constructor(options: {
    contentPersistence: LocalLibraryContentPersistence
    transport: LibraryContentCloudTransport
  }) {
    this.contentPersistence = options.contentPersistence
    this.transport = options.transport
  }

  async reconcile(state: ReplicaState): Promise<void> {
    const items = isRecord(state.library_items) ? state.library_items : {}
    for (const [itemId, rawItem] of Object.entries(items).sort(([a], [b]) =>
      a.localeCompare(b)
    )) {
      const item = isRecord(rawItem) ? rawItem : {}
      const local = await this.contentPersistence.load(itemId)
      if (local) {
        await this.publish(
          new TextEncoder().encode(local.content),
          itemId,
          "reader",
          readerContentType(item.type)
        )
        if (local.fileBytes) {
          await this.publish(
            local.fileBytes,
            itemId,
            "source",
            local.mediaType ?? "application/octet-stream"
          )
        } else {
          const source = await this.transport.download(itemId, "source")
          if (source) {
            await this.contentPersistence.save({
              ...local,
              fileBytes: source.data,
              mediaType: source.metadata.contentType,
              fileName: local.fileName ?? inferredFileName(item.source),
            })
          }
        }
        continue
      }

      const reader = await this.transport.download(itemId, "reader")
      if (!reader) continue
      const source = await this.transport.download(itemId, "source")
      await this.contentPersistence.save({
        itemId,
        content: new TextDecoder("utf-8", { fatal: true }).decode(reader.data),
        fileBytes: source?.data ?? null,
        mediaType: source?.metadata.contentType ?? null,
        fileName: inferredFileName(item.source),
      })
    }
  }

  private async publish(
    data: Uint8Array,
    itemId: string,
    variant: LibraryContentCloudVariant,
    contentType: string
  ): Promise<void> {
    if (data.byteLength === 0) return
    const digest = await crypto.subtle.digest(
      "SHA-256",
      Uint8Array.from(data).buffer
    )
    const sha256Base64 = bytesToBase64(new Uint8Array(digest))
    const remote = await this.transport.head(itemId, variant)
    if (
      remote?.byteLength === data.byteLength &&
      remote.sha256Base64 === sha256Base64
    ) {
      return
    }
    await this.transport.upload(
      itemId,
      variant,
      data,
      contentType,
      sha256Base64
    )
  }
}

export class WebReplicaSyncCoordinator {
  private readonly deviceId: string
  private readonly sdk: ReadioSdkClient
  private readonly persistence: ReplicaSyncPersistence
  private readonly transport: ReplicaTransport
  private readonly contentReconciler?: LibraryContentCloudReconciling
  private readonly diagnostics: Pick<WebDiagnosticLog, "beginOperation" | "record">

  constructor(options: {
    deviceId: string
    sdk: ReadioSdkClient
    persistence: ReplicaSyncPersistence
    transport: ReplicaTransport
    contentReconciler?: LibraryContentCloudReconciling
    diagnostics?: Pick<WebDiagnosticLog, "beginOperation" | "record">
  }) {
    this.deviceId = options.deviceId
    this.sdk = options.sdk
    this.persistence = options.persistence
    this.transport = options.transport
    this.contentReconciler = options.contentReconciler
    this.diagnostics = options.diagnostics ?? webDiagnostics
  }

  async sync(
    localEntities: LocalReplicaEntity[]
  ): Promise<{ cursor: number; state: ReplicaState }> {
    const operationId = this.diagnostics.beginOperation()
    this.diagnostics.record({
      operationId,
      component: "web-replica-sync",
      category: "sync",
      name: "sync_started",
      fields: { phase: "sync", direction: "bidirectional" },
    })
    let record = (await this.persistence.load()) ?? EMPTY_REPLICA_RECORD
    const state = await this.sdk.newSyncState(record.stateJSON)

    try {
      record = await this.pullAll(state, record, operationId)

      state.stageLocal(localEntities)
      record = { ...record, stateJSON: state.serialize() }
      // Persisting the stable change IDs before the first request makes a
      // crash replay idempotent instead of losing a remotely visible change.
      await this.persistence.save(record)

      record = await this.pushAll(state, record, operationId)
      // The push cursor is only a remote high-water mark. Pulling from the
      // imported cursor prevents concurrent changes from being skipped.
      record = await this.pullAll(state, record, operationId)
      await this.transport.uploadSnapshot(
        state.snapshot(),
        this.deviceId,
        record.cursor
      )
      this.diagnostics.record({
        operationId,
        component: "web-replica-sync",
        category: "sync",
        name: "snapshot_uploaded",
        fields: {
          phase: "snapshot",
          direction: "push",
          cursor: record.cursor,
          outcome: "completed",
        },
      })

      const projectedState = state.state()
      await this.contentReconciler?.reconcile(projectedState)
      this.diagnostics.record({
        operationId,
        component: "web-replica-sync",
        category: "sync",
        name: "sync_completed",
        fields: {
          phase: "sync",
          direction: "bidirectional",
          cursor: record.cursor,
          outcome: "completed",
        },
      })
      return { cursor: record.cursor, state: projectedState }
    } catch (error) {
      this.diagnostics.record({
        operationId,
        component: "web-replica-sync",
        category: "sync",
        name: "sync_failed",
        fields: {
          phase: "sync",
          direction: "bidirectional",
          cursor: record.cursor,
          outcome: "failed",
          reason_code: error instanceof ReplicaHTTPError
            ? `http_${error.status}`
            : "sync_failed",
        },
      })
      throw error
    } finally {
      state.free()
    }
  }

  private async pullAll(
    state: Awaited<ReturnType<ReadioSdkClient["newSyncState"]>>,
    initialRecord: ReplicaSyncRecord,
    operationId: string
  ): Promise<ReplicaSyncRecord> {
    let record = initialRecord
    while (true) {
      const page = await this.transport.pull(record.cursor, REPLICA_PULL_LIMIT)
      if (page.hasMore && page.cursor === record.cursor) {
        throw new Error("Replica pull did not advance its cursor")
      }
      state.importRemote(
        page.changes.map(({ changeId, changeBase64 }) => ({
          changeId,
          changeBase64,
        }))
      )
      record = { cursor: page.cursor, stateJSON: state.serialize() }
      await this.persistence.save(record)
      this.diagnostics.record({
        operationId,
        component: "web-replica-sync",
        category: "sync",
        name: "pull_completed",
        fields: {
          phase: "pull",
          direction: "pull",
          change_count: page.changes.length,
          cursor: page.cursor,
          outcome: "completed",
        },
      })
      if (!page.hasMore) return record
    }
  }

  private async pushAll(
    state: Awaited<ReturnType<ReadioSdkClient["newSyncState"]>>,
    initialRecord: ReplicaSyncRecord,
    operationId: string
  ): Promise<ReplicaSyncRecord> {
    let record = initialRecord
    while (true) {
      const before = state.outbox()
      const result = await this.transport.push({
        deviceId: this.deviceId,
        ackCursor: record.cursor,
        headsBase64: bytesToBase64(state.heads()),
        changes: before.slice(0, REPLICA_PUSH_LIMIT),
      })
      state.acknowledge(result.acceptedChangeIds)
      record = { ...record, stateJSON: state.serialize() }
      await this.persistence.save(record)
      this.diagnostics.record({
        operationId,
        component: "web-replica-sync",
        category: "sync",
        name: "push_completed",
        fields: {
          phase: "push",
          direction: "push",
          change_count: result.acceptedChangeIds.length,
          cursor: record.cursor,
          outcome: "completed",
        },
      })

      const remaining = state.outbox()
      if (remaining.length === 0) return record
      if (remaining.length === before.length) {
        throw new Error("Cloud did not acknowledge any replica changes")
      }
    }
  }
}

export class ReplicaHTTPError extends Error {
  constructor(readonly status: number) {
    super(`Replica request failed with HTTP ${status}`)
    this.name = "ReplicaHTTPError"
  }
}

const REPLICA_STORE = "replica-sync"
const CONTENT_STORE = "library-content"
const INDEXED_DB_VERSION = 2
const REPLICA_RECORD_KEY = "primary"
const REPLICA_PULL_LIMIT = 100
// Cloud validates a bounded batch before Rust applies it.
const REPLICA_PUSH_LIMIT = 32
const MAX_READER_CONTENT_BYTES = 32 * 1024 * 1024
const MAX_SOURCE_CONTENT_BYTES = 200 * 1024 * 1024
const SHA256_BASE64_PATTERN = /^[A-Za-z0-9+/]{43}=$/
const EMPTY_REPLICA_RECORD: ReplicaSyncRecord = {
  cursor: 0,
  stateJSON: "{}",
}

function openReadioIndexedDB(
  indexedDB: IDBFactory,
  databaseName: string
): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(databaseName, INDEXED_DB_VERSION)
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains(REPLICA_STORE)) {
        request.result.createObjectStore(REPLICA_STORE)
      }
      if (!request.result.objectStoreNames.contains(CONTENT_STORE)) {
        request.result.createObjectStore(CONTENT_STORE)
      }
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
    request.onblocked = () =>
      reject(new Error("Readio IndexedDB upgrade is blocked"))
  })
}

function idbRequest<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
  })
}

function idbTransaction(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve()
    transaction.onerror = () => reject(transaction.error)
    transaction.onabort = () => reject(transaction.error)
  })
}

function assertSuccessfulResponse(response: Response): void {
  if (!response.ok) throw new ReplicaHTTPError(response.status)
}

function cloudEndpoint(baseURL: URL, path: string): URL {
  const directory = new URL(baseURL)
  if (!directory.pathname.endsWith("/")) directory.pathname += "/"
  return new URL(path.replace(/^\/+/, ""), directory)
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
}

function readerContentType(value: unknown): string {
  return value === "epub"
    ? "application/vnd.readio.epub+json"
    : "text/plain; charset=utf-8"
}

function inferredFileName(value: unknown): string | null {
  if (typeof value !== "string" || value.length === 0) return null
  try {
    const url = new URL(value)
    return url.pathname.split("/").filter(Boolean).at(-1) ?? null
  } catch {
    return value.replaceAll("\\", "/").split("/").filter(Boolean).at(-1) ?? null
  }
}

function isLibraryContentCloudMetadata(
  value: unknown,
  itemId: string,
  variant: LibraryContentCloudVariant
): value is LibraryContentCloudMetadata {
  if (!isRecord(value)) return false
  const maximumLength =
    variant === "reader" ? MAX_READER_CONTENT_BYTES : MAX_SOURCE_CONTENT_BYTES
  return (
    value.itemId === itemId &&
    value.variant === variant &&
    typeof value.byteLength === "number" &&
    Number.isSafeInteger(value.byteLength) &&
    value.byteLength > 0 &&
    value.byteLength <= maximumLength &&
    typeof value.contentType === "string" &&
    value.contentType.length > 0 &&
    value.contentType.length <= 256 &&
    typeof value.sha256Base64 === "string" &&
    SHA256_BASE64_PATTERN.test(value.sha256Base64) &&
    typeof value.etag === "string" &&
    value.etag.length > 0
  )
}

function contentMetadataFromHeaders(
  response: Response,
  itemId: string,
  variant: LibraryContentCloudVariant
): LibraryContentCloudMetadata {
  const value = {
    itemId,
    variant,
    byteLength: Number(response.headers.get("content-length")),
    contentType: response.headers.get("content-type"),
    sha256Base64: response.headers.get("x-readio-content-sha256"),
    etag: response.headers.get("etag"),
  }
  if (!isLibraryContentCloudMetadata(value, itemId, variant)) {
    throw new Error("Cloud returned invalid library content headers")
  }
  return value
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = ""
  for (const byte of bytes) binary += String.fromCharCode(byte)
  return btoa(binary)
}
