import type { TextLocator } from "./rust-core.ts"

export type PositionAffinity = "forward" | "backward"

export interface ReadingPositionMutation {
  document_revision: string
  char_offset: number
  affinity: PositionAffinity
  observed_at_unix_ms: number
  session_id: string
  mutation_id: string
  base_version: number
  locator?: TextLocator | null
}

export interface ReadingPositionRecord extends ReadingPositionMutation {
  version: number
}

export interface PositionCandidate extends ReadingPositionRecord {
  origin: "current" | "incoming"
}

export interface PositionMutationResponse {
  status: "applied" | "conflict"
  idempotent_replay: boolean
  version: number
  position: ReadingPositionRecord | null
  conflict_id: string | null
  candidates: PositionCandidate[]
}

export class ReadingPositionConflictError extends Error {
  readonly status = 409
  readonly conflict: PositionMutationResponse

  constructor(conflict: PositionMutationResponse) {
    super("reading position conflict")
    this.name = "ReadingPositionConflictError"
    this.conflict = conflict
  }
}

export async function saveReadingPosition(
  itemId: string,
  mutation: ReadingPositionMutation,
  store?: LocalReplicaStore
): Promise<PositionMutationResponse> {
  const local = store ?? (await getBrowserLocalReplicaStore())
  const response = await local.saveReadingPosition(itemId, mutation)
  if (response.status === "conflict") {
    throw new ReadingPositionConflictError(response)
  }
  return response
}

export async function fetchReadingPosition(
  itemId: string,
  store?: LocalReplicaStore
): Promise<ReadingPositionRecord> {
  const local = store ?? (await getBrowserLocalReplicaStore())
  const position = await local.readingPosition(itemId)
  if (!position) throw new Error(`Reading position not found: ${itemId}`)
  return position
}
import { getBrowserLocalReplicaStore } from "./browser-local-replica.ts"
import type { LocalReplicaStore } from "./local-replica-store.ts"
