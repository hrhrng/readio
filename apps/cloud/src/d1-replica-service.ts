import { Buffer } from "node:buffer"

import type {
  ReplicaPullResult,
  ReplicaPushBatch,
  ReplicaPushResult,
  ReplicaService,
  ReplicaSnapshotDownload,
  ReplicaSnapshotInput,
  ReplicaSnapshotRecord,
  ReplicaStoredChange,
} from "./app"

interface ChangeRow {
  cursor: number
  change_id: string
  device_id: string
  change_blob: ArrayBuffer
}

interface CursorRow {
  cursor: number | null
}

interface ChangeIdRow {
  change_id: string
}

interface SnapshotRow {
  cursor: number
  object_key: string
  etag: string
  byte_length: number
}

function decodeBase64(value: string): Uint8Array {
  return new Uint8Array(Buffer.from(value, "base64"))
}

function encodeBase64(value: ArrayBuffer): string {
  return Buffer.from(value).toString("base64")
}

function storedChange(row: ChangeRow): ReplicaStoredChange {
  return {
    cursor: row.cursor,
    changeId: row.change_id,
    deviceId: row.device_id,
    changeBase64: encodeBase64(row.change_blob),
  }
}

async function snapshotObjectKey(userId: string, cursor: number): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(userId))
  const namespace = [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("")
  return `replica-snapshots/${namespace}/${cursor}.loro`
}

export class D1ReplicaService implements ReplicaService {
  constructor(
    private readonly database: D1Database,
    private readonly media: R2Bucket,
    private readonly now: () => number = Date.now,
  ) {}

  async push(userId: string, batch: ReplicaPushBatch): Promise<ReplicaPushResult> {
    const session = this.database.withSession("first-primary")
    const timestamp = this.now()
    const statements = batch.changes.map((change) => {
      const bytes = decodeBase64(change.changeBase64)
      return session
        .prepare(
          `INSERT INTO replica_changes
             (user_id, change_id, device_id, change_blob, byte_length, created_at_unix_ms)
           VALUES (?, ?, ?, ?, ?, ?)
           ON CONFLICT(user_id, change_id) DO NOTHING`,
        )
        .bind(userId, change.changeId, batch.deviceId, bytes, bytes.byteLength, timestamp)
    })
    statements.push(
      session
        .prepare(
          `INSERT INTO replica_devices
             (user_id, device_id, ack_cursor, heads_blob, updated_at_unix_ms)
           VALUES (?, ?, ?, ?, ?)
           ON CONFLICT(user_id, device_id) DO UPDATE SET
             ack_cursor = MAX(replica_devices.ack_cursor, excluded.ack_cursor),
             heads_blob = excluded.heads_blob,
             updated_at_unix_ms = excluded.updated_at_unix_ms`,
        )
        .bind(
          userId,
          batch.deviceId,
          batch.ackCursor,
          decodeBase64(batch.headsBase64),
          timestamp,
        ),
    )
    await session.batch(statements)

    const requestedIds = batch.changes.map((change) => change.changeId)
    let acceptedChangeIds: string[] = []
    if (requestedIds.length > 0) {
      const result = await session
        .prepare(
          `SELECT change_id
             FROM replica_changes
            WHERE user_id = ?
              AND change_id IN (SELECT value FROM json_each(?))`,
        )
        .bind(userId, JSON.stringify(requestedIds))
        .all<ChangeIdRow>()
      const accepted = new Set(result.results.map((row) => row.change_id))
      acceptedChangeIds = requestedIds.filter((changeId) => accepted.has(changeId))
    }

    const cursor = await session
      .prepare(
        "SELECT MAX(cursor) AS cursor FROM replica_changes WHERE user_id = ?",
      )
      .bind(userId)
      .first<CursorRow>()

    return {
      cursor: cursor?.cursor ?? 0,
      acceptedChangeIds,
    }
  }

  async pull(userId: string, cursor: number, limit: number): Promise<ReplicaPullResult> {
    const session = this.database.withSession("first-primary")
    const result = await session
      .prepare(
        `SELECT cursor, change_id, device_id, change_blob
           FROM replica_changes
          WHERE user_id = ? AND cursor > ?
          ORDER BY cursor ASC
          LIMIT ?`,
      )
      .bind(userId, cursor, limit + 1)
      .all<ChangeRow>()
    const hasMore = result.results.length > limit
    const page = result.results.slice(0, limit)

    return {
      cursor: page.at(-1)?.cursor ?? cursor,
      hasMore,
      changes: page.map(storedChange),
    }
  }

  async putSnapshot(
    userId: string,
    input: ReplicaSnapshotInput,
  ): Promise<ReplicaSnapshotRecord | null> {
    const session = this.database.withSession("first-primary")
    const latest = await session
      .prepare(
        "SELECT MAX(cursor) AS cursor FROM replica_changes WHERE user_id = ?",
      )
      .bind(userId)
      .first<CursorRow>()
    if (input.cursor > (latest?.cursor ?? 0)) return null

    const objectKey = await snapshotObjectKey(userId, input.cursor)
    const object = await this.media.put(objectKey, input.body, {
      httpMetadata: { contentType: "application/octet-stream" },
      customMetadata: { cursor: String(input.cursor) },
    })
    if (object.size !== input.byteLength) {
      await this.media.delete(objectKey)
      throw new Error("replica_snapshot_length_mismatch")
    }

    await session
      .prepare(
        `INSERT INTO replica_snapshots
           (user_id, cursor, object_key, etag, byte_length,
            created_by_device_id, created_at_unix_ms)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id, cursor) DO UPDATE SET
           object_key = excluded.object_key,
           etag = excluded.etag,
           byte_length = excluded.byte_length,
           created_by_device_id = excluded.created_by_device_id,
           created_at_unix_ms = excluded.created_at_unix_ms`,
      )
      .bind(
        userId,
        input.cursor,
        objectKey,
        object.etag,
        object.size,
        input.deviceId,
        this.now(),
      )
      .run()

    return {
      cursor: input.cursor,
      etag: object.httpEtag,
      byteLength: object.size,
    }
  }

  async getSnapshot(userId: string): Promise<ReplicaSnapshotDownload | null> {
    const row = await this.database
      .prepare(
        `SELECT cursor, object_key, etag, byte_length
           FROM replica_snapshots
          WHERE user_id = ?
          ORDER BY cursor DESC
          LIMIT 1`,
      )
      .bind(userId)
      .first<SnapshotRow>()
    if (!row) return null

    const object = await this.media.get(row.object_key)
    if (!object) return null
    return {
      cursor: row.cursor,
      etag: object.httpEtag,
      byteLength: row.byte_length,
      body: object.body,
    }
  }
}
