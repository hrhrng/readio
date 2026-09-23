import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"
import { MessageChannel } from "node:worker_threads"

import sqlite3InitModule from "@sqlite.org/sqlite-wasm"

import { WebLocalReplicaStore } from "../lib/local-replica-store.ts"
import {
  handleLocalReplicaRequest,
  SqliteWorkerLocalReplicaStore,
  type LocalReplicaMessageEndpoint,
} from "../lib/sqlite-local-replica-protocol.ts"
import {
  SqliteLibraryContentPersistence,
  SqliteReplicaSyncPersistence,
} from "../lib/sqlite-replica-persistence.ts"
import {
  createReadioSdkClient,
  type ReadioSdkWasmModule,
} from "../lib/readio-sdk.ts"
import type {
  LibraryContentCloudMetadata,
  LibraryContentCloudTransport,
  ReplicaPullPage,
  ReplicaPushRequest,
  ReplicaPushResult,
  ReplicaTransport,
} from "../lib/replica-sync.ts"
import init, * as generatedWasm from "../wasm/readio_wasm.js"

class ProtocolReplicaTransport implements ReplicaTransport {
  snapshotDeviceId: string | null = null

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
    deviceId: string,
    _cursor: number
  ): Promise<void> {
    this.snapshotDeviceId = deviceId
  }
}

class ProtocolContentTransport implements LibraryContentCloudTransport {
  async head() { return null }
  async upload(
    itemId: string,
    variant: "reader" | "source",
    data: Uint8Array,
    contentType: string,
    sha256Base64: string
  ): Promise<LibraryContentCloudMetadata> {
    return {
      itemId,
      variant,
      byteLength: data.byteLength,
      contentType,
      sha256Base64,
      etag: "protocol-fixture",
    }
  }
  async download() { return null }
}

test("the SQLite Worker protocol executes local mutations through real Rust WASM", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  await init({ module_or_path: binary })
  const sqlite3 = await sqlite3InitModule()
  const database = new sqlite3.oo1.DB(":memory:", "c")
  const store = new WebLocalReplicaStore({
    sdk: createReadioSdkClient(
      async () => generatedWasm as unknown as ReadioSdkWasmModule
    ),
    persistence: new SqliteReplicaSyncPersistence(database),
    contentPersistence: new SqliteLibraryContentPersistence(database),
  })

  try {
    await handleLocalReplicaRequest(store, {
      id: 1,
      type: "upsert",
      entities: [
        {
          entityKind: "bookmark",
          entityId: "bookmark-1",
          payload: {
            id: "bookmark-1",
            item_id: "book-1",
            char_offset: 8,
            affinity: "forward",
            note: "Local",
            created_at_unix_ms: 10,
            updated_at_unix_ms: 10,
            version: 1,
          },
        },
      ],
    })

    assert.deepEqual(
      await handleLocalReplicaRequest(store, {
        id: 2,
        type: "bookmarks",
        itemId: "book-1",
      }),
      {
        id: 2,
        ok: true,
        value: [
          {
            id: "bookmark-1",
            item_id: "book-1",
            char_offset: 8,
            affinity: "forward",
            note: "Local",
            created_at_unix_ms: 10,
            updated_at_unix_ms: 10,
            version: 1,
          },
        ],
      }
    )

    const channel = new MessageChannel()
    channel.port2.on("message", async (request) => {
      channel.port2.postMessage(
        await handleLocalReplicaRequest(store, request)
      )
    })
    const endpoint: LocalReplicaMessageEndpoint = {
      postMessage(message) {
        channel.port1.postMessage(message)
      },
      onmessage: null,
      close() {
        channel.port1.close()
      },
    }
    channel.port1.on("message", (response) => {
      endpoint.onmessage?.({ data: response } as MessageEvent)
    })
    const client = new SqliteWorkerLocalReplicaStore(endpoint)
    try {
      await client.delete([
        { entityKind: "bookmark", entityId: "bookmark-1" },
      ])
      assert.deepEqual(await client.bookmarks("book-1"), [])
    } finally {
      client.close()
      channel.port2.close()
    }
  } finally {
    database.close()
  }
})

test("the SQLite Worker exposes Rust library projections through RPC", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  await init({ module_or_path: binary })
  const sqlite3 = await sqlite3InitModule()
  const database = new sqlite3.oo1.DB(":memory:", "c")
  const replicaTransport = new ProtocolReplicaTransport()
  const store = new WebLocalReplicaStore({
    sdk: createReadioSdkClient(
      async () => generatedWasm as unknown as ReadioSdkWasmModule
    ),
    persistence: new SqliteReplicaSyncPersistence(database),
    contentPersistence: new SqliteLibraryContentPersistence(database),
    cloudTransportFactory: () => ({
      replica: replicaTransport,
      content: new ProtocolContentTransport(),
    }),
  })
  const channel = new MessageChannel()
  channel.port2.on("message", async (request) => {
    channel.port2.postMessage(await handleLocalReplicaRequest(store, request))
  })
  const endpoint: LocalReplicaMessageEndpoint = {
    postMessage(message) {
      channel.port1.postMessage(message)
    },
    onmessage: null,
    close() {
      channel.port1.close()
    },
  }
  channel.port1.on("message", (response) => {
    endpoint.onmessage?.({ data: response } as MessageEvent)
  })
  const client = new SqliteWorkerLocalReplicaStore(endpoint)

  try {
    await client.upsert([
      {
        entityKind: "library_item",
        entityId: "book-1",
        payload: {
          id: "book-1",
          title: "Local book",
          type: "txt",
          progress: 12,
          date: "2026-01-01",
          category: "imported",
          folder_id: "inbox",
          source: "upload:book-1",
          file_path: "local:book-1",
          cover_image: null,
          voice: null,
          speed: null,
          current_chapter: null,
          content_format: null,
        },
      },
    ])

    assert.equal(
      (
        await client.queryLibrary({
          page: 1,
          page_size: 20,
          sort_by: "created_at",
          sort_order: "descending",
        })
      ).items[0]?.title,
      "Local book"
    )
    assert.equal((await client.libraryItem("book-1"))?.progress, 12)
    await client.saveReadingPosition("book-1", {
      document_revision: "revision-1",
      char_offset: 72,
      affinity: "forward",
      observed_at_unix_ms: 1_700_000_000_000,
      session_id: "session-a",
      mutation_id: "mutation-a",
      base_version: 0,
    })
    assert.equal((await client.readingPosition("book-1"))?.char_offset, 72)

    await client.saveLibraryContent({
      itemId: "book-1",
      content: "Local text",
      fileBytes: new Uint8Array([11, 22, 33]),
      mediaType: "text/plain",
      fileName: "local.txt",
    })
    assert.deepEqual(await client.libraryContent("book-1"), {
      itemId: "book-1",
      content: "Local text",
      fileBytes: new Uint8Array([11, 22, 33]),
      mediaType: "text/plain",
      fileName: "local.txt",
    })
    await client.deleteLibraryContent("book-1")
    assert.equal(await client.libraryContent("book-1"), null)

    const synced = await client.syncCloud({
      baseURL: "https://readio.test/api/cloud/",
      deviceId: "sqlite-worker-device",
    })
    assert.deepEqual(synced.library_items?.["book-1"], {
      id: "book-1",
      title: "Local book",
      type: "txt",
      progress: 12,
      date: "2026-01-01",
      category: "imported",
      folder_id: "inbox",
      source: "upload:book-1",
      file_path: "local:book-1",
      cover_image: null,
      voice: null,
      speed: null,
      current_chapter: null,
      content_format: null,
    })
    assert.equal(replicaTransport.snapshotDeviceId, "sqlite-worker-device")
  } finally {
    client.close()
    channel.port2.close()
    database.close()
  }
})
