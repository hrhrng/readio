import coreModule from "../wasm/readio_wasm_bg.wasm"
import {
  initSync,
  plan_sync_push,
} from "../wasm/readio_wasm.js"

import type { SyncPushBatch } from "./app"
import type {
  StoragePlan,
  StoragePlanner,
  StorageSnapshot,
} from "./storage-planner"

let initialized = false

function ensureInitialized() {
  if (initialized) return
  initSync({ module: coreModule })
  initialized = true
}

function rustBatch(batch: SyncPushBatch) {
  return {
    device_id: batch.deviceId,
    mutations: batch.mutations.map((mutation) => ({
      mutation_id: mutation.mutationId,
      entity_kind: mutation.entityKind,
      entity_id: mutation.entityId,
      operation: mutation.operation,
      base_version: mutation.baseVersion,
      client_timestamp_unix_ms: mutation.clientTimestampUnixMs,
      payload: mutation.payload,
    })),
  }
}

export class RustStoragePlanner implements StoragePlanner {
  plan(
    snapshot: StorageSnapshot,
    batch: SyncPushBatch,
    serverTimestampUnixMs: number,
    startingCursor: number,
  ): StoragePlan {
    ensureInitialized()
    const encoded = plan_sync_push(
      JSON.stringify(snapshot),
      JSON.stringify(rustBatch(batch)),
      BigInt(serverTimestampUnixMs),
      BigInt(startingCursor),
    )
    return JSON.parse(encoded) as StoragePlan
  }
}
