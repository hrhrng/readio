import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

import {
  deleteBookmark,
  fetchBookmarks,
  saveBookmark,
} from "../lib/bookmark-api.ts"
import {
  WebLocalReplicaStore,
  type LocalReplicaPersistence,
  type LocalReplicaRecord,
} from "../lib/local-replica-store.ts"
import {
  createReadioSdkClient,
  type ReadioSdkWasmModule,
} from "../lib/readio-sdk.ts"
import type { Bookmark } from "../lib/rust-core.ts"
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

async function testStore(persistence = new MemoryPersistence()) {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  await init({ module_or_path: binary })
  return {
    persistence,
    store: new WebLocalReplicaStore({
      sdk: createReadioSdkClient(
        async () => generatedWasm as unknown as ReadioSdkWasmModule
      ),
      persistence,
    }),
  }
}

const bookmark = (
  id: string,
  itemId: string,
  charOffset: number
): Bookmark => ({
  id,
  item_id: itemId,
  char_offset: charOffset,
  affinity: "forward",
  note: id,
  created_at_unix_ms: 1_700_000_000_000,
  updated_at_unix_ms: 1_700_000_000_000,
  version: 1,
})

test("web bookmarks persist locally and use Rust reader ordering without HTTP", async () => {
  const { persistence, store } = await testStore()
  const originalFetch = globalThis.fetch
  globalThis.fetch = async () => {
    throw new Error("local bookmark operations must not use HTTP")
  }

  try {
    await saveBookmark(bookmark("later", "book-1", 42), store)
    await saveBookmark(bookmark("other", "book-2", 1), store)
    await saveBookmark(bookmark("earlier", "book-1", 4), store)

    const restored = (await testStore(persistence)).store
    assert.deepEqual(
      (await fetchBookmarks("book-1", restored)).map(({ id }) => id),
      ["earlier", "later"]
    )
    assert.equal(JSON.parse(persistence.record!.stateJSON).outbox.length, 3)
  } finally {
    globalThis.fetch = originalFetch
  }
})

test("web bookmark deletion becomes a durable Rust CRDT deletion", async () => {
  const { persistence, store } = await testStore()
  await saveBookmark(bookmark("bookmark-1", "book-1", 4), store)

  await deleteBookmark("bookmark-1", store)

  assert.deepEqual(await fetchBookmarks("book-1", store), [])
  assert.equal(JSON.parse(persistence.record!.stateJSON).outbox.length, 2)
})
