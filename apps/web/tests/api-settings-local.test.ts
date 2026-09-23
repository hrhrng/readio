import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

import { fetchSettings, patchSettings } from "../lib/api.ts"
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

class MemoryPersistence implements LocalReplicaPersistence {
  record: LocalReplicaRecord | null = null
  saveCount = 0

  async load(): Promise<LocalReplicaRecord | null> {
    return this.record === null ? null : structuredClone(this.record)
  }

  async save(record: LocalReplicaRecord): Promise<void> {
    this.saveCount += 1
    this.record = structuredClone(record)
  }
}

test("web settings use the local Rust replica and null deletes an override", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  await init({ module_or_path: binary })
  const persistence = new MemoryPersistence()
  const store = new WebLocalReplicaStore({
    sdk: createReadioSdkClient(
      async () => generatedWasm as unknown as ReadioSdkWasmModule
    ),
    persistence,
  })
  const originalFetch = globalThis.fetch
  globalThis.fetch = async () => {
    throw new Error("local settings must not use HTTP")
  }

  try {
    await patchSettings({ theme: "dark", font_size: "20" }, store)
    assert.deepEqual(await fetchSettings(store), {
      font_size: "20",
      theme: "dark",
    })
    assert.equal(persistence.saveCount, 1)

    await patchSettings({ theme: null }, store)
    assert.deepEqual(await fetchSettings(store), { font_size: "20" })
    assert.equal(persistence.saveCount, 2)
    assert.equal(JSON.parse(persistence.record!.stateJSON).outbox.length, 3)
  } finally {
    globalThis.fetch = originalFetch
  }
})
