import type {
  SyncEntityKind,
  SyncMutationResult,
  SyncPushBatch,
} from "./app"

export interface StoredEntity {
  entity_kind: SyncEntityKind
  entity_id: string
  operation: "upsert" | "delete"
  version: number
  cursor: number
  server_timestamp_unix_ms: number
  payload: unknown
}

export interface StoredMutation {
  mutation_id: string
  entity_kind: SyncEntityKind
  entity_id: string
  version: number
  cursor: number
}

export interface StorageSnapshot {
  entities: StoredEntity[]
  mutations: StoredMutation[]
}

export interface StorageWrite {
  device_id: string
  mutation_id: string
  expected_version: number
  record: StoredEntity
}

export interface StoragePlanResult {
  mutation_id: string
  status: "applied" | "replayed" | "conflict"
  version?: number
  cursor?: number
  current?: StoredEntity
}

export interface StoragePlan {
  cursor: number
  results: StoragePlanResult[]
  writes: StorageWrite[]
}

export interface StoragePlanner {
  plan(
    snapshot: StorageSnapshot,
    batch: SyncPushBatch,
    serverTimestampUnixMs: number,
    startingCursor: number,
  ): StoragePlan
}

export function publicMutationResult(result: StoragePlanResult): SyncMutationResult {
  return {
    mutationId: result.mutation_id,
    status: result.status,
    version: result.version,
    cursor: result.cursor,
    current: result.current
      ? {
          cursor: result.current.cursor,
          entityKind: result.current.entity_kind,
          entityId: result.current.entity_id,
          operation: result.current.operation,
          version: result.current.version,
          serverTimestampUnixMs: result.current.server_timestamp_unix_ms,
          payload: result.current.payload,
        }
      : undefined,
  }
}
