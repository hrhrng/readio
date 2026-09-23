"use client"

import { webReadioSdk } from "./readio-core-runtime.ts"
import {
  type CloudSyncOptions,
  type LocalReplicaStore,
  WebLocalReplicaStore,
} from "./local-replica-store.ts"
import {
  IndexedDBLibraryContentPersistence,
  IndexedDBReplicaSyncPersistence,
} from "./replica-sync.ts"
import { SqliteWorkerLocalReplicaStore } from "./sqlite-local-replica-protocol.ts"

let singleton: Promise<LocalReplicaStore> | undefined
const CLOUD_DEVICE_ID_KEY = "readio.cloud.device-id.v1"

export interface WebCloudSyncDependencies {
  baseURL?: string
  getStore?: () => Promise<Pick<LocalReplicaStore, "syncCloud">>
  randomUUID?: () => string
  storage?: Pick<Storage, "getItem" | "setItem">
}

export type WebCloudSyncResult = { status: "anonymous" | "synced" }

/**
 * Lazily opens the browser's account-free local replica. SQLite/OPFS becomes
 * the primary adapter when its Worker is available; IndexedDB is the durable
 * compatibility fallback for browsers without the required OPFS primitives.
 */
export function getBrowserLocalReplicaStore(): Promise<LocalReplicaStore> {
  singleton ??= createBrowserLocalReplicaStore()
  return singleton
}

/**
 * Opens the authenticated cloud path only after Better Auth has produced a
 * user. The stable platform identity is separate from account identity, so
 * signing out never makes anonymous local reading depend on the network.
 */
export async function syncWebCloud(
  userId: string | null | undefined,
  dependencies: WebCloudSyncDependencies = {}
): Promise<WebCloudSyncResult> {
  if (!userId) return { status: "anonymous" }

  const storage = dependencies.storage ?? globalThis.localStorage
  let deviceId = storage.getItem(CLOUD_DEVICE_ID_KEY)
  if (!validCloudDeviceId(deviceId)) {
    const randomUUID =
      dependencies.randomUUID ?? globalThis.crypto.randomUUID.bind(crypto)
    deviceId = `web-${randomUUID()}`
    storage.setItem(CLOUD_DEVICE_ID_KEY, deviceId)
  }

  const baseURL =
    dependencies.baseURL ??
    new URL("/api/cloud/", globalThis.location.origin).href
  const getStore = dependencies.getStore ?? getBrowserLocalReplicaStore
  const options: CloudSyncOptions = { baseURL, deviceId }
  await (await getStore()).syncCloud(options)
  return { status: "synced" }
}

async function createBrowserLocalReplicaStore(): Promise<LocalReplicaStore> {
  if (typeof Worker !== "undefined" && globalThis.crossOriginIsolated) {
    const sqlite = new SqliteWorkerLocalReplicaStore(
      new Worker(new URL("./sqlite-replica.worker.ts", import.meta.url), {
        type: "module",
        name: "readio-sqlite",
      })
    )
    try {
      await sqlite.state()
      return sqlite
    } catch {
      sqlite.close()
    }
  }

  return new WebLocalReplicaStore({
    sdk: webReadioSdk,
    persistence: new IndexedDBReplicaSyncPersistence(),
    contentPersistence: new IndexedDBLibraryContentPersistence(),
  })
}

function validCloudDeviceId(value: string | null): value is string {
  return value !== null && value.startsWith("web-") && value.length <= 128
}
