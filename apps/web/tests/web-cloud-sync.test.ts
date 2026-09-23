import assert from "node:assert/strict"
import test from "node:test"

import * as browserReplica from "../lib/browser-local-replica.ts"
import type { CloudSyncOptions } from "../lib/local-replica-store.ts"

type SyncWebCloud = (
  userId: string | null | undefined,
  dependencies?: {
    baseURL?: string
    getStore?: () => Promise<{
      syncCloud(options: CloudSyncOptions): Promise<unknown>
    }>
    randomUUID?: () => string
    storage?: Pick<Storage, "getItem" | "setItem">
  }
) => Promise<{ status: "anonymous" | "synced" }>

class MemoryStorage implements Pick<Storage, "getItem" | "setItem"> {
  readonly values = new Map<string, string>()

  getItem(key: string): string | null {
    return this.values.get(key) ?? null
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value)
  }
}

test("anonymous local readers never initialize or contact cloud sync", async () => {
  const syncWebCloud = Reflect.get(browserReplica, "syncWebCloud") as
    | SyncWebCloud
    | undefined
  assert.equal(typeof syncWebCloud, "function")
  let storeInitializations = 0
  const storage = new MemoryStorage()

  const result = await syncWebCloud!(null, {
    baseURL: "https://readio.test/api/cloud/",
    storage,
    randomUUID: () => "should-not-run",
    async getStore() {
      storeInitializations += 1
      throw new Error("anonymous sync must not open the cloud path")
    },
  })

  assert.deepEqual(result, { status: "anonymous" })
  assert.equal(storeInitializations, 0)
  assert.equal(storage.values.size, 0)
})

test("authenticated sync reuses one stable browser device identity", async () => {
  const syncWebCloud = Reflect.get(browserReplica, "syncWebCloud") as
    | SyncWebCloud
    | undefined
  assert.equal(typeof syncWebCloud, "function")
  const storage = new MemoryStorage()
  const calls: CloudSyncOptions[] = []
  let generatedIds = 0
  const dependencies = {
    baseURL: "https://readio.test/api/cloud/",
    storage,
    randomUUID() {
      generatedIds += 1
      return "0f50dc71-9075-49f8-bc04-cd6c49a262f2"
    },
    async getStore() {
      return {
        async syncCloud(options: CloudSyncOptions) {
          calls.push(options)
          return {}
        },
      }
    },
  }

  assert.deepEqual(await syncWebCloud!("user-a", dependencies), {
    status: "synced",
  })
  assert.deepEqual(await syncWebCloud!("user-a", dependencies), {
    status: "synced",
  })
  assert.deepEqual(calls, [
    {
      baseURL: "https://readio.test/api/cloud/",
      deviceId: "web-0f50dc71-9075-49f8-bc04-cd6c49a262f2",
    },
    {
      baseURL: "https://readio.test/api/cloud/",
      deviceId: "web-0f50dc71-9075-49f8-bc04-cd6c49a262f2",
    },
  ])
  assert.equal(generatedIds, 1)
})
