import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

import { IDBFactory } from "fake-indexeddb"
import * as replicaSyncModule from "../lib/replica-sync.ts"

import {
  createReadioSdkClient,
  type LocalReplicaEntity,
  type ReadioSdkWasmModule,
  type ReplicaState,
} from "../lib/readio-sdk.ts"
import {
  FetchLibraryContentTransport,
  FetchReplicaTransport,
  IndexedDBReplicaSyncPersistence,
  WebLibraryContentCloudReconciler,
  WebReplicaSyncCoordinator,
  type LibraryContentCloudMetadata,
  type LibraryContentCloudTransport,
  type ReplicaPullPage,
  type ReplicaPushRequest,
  type ReplicaPushResult,
  type ReplicaStoredChange,
  type ReplicaSyncPersistence,
  type ReplicaSyncRecord,
  type ReplicaTransport,
} from "../lib/replica-sync.ts"
import type {
  LocalLibraryContentPersistence,
  LocalLibraryContentRecord,
} from "../lib/local-replica-store.ts"
import init, * as generatedWasm from "../wasm/readio_wasm.js"
import { WebDiagnosticLog } from "../lib/diagnostics.ts"

async function realSdk() {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  await init({ module_or_path: binary })
  return createReadioSdkClient(
    async () => generatedWasm as unknown as ReadioSdkWasmModule
  )
}

class MemoryPersistence implements ReplicaSyncPersistence {
  record: ReplicaSyncRecord | null = null

  async load(): Promise<ReplicaSyncRecord | null> {
    return this.record === null ? null : structuredClone(this.record)
  }

  async save(record: ReplicaSyncRecord): Promise<void> {
    this.record = structuredClone(record)
  }
}

class ObservingTransport implements ReplicaTransport {
  private pushed = [] as ReplicaPushRequest["changes"]
  pushObservedDurableOutbox = false
  uploaded: { bytes: Uint8Array; deviceId: string; cursor: number } | null = null

  constructor(
    private readonly persistence: MemoryPersistence,
    private readonly remote: ReplicaStoredChange
  ) {}

  async pull(after: number, _limit: number): Promise<ReplicaPullPage> {
    if (after === 0) {
      return { cursor: 1, hasMore: false, changes: [this.remote] }
    }
    if (after === 1 && this.pushed[0]) {
      return {
        cursor: 2,
        hasMore: false,
        changes: [
          {
            cursor: 2,
            deviceId: "web-test",
            ...this.pushed[0],
          },
        ],
      }
    }
    return { cursor: after, hasMore: false, changes: [] }
  }

  async push(request: ReplicaPushRequest): Promise<ReplicaPushResult> {
    const durable = await this.persistence.load()
    this.pushObservedDurableOutbox =
      durable !== null && JSON.parse(durable.stateJSON).outbox.length > 0
    this.pushed = request.changes
    return {
      cursor: 2,
      acceptedChangeIds: request.changes.map((change) => change.changeId),
    }
  }

  async uploadSnapshot(
    bytes: Uint8Array,
    deviceId: string,
    cursor: number
  ): Promise<void> {
    this.uploaded = { bytes, deviceId, cursor }
  }
}

class ObservingContentReconciler {
  states: ReplicaState[] = []

  async reconcile(state: ReplicaState): Promise<void> {
    this.states.push(structuredClone(state))
  }
}

test("web sync durably stages before push and snapshots only after the post-push pull", async () => {
  const sdk = await realSdk()
  const remoteState = await sdk.newSyncState()
  remoteState.stageLocal([
    {
      entityKind: "reading_position",
      entityId: "remote-book",
      payload: { char_offset: 144 },
    },
  ])
  const [remoteChange] = remoteState.outbox()
  assert.ok(remoteChange)

  const persistence = new MemoryPersistence()
  const transport = new ObservingTransport(persistence, {
    cursor: 1,
    deviceId: "other-device",
    ...remoteChange,
  })
  const contentReconciler = new ObservingContentReconciler()
  const coordinator = new WebReplicaSyncCoordinator({
    deviceId: "web-test",
    sdk,
    persistence,
    transport,
    contentReconciler,
  })
  const local: LocalReplicaEntity = {
    entityKind: "setting",
    entityId: "preferences",
    payload: { theme: "dark" },
  }

  const result = await coordinator.sync([local])

  assert.equal(transport.pushObservedDurableOutbox, true)
  assert.equal(result.cursor, 2)
  assert.deepEqual(result.state.reading_positions?.["remote-book"], {
    char_offset: 144,
  })
  assert.deepEqual(result.state.settings?.preferences, { theme: "dark" })
  assert.equal(persistence.record?.cursor, 2)
  assert.deepEqual(JSON.parse(persistence.record!.stateJSON).outbox, [])
  assert.equal(transport.uploaded?.deviceId, "web-test")
  assert.equal(transport.uploaded?.cursor, 2)
  assert.ok((transport.uploaded?.bytes.byteLength ?? 0) > 0)
  assert.deepEqual(contentReconciler.states, [result.state])

  remoteState.free()
})

test("web sync records imported, pushed, and snapshotted facts under one operation", async () => {
  const sdk = await realSdk()
  const remoteState = await sdk.newSyncState()
  remoteState.stageLocal([
    {
      entityKind: "setting",
      entityId: "remote-preferences",
      payload: { theme: "light" },
    },
  ])
  const [remoteChange] = remoteState.outbox()
  assert.ok(remoteChange)
  const persistence = new MemoryPersistence()
  const diagnostics = new WebDiagnosticLog({
    now: () => 84,
    createOperationId: () => "op-web-sync",
  })
  const coordinator = new WebReplicaSyncCoordinator({
    deviceId: "web-test",
    sdk,
    persistence,
    transport: new ObservingTransport(persistence, {
      cursor: 1,
      deviceId: "remote-device",
      ...remoteChange,
    }),
    diagnostics,
  })

  await coordinator.sync([
    {
      entityKind: "setting",
      entityId: "local-preferences",
      payload: { theme: "dark" },
    },
  ])

  const events = diagnostics.snapshot()
  assert.ok(events.length >= 5)
  assert.ok(events.every((event) => event.operation_id === "op-web-sync"))
  assert.ok(
    events.some(
      (event) =>
        event.name === "pull_completed" &&
        event.fields.direction === "pull" &&
        event.fields.change_count === 1
    )
  )
  assert.ok(
    events.some(
      (event) =>
        event.name === "push_completed" &&
        event.fields.direction === "push" &&
        event.fields.change_count === 1
    )
  )
  assert.ok(
    events.some(
      (event) =>
        event.name === "snapshot_uploaded" && event.fields.cursor === 2
    )
  )
  remoteState.free()
})

class MemoryContentPersistence implements LocalLibraryContentPersistence {
  readonly records = new Map<string, LocalLibraryContentRecord>()

  async load(itemId: string): Promise<LocalLibraryContentRecord | null> {
    return this.records.get(itemId) ?? null
  }

  async save(record: LocalLibraryContentRecord): Promise<void> {
    this.records.set(record.itemId, structuredClone(record))
  }

  async delete(itemId: string): Promise<void> {
    this.records.delete(itemId)
  }
}

class MemoryCloudContentTransport implements LibraryContentCloudTransport {
  readonly stored = new Map<string, { data: Uint8Array; contentType: string }>()
  uploads = 0

  private key(itemId: string, variant: "reader" | "source"): string {
    return `${itemId}:${variant}`
  }

  async head(
    itemId: string,
    variant: "reader" | "source"
  ): Promise<LibraryContentCloudMetadata | null> {
    const value = this.stored.get(this.key(itemId, variant))
    return value ? this.metadata(itemId, variant, value) : null
  }

  async upload(
    itemId: string,
    variant: "reader" | "source",
    data: Uint8Array,
    contentType: string,
    sha256Base64: string
  ): Promise<LibraryContentCloudMetadata> {
    this.uploads += 1
    const value = { data: data.slice(), contentType }
    this.stored.set(this.key(itemId, variant), value)
    return {
      itemId,
      variant,
      byteLength: data.byteLength,
      contentType,
      sha256Base64,
      etag: "fixture",
    }
  }

  async download(itemId: string, variant: "reader" | "source") {
    const value = this.stored.get(this.key(itemId, variant))
    if (!value) return null
    return {
      metadata: await this.metadata(itemId, variant, value),
      data: value.data.slice(),
    }
  }

  private async metadata(
    itemId: string,
    variant: "reader" | "source",
    value: { data: Uint8Array; contentType: string }
  ): Promise<LibraryContentCloudMetadata> {
    const digest = await crypto.subtle.digest(
      "SHA-256",
      Uint8Array.from(value.data).buffer
    )
    return {
      itemId,
      variant,
      byteLength: value.data.byteLength,
      contentType: value.contentType,
      sha256Base64: Buffer.from(digest).toString("base64"),
      etag: "fixture",
    }
  }
}

test("web cloud content restores missing books and skips matching uploads", async () => {
  const local = new MemoryContentPersistence()
  await local.save({
    itemId: "local-book",
    content: "Local reader content",
    fileBytes: new Uint8Array([1, 2, 3]),
    mediaType: "application/epub+zip",
    fileName: "local.epub",
  })
  const cloud = new MemoryCloudContentTransport()
  cloud.stored.set("remote-book:reader", {
    data: new TextEncoder().encode("Remote reader content"),
    contentType: "text/plain; charset=utf-8",
  })
  cloud.stored.set("remote-book:source", {
    data: new Uint8Array([4, 5, 6]),
    contentType: "application/epub+zip",
  })
  const reconciler = new WebLibraryContentCloudReconciler({
    contentPersistence: local,
    transport: cloud,
  })
  const state = {
    library_items: {
      "local-book": { type: "epub", source: "local.epub" },
      "remote-book": { type: "epub", source: "remote.epub" },
    },
  }

  await reconciler.reconcile(state)
  assert.deepEqual(await local.load("remote-book"), {
    itemId: "remote-book",
    content: "Remote reader content",
    fileBytes: new Uint8Array([4, 5, 6]),
    mediaType: "application/epub+zip",
    fileName: "remote.epub",
  })
  assert.equal(cloud.uploads, 2)

  await reconciler.reconcile(state)
  assert.equal(cloud.uploads, 2)
})

test("web replica state survives a new IndexedDB persistence instance", async () => {
  const factory = new IDBFactory()
  const options = {
    databaseName: "readio-sync-test",
    indexedDB: factory,
  }
  const record = {
    cursor: 7,
    stateJSON: JSON.stringify({ outbox: [], snapshotBase64: "AQID" }),
  }

  await new IndexedDBReplicaSyncPersistence(options).save(record)

  assert.deepEqual(
    await new IndexedDBReplicaSyncPersistence(options).load(),
    record
  )
})

test("IndexedDB fallback preserves local library content and source bytes", async () => {
  const factory = new IDBFactory()
  const options = {
    databaseName: "readio-content-test",
    indexedDB: factory,
  }
  const ContentPersistence = Reflect.get(
    replicaSyncModule,
    "IndexedDBLibraryContentPersistence"
  )
  assert.equal(typeof ContentPersistence, "function")
  const first = new ContentPersistence(options)

  await first.save({
    itemId: "book-1",
    content: "offline",
    fileBytes: new Uint8Array([4, 5, 6]),
    mediaType: "text/plain",
    fileName: "offline.txt",
  })

  const restored = new ContentPersistence(options)
  assert.deepEqual(await restored.load("book-1"), {
    itemId: "book-1",
    content: "offline",
    fileBytes: new Uint8Array([4, 5, 6]),
    mediaType: "text/plain",
    fileName: "offline.txt",
  })
  await restored.delete("book-1")
  assert.equal(await restored.load("book-1"), null)
})

test("web Cloudflare transport sends the authenticated replica HTTP contract", async () => {
  const requests: Array<{ url: string; init: RequestInit }> = []
  const changeId = "a".repeat(64)
  const fetcher: typeof fetch = async (input, init = {}) => {
    requests.push({ url: String(input), init })
    if (requests.length === 1) {
      return Response.json({ cursor: 3, hasMore: false, changes: [] })
    }
    if (requests.length === 2) {
      return Response.json({ cursor: 4, acceptedChangeIds: [changeId] })
    }
    return Response.json(
      { cursor: 4, etag: "fixture", byteLength: 3 },
      { status: 201 }
    )
  }
  const transport = new FetchReplicaTransport({
    baseURL: new URL("https://readio.test/api/cloud/"),
    accessToken: "test-token",
    fetch: fetcher,
  })

  assert.equal((await transport.pull(3, 100)).cursor, 3)
  assert.deepEqual(
    await transport.push({
      deviceId: "web-http",
      ackCursor: 3,
      headsBase64: "BAUG",
      changes: [{ changeId, changeBase64: "AQID" }],
    }),
    { cursor: 4, acceptedChangeIds: [changeId] }
  )
  await transport.uploadSnapshot(new Uint8Array([1, 2, 3]), "web-http", 4)

  assert.equal(
    requests[0]?.url,
    "https://readio.test/api/cloud/v1/replica/pull?cursor=3&limit=100"
  )
  assert.equal(requests[0]?.init.method, "GET")
  assert.equal(requests[0]?.init.credentials, "include")
  assert.equal(
    new Headers(requests[0]?.init.headers).get("authorization"),
    "Bearer test-token"
  )

  assert.equal(requests[1]?.url, "https://readio.test/api/cloud/v1/replica/push")
  assert.equal(requests[1]?.init.method, "POST")
  assert.deepEqual(JSON.parse(String(requests[1]?.init.body)), {
    deviceId: "web-http",
    ackCursor: 3,
    headsBase64: "BAUG",
    changes: [{ changeId, changeBase64: "AQID" }],
  })

  const snapshotHeaders = new Headers(requests[2]?.init.headers)
  assert.equal(
    requests[2]?.url,
    "https://readio.test/api/cloud/v1/replica/snapshot"
  )
  assert.equal(requests[2]?.init.method, "PUT")
  assert.equal(snapshotHeaders.get("content-type"), "application/octet-stream")
  assert.equal(snapshotHeaders.get("x-readio-device-id"), "web-http")
  assert.equal(snapshotHeaders.get("x-readio-replica-cursor"), "4")
  assert.deepEqual(
    new Uint8Array(await (requests[2]?.init.body as Blob).arrayBuffer()),
    new Uint8Array([1, 2, 3])
  )
})

test("web Cloudflare content transport uses private HEAD, PUT, and GET streams", async () => {
  const requests: Array<{ url: string; init: RequestInit }> = []
  const digest = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
  const metadata = {
    itemId: "book/a",
    variant: "reader" as const,
    byteLength: 4,
    contentType: "text/plain; charset=utf-8",
    sha256Base64: digest,
    etag: '"content-etag"',
  }
  const fetcher: typeof fetch = async (input, init = {}) => {
    requests.push({ url: String(input), init })
    if (init.method === "HEAD") {
      return new Response(null, {
        headers: {
          "content-length": "4",
          "content-type": metadata.contentType,
          "x-readio-content-sha256": digest,
          etag: metadata.etag,
        },
      })
    }
    if (init.method === "PUT") {
      return Response.json(metadata, { status: 201 })
    }
    return new Response("book", {
      headers: {
        "content-length": "4",
        "content-type": metadata.contentType,
        "x-readio-content-sha256": digest,
        etag: metadata.etag,
      },
    })
  }
  const transport = new FetchLibraryContentTransport({
    baseURL: new URL("https://readio.test/api/cloud/"),
    accessToken: "test-token",
    fetch: fetcher,
  })

  assert.deepEqual(await transport.head("book/a", "reader"), metadata)
  assert.deepEqual(
    await transport.upload(
      "book/a",
      "reader",
      new Uint8Array([98, 111, 111, 107]),
      metadata.contentType,
      digest
    ),
    metadata
  )
  assert.deepEqual(await transport.download("book/a", "reader"), {
    metadata,
    data: new Uint8Array([98, 111, 111, 107]),
  })

  assert.equal(
    requests[0]?.url,
    "https://readio.test/api/cloud/v1/library/content/book%2Fa/reader"
  )
  for (const request of requests) {
    assert.equal(request.init.credentials, "include")
    assert.equal(
      new Headers(request.init.headers).get("authorization"),
      "Bearer test-token"
    )
  }
  const uploadHeaders = new Headers(requests[1]?.init.headers)
  assert.equal(uploadHeaders.get("x-readio-content-sha256"), digest)
  assert.deepEqual(
    new Uint8Array(await (requests[1]?.init.body as Blob).arrayBuffer()),
    new Uint8Array([98, 111, 111, 107])
  )
})
