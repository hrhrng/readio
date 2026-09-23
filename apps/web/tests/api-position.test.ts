import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

import {
  ReadingPositionConflictError,
  fetchReadingPosition,
  saveReadingPosition,
} from "../lib/reading-position-api.ts"
import {
  WebLocalReplicaStore,
  type LocalReplicaPersistence,
  type LocalReplicaRecord,
} from "../lib/local-replica-store.ts"
import {
  createReadioSdkClient,
  type ReadioSdkWasmModule,
} from "../lib/readio-sdk.ts"
import init, * as generatedWasm from "../wasm/readio_wasm.js"

const position = {
  document_revision: "revision-1",
  char_offset: 4,
  affinity: "forward" as const,
  observed_at_unix_ms: 1_700_000_000_000,
  session_id: "session-a",
  mutation_id: "mutation-a",
  base_version: 0,
}

class MemoryPersistence implements LocalReplicaPersistence {
  record: LocalReplicaRecord | null = null

  async load(): Promise<LocalReplicaRecord | null> {
    return this.record === null ? null : structuredClone(this.record)
  }

  async save(record: LocalReplicaRecord): Promise<void> {
    this.record = structuredClone(record)
  }
}

async function localStore() {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  await init({ module_or_path: binary })
  return new WebLocalReplicaStore({
    sdk: createReadioSdkClient(
      async () => generatedWasm as unknown as ReadioSdkWasmModule
    ),
    persistence: new MemoryPersistence(),
  })
}

test("web reading position mutation is versioned, durable, and idempotent locally", async () => {
  const store = await localStore()
  const originalFetch = globalThis.fetch
  globalThis.fetch = async () => {
    throw new Error("reading positions must not use HTTP")
  }

  try {
    assert.deepEqual(await saveReadingPosition("book-1", position, store), {
      status: "applied",
      idempotent_replay: false,
      version: 1,
      position: { ...position, version: 1 },
      conflict_id: null,
      candidates: [],
    })
    assert.deepEqual(await fetchReadingPosition("book-1", store), {
      ...position,
      version: 1,
    })
    assert.equal(
      (await saveReadingPosition("book-1", position, store)).idempotent_replay,
      true
    )
  } finally {
    globalThis.fetch = originalFetch
  }
})

test("stale local reading position writes preserve explicit conflict resolution", async () => {
  const store = await localStore()
  await saveReadingPosition("book-1", position, store)
  const incoming = {
    ...position,
    char_offset: 1,
    mutation_id: "mutation-b",
  }

  await assert.rejects(
    saveReadingPosition("book-1", incoming, store),
    (error: unknown) => {
      assert.ok(error instanceof ReadingPositionConflictError)
      assert.equal(error.status, 409)
      assert.equal(error.conflict.status, "conflict")
      assert.deepEqual(error.conflict.candidates, [
        { origin: "current", ...position, version: 1 },
        { origin: "incoming", ...incoming, version: 0 },
      ])
      return true
    }
  )
})

test("missing local reading positions are reported without an HTTP fallback", async () => {
  const store = await localStore()
  await assert.rejects(
    () => fetchReadingPosition("missing", store),
    /not found/i
  )
})
