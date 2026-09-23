import { env } from "cloudflare:test"
import { beforeEach, describe, expect, it } from "vitest"

import { D1ReplicaService } from "../src/d1-replica-service"

async function insertUser(id: string) {
  await env.DB.prepare(
    `INSERT INTO user (id, name, email, email_verified, created_at, updated_at)
     VALUES (?, ?, ?, 1, 0, 0)`,
  )
    .bind(id, id, `${id}@example.com`)
    .run()
}

describe("D1 Loro replica adapter", () => {
  beforeEach(async () => {
    await env.DB.exec(
      "DELETE FROM replica_devices; DELETE FROM replica_changes; DELETE FROM replica_snapshots; DELETE FROM user;",
    )
    await insertUser("reader-a")
    await insertUser("reader-b")
  })

  it("deduplicates replayed changes, persists device heads, and isolates pull streams", async () => {
    const service = new D1ReplicaService(env.DB, env.MEDIA, () => 1_000)
    const changeId = "a".repeat(64)
    const batch = {
      deviceId: "mac-a",
      ackCursor: 0,
      headsBase64: "BAUG",
      changes: [{ changeId, changeBase64: "AQID" }],
    }

    const first = await service.push("reader-a", batch)
    const replay = await service.push("reader-a", batch)

    expect(first).toEqual({ cursor: 1, acceptedChangeIds: [changeId] })
    expect(replay).toEqual(first)
    expect(
      await env.DB.prepare(
        "SELECT COUNT(*) AS count FROM replica_changes WHERE user_id = ?",
      )
        .bind("reader-a")
        .first<number>("count"),
    ).toBe(1)
    expect(
      await env.DB.prepare(
        "SELECT ack_cursor, hex(heads_blob) AS heads FROM replica_devices WHERE user_id = ? AND device_id = ?",
      )
        .bind("reader-a", "mac-a")
        .first(),
    ).toMatchObject({ ack_cursor: 0, heads: "040506" })

    await expect(service.pull("reader-a", 0, 100)).resolves.toEqual({
      cursor: 1,
      hasMore: false,
      changes: [
        {
          cursor: 1,
          changeId,
          deviceId: "mac-a",
          changeBase64: "AQID",
        },
      ],
    })
    await expect(service.pull("reader-b", 0, 100)).resolves.toEqual({
      cursor: 0,
      hasMore: false,
      changes: [],
    })
  })

  it("stores bounded snapshots privately in R2 only at a persisted change cursor", async () => {
    const service = new D1ReplicaService(env.DB, env.MEDIA, () => 2_000)
    const changeId = "b".repeat(64)
    const pushed = await service.push("reader-a", {
      deviceId: "phone-a",
      ackCursor: 0,
      headsBase64: "",
      changes: [{ changeId, changeBase64: "AQID" }],
    })
    const bytes = new Uint8Array([9, 8, 7, 6])
    const body = new Response(bytes).body
    expect(body).not.toBeNull()

    const stored = await service.putSnapshot("reader-a", {
      deviceId: "phone-a",
      cursor: pushed.cursor,
      byteLength: bytes.byteLength,
      body: body!,
    })
    expect(stored).toMatchObject({
      cursor: pushed.cursor,
      byteLength: bytes.byteLength,
    })
    expect(stored?.etag).toBeTruthy()
    await expect(
      service.putSnapshot("reader-a", {
        deviceId: "phone-a",
        cursor: pushed.cursor + 1,
        byteLength: bytes.byteLength,
        body: new Response(bytes).body!,
      }),
    ).resolves.toBeNull()

    const downloaded = await service.getSnapshot("reader-a")
    expect(downloaded).toMatchObject(stored!)
    expect([
      ...new Uint8Array(await new Response(downloaded!.body).arrayBuffer()),
    ]).toEqual([...bytes])
    await expect(service.getSnapshot("reader-b")).resolves.toBeNull()

    const metadata = await env.DB.prepare(
      "SELECT object_key FROM replica_snapshots WHERE user_id = ?",
    )
      .bind("reader-a")
      .first<{ object_key: string }>()
    expect(metadata?.object_key).not.toContain("reader-a")
  })
})
