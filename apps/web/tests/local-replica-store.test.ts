import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

import {
  createReadioSdkClient,
  type ReadioSdkWasmModule,
} from "../lib/readio-sdk.ts"
import {
  WebLocalReplicaStore,
  type LocalLibraryContentPersistence,
  type LocalLibraryContentRecord,
  type LocalReplicaPersistence,
  type LocalReplicaRecord,
} from "../lib/local-replica-store.ts"
import type {
  LibraryContentCloudMetadata,
  LibraryContentCloudTransport,
  ReplicaPullPage,
  ReplicaPushRequest,
  ReplicaPushResult,
  ReplicaTransport,
} from "../lib/replica-sync.ts"
import init, * as generatedWasm from "../wasm/readio_wasm.js"

class MemoryPersistence implements LocalReplicaPersistence {
  record: LocalReplicaRecord | null = null

  async load(): Promise<LocalReplicaRecord | null> {
    return this.record === null ? null : structuredClone(this.record)
  }

  async save(record: LocalReplicaRecord): Promise<void> {
    this.record = structuredClone(record)
  }
}

class MemoryContentPersistence implements LocalLibraryContentPersistence {
  records = new Map<string, LocalLibraryContentRecord>()
  async load(itemId: string) { return this.records.get(itemId) ?? null }
  async save(record: LocalLibraryContentRecord) {
    this.records.set(record.itemId, structuredClone(record))
  }
  async delete(itemId: string) { this.records.delete(itemId) }
}

class SuccessfulReplicaTransport implements ReplicaTransport {
  snapshotCursor: number | null = null
  async pull(after: number, _limit: number): Promise<ReplicaPullPage> {
    return { cursor: after, hasMore: false, changes: [] }
  }
  async push(request: ReplicaPushRequest): Promise<ReplicaPushResult> {
    return {
      cursor: request.ackCursor,
      acceptedChangeIds: request.changes.map((change) => change.changeId),
    }
  }
  async uploadSnapshot(
    _bytes: Uint8Array,
    _deviceId: string,
    cursor: number
  ) {
    this.snapshotCursor = cursor
  }
}

class EmptyContentTransport implements LibraryContentCloudTransport {
  uploads: Array<{ itemId: string; variant: string }> = []
  async head() { return null }
  async upload(
    itemId: string,
    variant: "reader" | "source",
    data: Uint8Array,
    contentType: string,
    sha256Base64: string
  ): Promise<LibraryContentCloudMetadata> {
    this.uploads.push({ itemId, variant })
    return {
      itemId,
      variant,
      byteLength: data.byteLength,
      contentType,
      sha256Base64,
      etag: "fixture",
    }
  }
  async download() { return null }
}

async function realSdk() {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  await init({ module_or_path: binary })
  return createReadioSdkClient(
    async () => generatedWasm as unknown as ReadioSdkWasmModule
  )
}

test("local Web mutations persist the shared Rust snapshot and deletion outbox", async () => {
  const sdk = await realSdk()
  const persistence = new MemoryPersistence()
  const first = new WebLocalReplicaStore({ sdk, persistence })

  await first.upsert([
    {
      entityKind: "bookmark",
      entityId: "bookmark-1",
      payload: {
        id: "bookmark-1",
        item_id: "book-1",
        char_offset: 42,
        affinity: "forward",
        note: "Quiet passage",
        created_at_unix_ms: 10,
        updated_at_unix_ms: 10,
        version: 1,
      },
    },
  ])

  const restored = new WebLocalReplicaStore({ sdk, persistence })
  assert.deepEqual((await restored.state()).bookmarks?.["bookmark-1"], {
    id: "bookmark-1",
    item_id: "book-1",
    char_offset: 42,
    affinity: "forward",
    note: "Quiet passage",
    created_at_unix_ms: 10,
    updated_at_unix_ms: 10,
    version: 1,
  })
  assert.deepEqual(await restored.bookmarks("book-1"), [
    {
      id: "bookmark-1",
      item_id: "book-1",
      char_offset: 42,
      affinity: "forward",
      note: "Quiet passage",
      created_at_unix_ms: 10,
      updated_at_unix_ms: 10,
      version: 1,
    },
  ])

  await restored.delete([
    { entityKind: "bookmark", entityId: "bookmark-1" },
  ])

  assert.deepEqual((await first.state()).bookmarks, {})
  assert.equal(JSON.parse(persistence.record!.stateJSON).outbox.length, 2)
})

test("concurrent local writes serialize without dropping either Rust change", async () => {
  const sdk = await realSdk()
  const persistence = new MemoryPersistence()
  const store = new WebLocalReplicaStore({ sdk, persistence })

  await Promise.all([
    store.upsert([
      {
        entityKind: "setting",
        entityId: "theme",
        payload: { value: "dark" },
      },
    ]),
    store.upsert([
      {
        entityKind: "reading_position",
        entityId: "book-1",
        payload: { char_offset: 144 },
      },
    ]),
  ])

  assert.deepEqual(await store.state(), {
    bookmarks: {},
    library_items: {},
    reading_positions: { "book-1": { char_offset: 144 } },
    settings: { theme: { value: "dark" } },
  })
  assert.deepEqual(await store.settings(), { theme: "dark" })
  assert.equal(JSON.parse(persistence.record!.stateJSON).outbox.length, 2)
})

test("local Web cloud sync serializes Rust state and both content variants", async () => {
  const sdk = await realSdk()
  const persistence = new MemoryPersistence()
  const contentPersistence = new MemoryContentPersistence()
  const replica = new SuccessfulReplicaTransport()
  const content = new EmptyContentTransport()
  const receivedOptions: unknown[] = []
  const store = new WebLocalReplicaStore({
    sdk,
    persistence,
    contentPersistence,
    cloudTransportFactory(options) {
      receivedOptions.push(options)
      return { replica, content }
    },
  })
  await store.upsert([
    {
      entityKind: "library_item",
      entityId: "book-1",
      payload: {
        id: "book-1",
        title: "Cloud book",
        type: "txt",
        progress: 0,
        date: "2026-08-06",
        category: "imported",
        folder_id: "f1",
        source: "cloud.txt",
        file_path: "local:book-1",
        cover_image: null,
        voice: null,
        speed: null,
        current_chapter: null,
        content_format: "plaintext",
      },
    },
  ])
  await store.saveLibraryContent({
    itemId: "book-1",
    content: "Cloud reader content",
    fileBytes: new Uint8Array([1, 2, 3]),
    mediaType: "text/plain",
    fileName: "cloud.txt",
  })

  const state = await store.syncCloud({
    baseURL: "https://readio.test/api/cloud/",
    accessToken: "token",
    deviceId: "web-device",
  })

  assert.deepEqual(state.library_items?.["book-1"], {
    id: "book-1",
    title: "Cloud book",
    type: "txt",
    progress: 0,
    date: "2026-08-06",
    category: "imported",
    folder_id: "f1",
    source: "cloud.txt",
    file_path: "local:book-1",
    cover_image: null,
    voice: null,
    speed: null,
    current_chapter: null,
    content_format: "plaintext",
  })
  assert.deepEqual(receivedOptions, [
    {
      baseURL: "https://readio.test/api/cloud/",
      accessToken: "token",
      deviceId: "web-device",
    },
  ])
  assert.equal(replica.snapshotCursor, 0)
  assert.deepEqual(content.uploads, [
    { itemId: "book-1", variant: "reader" },
    { itemId: "book-1", variant: "source" },
  ])
})

test("local Web library reads use the shared Rust query projection", async () => {
  const sdk = await realSdk()
  const store = new WebLocalReplicaStore({
    sdk,
    persistence: new MemoryPersistence(),
  })
  const item = (id: string, title: string) => ({
    id,
    title,
    type: "txt",
    progress: 0,
    date: "2026-01-01",
    category: "imported",
    folder_id: "f1",
    source: `upload:${id}`,
    file_path: `local:${id}`,
    cover_image: null,
    voice: null,
    speed: null,
    current_chapter: null,
    content_format: null,
  })
  await store.upsert([
    { entityKind: "library_item", entityId: "b", payload: item("b", "Beta") },
    { entityKind: "library_item", entityId: "a", payload: item("a", "Alpha") },
  ])

  const page = await store.queryLibrary({
    page: 1,
    page_size: 20,
    sort_by: "title",
    sort_order: "ascending",
  })

  assert.deepEqual(
    page.items.map(({ id }) => id),
    ["a", "b"]
  )
  assert.equal((await store.libraryItem("a"))?.title, "Alpha")
})

test("local Web reading positions use the shared Rust projection", async () => {
  const sdk = await realSdk()
  const store = new WebLocalReplicaStore({
    sdk,
    persistence: new MemoryPersistence(),
  })
  const position = {
    document_revision: "revision-1",
    char_offset: 144,
    affinity: "forward" as const,
    observed_at_unix_ms: 1_700_000_000_000,
    session_id: "session-a",
    mutation_id: "mutation-a",
    base_version: 0,
  }
  const transition = await store.saveReadingPosition("book-1", position)

  assert.equal(transition.version, 1)
  assert.deepEqual(await store.readingPosition("book-1"), {
    ...position,
    version: 1,
  })
  assert.equal(await store.readingPosition("missing"), null)
})
