import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

import { WebLocalReplicaStore } from "../lib/local-replica-store.ts"
import { ReaderProgressCommitter } from "../lib/reader-progress.ts"
import { webRustCore } from "../lib/readio-core-runtime.ts"
import {
  createReadioSdkClient,
  type ReadioSdkWasmModule,
} from "../lib/readio-sdk.ts"
import type {
  LocalReplicaPersistence,
  LocalReplicaRecord,
} from "../lib/local-replica-store.ts"
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

async function localStore(): Promise<WebLocalReplicaStore> {
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

test("reader progress commits the Rust Unicode-scalar start of a sentence", async () => {
  const store = await localStore()
  const document = (
    await webRustCore.parsePlainTextReader("A😀\n\nBeta")
  ).document
  const committer = new ReaderProgressCommitter({
    itemId: "book-1",
    document,
    core: webRustCore,
    store,
    sessionId: "session-a",
    now: () => 1_700_000_000_000,
    mutationId: () => "mutation-a",
  })

  await committer.commitSentence(1)

  const position = await store.readingPosition("book-1")
  assert.ok(position)
  const { locator, ...record } = position
  assert.deepEqual(record, {
    document_revision: document.revision?.revision_digest,
    char_offset: 3,
    affinity: "forward",
    observed_at_unix_ms: 1_700_000_000_000,
    session_id: "session-a",
    mutation_id: "mutation-a",
    base_version: 0,
    version: 1,
  })
  assert.equal(locator?.version, 1)
  assert.equal(locator?.block_id, document.blocks[1]?.id)
  assert.equal(locator?.offset_in_block, 0)
  assert.equal(locator?.quote.exact, "Beta")
})

test("concurrent reader effects serialize their base versions and restore the final sentence", async () => {
  const store = await localStore()
  const document = (
    await webRustCore.parsePlainTextReader("Alpha\n\nBeta")
  ).document
  let mutation = 0
  const committer = new ReaderProgressCommitter({
    itemId: "book-1",
    document,
    core: webRustCore,
    store,
    sessionId: "session-a",
    now: () => 1_700_000_000_000 + mutation,
    mutationId: () => `mutation-${++mutation}`,
  })

  await Promise.all([
    committer.commitSentence(0),
    committer.commitSentence(1),
  ])

  assert.equal((await store.readingPosition("book-1"))?.version, 2)
  assert.equal((await store.readingPosition("book-1"))?.char_offset, 6)
  assert.equal(await committer.restoreSentence(), 1)
})
