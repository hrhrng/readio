import { env } from "cloudflare:workers"
import { beforeEach, describe, expect, it } from "vitest"

import type { StoragePlan, StoragePlanner } from "../src/storage-planner"
import { D1SyncService } from "../src/d1-sync-service"

const userId = "reader-a"

const planner: StoragePlanner = {
  plan(_snapshot, batch, serverTimestampUnixMs, startingCursor): StoragePlan {
    const mutation = batch.mutations[0]
    const cursor = startingCursor + 1
    return {
      cursor,
      results: [
        {
          mutation_id: mutation.mutationId,
          status: "applied",
          version: 1,
          cursor,
        },
      ],
      writes: [
        {
          device_id: batch.deviceId,
          mutation_id: mutation.mutationId,
          expected_version: 0,
          record: {
            entity_kind: mutation.entityKind,
            entity_id: mutation.entityId,
            operation: mutation.operation,
            version: 1,
            cursor,
            server_timestamp_unix_ms: serverTimestampUnixMs,
            payload: mutation.payload,
          },
        },
      ],
    }
  },
}

async function insertUser(id: string) {
  await env.DB.prepare(
    `INSERT OR IGNORE INTO user
       (id, name, email, email_verified, created_at, updated_at)
     VALUES (?, ?, ?, 1, ?, ?)`,
  )
    .bind(id, id, `${id}@example.com`, 1, 1)
    .run()
}

describe("D1 sync adapter", () => {
  beforeEach(async () => {
    await env.DB.exec("DELETE FROM sync_mutations; DELETE FROM sync_changes; DELETE FROM sync_entities; DELETE FROM user;")
    await insertUser(userId)
  })

  it("persists one Rust-planned change and returns it only in that user's pull stream", async () => {
    const service = new D1SyncService(env.DB, planner, () => 1_000)
    const result = await service.push(userId, {
      deviceId: "mac-a",
      mutations: [
        {
          mutationId: "m-1",
          entityKind: "reading_position",
          entityId: "book-1",
          operation: "upsert",
          baseVersion: 0,
          clientTimestampUnixMs: 900,
          payload: { charOffset: 42 },
        },
      ],
    })

    expect(result.cursor).toBe(1)
    expect(result.results[0]).toMatchObject({ status: "applied", version: 1 })
    await expect(service.pull(userId, 0, 100)).resolves.toMatchObject({
      cursor: 1,
      hasMore: false,
      changes: [
        {
          entityKind: "reading_position",
          entityId: "book-1",
          version: 1,
          payload: { charOffset: 42 },
        },
      ],
    })

    await insertUser("reader-b")
    await expect(service.pull("reader-b", 0, 100)).resolves.toMatchObject({
      cursor: 0,
      changes: [],
    })
  })
})
