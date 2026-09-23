import sqlite3InitModule from "@sqlite.org/sqlite-wasm"

import { createReadioSdkClient } from "./readio-sdk.ts"
import { createReadioWasmLoader } from "./readio-core-runtime.ts"
import { WebLocalReplicaStore } from "./local-replica-store.ts"
import {
  handleLocalReplicaRequest,
  type LocalReplicaRequest,
  type LocalReplicaResponse,
} from "./sqlite-local-replica-protocol.ts"
import {
  SqliteLibraryContentPersistence,
  SqliteReplicaSyncPersistence,
} from "./sqlite-replica-persistence.ts"

const workerScope = globalThis as unknown as {
  onmessage: ((event: MessageEvent<LocalReplicaRequest>) => void) | null
  postMessage(response: LocalReplicaResponse): void
}

const store = initializeStore()

workerScope.onmessage = (event) => {
  void respond(event.data)
}

async function initializeStore(): Promise<WebLocalReplicaStore> {
  const sqlite3 = await sqlite3InitModule()
  const OpfsDatabase = sqlite3.oo1.OpfsDb
  if (typeof OpfsDatabase !== "function") {
    throw new Error("SQLite OPFS is unavailable")
  }

  const database = new OpfsDatabase("/readio/readio.sqlite3", "c")
  return new WebLocalReplicaStore({
    sdk: createReadioSdkClient(createReadioWasmLoader()),
    persistence: new SqliteReplicaSyncPersistence(database),
    contentPersistence: new SqliteLibraryContentPersistence(database),
  })
}

async function respond(request: LocalReplicaRequest): Promise<void> {
  try {
    workerScope.postMessage(
      await handleLocalReplicaRequest(await store, request)
    )
  } catch (error) {
    workerScope.postMessage({
      id: request.id,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    })
  }
}
