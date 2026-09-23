import type {
  SyncChange,
  SyncPullResult,
  SyncPushBatch,
  SyncPushResult,
  SyncService,
} from "./app"
import {
  publicMutationResult,
  type StoragePlan,
  type StoragePlanner,
  type StorageSnapshot,
  type StoredEntity,
  type StoredMutation,
  type StorageWrite,
} from "./storage-planner"

interface EntityRow {
  entity_kind: StoredEntity["entity_kind"]
  entity_id: string
  operation: StoredEntity["operation"]
  version: number
  cursor: number
  server_timestamp_unix_ms: number
  payload_json: string
}

interface MutationRow {
  mutation_id: string
  entity_kind: StoredMutation["entity_kind"]
  entity_id: string
  version: number
  cursor: number
}

interface CursorRow {
  cursor: number | null
}

const MAX_REPLAN_ATTEMPTS = 3
const KEY_SEPARATOR = "\u001f"

function parsePayload(payload: string): unknown {
  return JSON.parse(payload) as unknown
}

function entityFromRow(row: EntityRow): StoredEntity {
  return {
    entity_kind: row.entity_kind,
    entity_id: row.entity_id,
    operation: row.operation,
    version: row.version,
    cursor: row.cursor,
    server_timestamp_unix_ms: row.server_timestamp_unix_ms,
    payload: parsePayload(row.payload_json),
  }
}

function changeFromRow(row: EntityRow): SyncChange {
  return {
    cursor: row.cursor,
    entityKind: row.entity_kind,
    entityId: row.entity_id,
    operation: row.operation,
    version: row.version,
    serverTimestampUnixMs: row.server_timestamp_unix_ms,
    payload: parsePayload(row.payload_json),
  }
}

export class D1SyncService implements SyncService {
  constructor(
    private readonly database: D1Database,
    private readonly planner: StoragePlanner,
    private readonly now: () => number = Date.now,
  ) {}

  async push(userId: string, batch: SyncPushBatch): Promise<SyncPushResult> {
    for (let attempt = 0; attempt < MAX_REPLAN_ATTEMPTS; attempt += 1) {
      const { snapshot, cursor } = await this.loadSnapshot(userId, batch)
      const plan = this.planner.plan(snapshot, batch, this.now(), cursor)

      if (plan.writes.length === 0 || (await this.apply(userId, plan))) {
        return {
          cursor: plan.cursor,
          results: plan.results.map(publicMutationResult),
        }
      }
    }

    throw new Error("sync_compare_and_swap_exhausted")
  }

  async pull(userId: string, cursor: number, limit: number): Promise<SyncPullResult> {
    const response = await this.database
      .prepare(
        `SELECT entity_kind, entity_id, operation, version, cursor,
                server_timestamp_unix_ms, payload_json
           FROM sync_changes
          WHERE user_id = ? AND cursor > ?
          ORDER BY cursor ASC
          LIMIT ?`,
      )
      .bind(userId, cursor, limit + 1)
      .all<EntityRow>()
    const rows = response.results
    const hasMore = rows.length > limit
    const page = hasMore ? rows.slice(0, limit) : rows

    return {
      cursor: page.at(-1)?.cursor ?? cursor,
      hasMore,
      changes: page.map(changeFromRow),
    }
  }

  private async loadSnapshot(
    userId: string,
    batch: SyncPushBatch,
  ): Promise<{ snapshot: StorageSnapshot; cursor: number }> {
    const entityKeys = batch.mutations.map(
      (mutation) => `${mutation.entityKind}${KEY_SEPARATOR}${mutation.entityId}`,
    )
    const mutationIds = batch.mutations.map((mutation) => mutation.mutationId)
    const [entitiesResponse, mutationsResponse, cursorRow] = await Promise.all([
      this.database
        .prepare(
          `SELECT entity_kind, entity_id, operation, version, cursor,
                  server_timestamp_unix_ms, payload_json
             FROM sync_entities
            WHERE user_id = ?
              AND (entity_kind || char(31) || entity_id)
                  IN (SELECT value FROM json_each(?))`,
        )
        .bind(userId, JSON.stringify(entityKeys))
        .all<EntityRow>(),
      this.database
        .prepare(
          `SELECT mutation_id, entity_kind, entity_id, version, cursor
             FROM sync_mutations
            WHERE user_id = ?
              AND mutation_id IN (SELECT value FROM json_each(?))`,
        )
        .bind(userId, JSON.stringify(mutationIds))
        .all<MutationRow>(),
      this.database
        .prepare(
          `SELECT COALESCE(MAX(cursor), 0) AS cursor
             FROM sync_changes
            WHERE user_id = ?`,
        )
        .bind(userId)
        .first<CursorRow>(),
    ])

    return {
      snapshot: {
        entities: entitiesResponse.results.map(entityFromRow),
        mutations: mutationsResponse.results,
      },
      cursor: cursorRow?.cursor ?? 0,
    }
  }

  private async apply(userId: string, plan: StoragePlan): Promise<boolean> {
    const statements = plan.writes.flatMap((write) => this.statements(userId, write))

    try {
      const results = await this.database.batch(statements)
      return plan.writes.every((_write, index) => results[index * 3].meta.changes === 1)
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught)
      if (/unique|constraint/i.test(message)) return false
      throw caught
    }
  }

  private statements(userId: string, write: StorageWrite): D1PreparedStatement[] {
    const record = write.record
    const payload = JSON.stringify(record.payload)
    const entity =
      write.expected_version === 0
        ? this.database
            .prepare(
              `INSERT INTO sync_entities
                 (user_id, entity_kind, entity_id, operation, version, cursor,
                  server_timestamp_unix_ms, payload_json)
               SELECT ?, ?, ?, ?, ?, ?, ?, ?
                WHERE NOT EXISTS (
                  SELECT 1 FROM sync_entities
                   WHERE user_id = ? AND entity_kind = ? AND entity_id = ?
                )`,
            )
            .bind(
              userId,
              record.entity_kind,
              record.entity_id,
              record.operation,
              record.version,
              record.cursor,
              record.server_timestamp_unix_ms,
              payload,
              userId,
              record.entity_kind,
              record.entity_id,
            )
        : this.database
            .prepare(
              `UPDATE sync_entities
                  SET operation = ?, version = ?, cursor = ?,
                      server_timestamp_unix_ms = ?, payload_json = ?
                WHERE user_id = ? AND entity_kind = ? AND entity_id = ?
                  AND version = ?`,
            )
            .bind(
              record.operation,
              record.version,
              record.cursor,
              record.server_timestamp_unix_ms,
              payload,
              userId,
              record.entity_kind,
              record.entity_id,
              write.expected_version,
            )

    const change = this.database
      .prepare(
        `INSERT INTO sync_changes
           (user_id, cursor, entity_kind, entity_id, operation, version,
            server_timestamp_unix_ms, payload_json)
         SELECT user_id, cursor, entity_kind, entity_id, operation, version,
                server_timestamp_unix_ms, payload_json
           FROM sync_entities
          WHERE user_id = ? AND entity_kind = ? AND entity_id = ?
            AND version = ? AND cursor = ?`,
      )
      .bind(
        userId,
        record.entity_kind,
        record.entity_id,
        record.version,
        record.cursor,
      )

    const mutation = this.database
      .prepare(
        `INSERT INTO sync_mutations
           (user_id, mutation_id, device_id, entity_kind, entity_id,
            version, cursor, created_at_unix_ms)
         SELECT user_id, ?, ?, entity_kind, entity_id, version, cursor, ?
           FROM sync_entities
          WHERE user_id = ? AND entity_kind = ? AND entity_id = ?
            AND version = ? AND cursor = ?`,
      )
      .bind(
        write.mutation_id,
        write.device_id,
        record.server_timestamp_unix_ms,
        userId,
        record.entity_kind,
        record.entity_id,
        record.version,
        record.cursor,
      )

    return [entity, change, mutation]
  }
}
