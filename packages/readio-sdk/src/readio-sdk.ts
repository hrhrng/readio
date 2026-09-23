import {
  createRustCoreClient,
  type Bookmark,
  type ImportRecord,
  type TextLocator,
  type RustCoreClient,
  type RustCoreWasmModule,
} from "./rust-core.ts"

export type ReplicaLibraryItemType = "web" | "txt" | "pdf" | "epub"
export type ReplicaLibraryItemCategory = "imported" | "podcasts"

export interface ReplicaLibraryItem {
  id: string
  title: string
  author?: string
  type: ReplicaLibraryItemType
  progress: number
  date: string
  category: ReplicaLibraryItemCategory
  folder_id: string
  source: string
  file_path: string | null
  cover_image: string | null
  voice: string | null
  speed: number | null
  current_chapter: string | null
  content_format: string | null
  import_record?: ImportRecord | null
}

export interface ReplicaLibraryPage {
  items: ReplicaLibraryItem[]
  total: number
  page: number
  page_size: number
  total_pages: number
}

export interface ReplicaLibraryQuery {
  category?: ReplicaLibraryItemCategory
  search?: string
  folder_id?: string
  type?: ReplicaLibraryItemType
  page?: number
  page_size?: number
  sort_by?: "created_at" | "title" | "progress"
  sort_order?: "ascending" | "descending"
  progress_min?: number
  progress_max?: number
}

export interface ReplicaReadingPosition {
  document_revision: string
  char_offset: number
  affinity: "forward" | "backward"
  observed_at_unix_ms: number
  session_id: string
  mutation_id: string
  base_version: number
  version: number
  locator?: TextLocator | null
}

export type ReplicaReadingPositionMutation = Omit<
  ReplicaReadingPosition,
  "version"
>

export interface ReplicaPositionCandidate extends ReplicaReadingPosition {
  origin: "current" | "incoming"
}

export interface ReplicaPositionMutationResponse {
  status: "applied" | "conflict"
  idempotent_replay: boolean
  version: number
  position: ReplicaReadingPosition | null
  conflict_id: string | null
  candidates: ReplicaPositionCandidate[]
}

export type ReplicaEntityKind =
  | "library_item"
  | "reading_position"
  | "bookmark"
  | "setting"

export type ReplicaState = Record<string, Record<string, unknown>>

export interface LocalReplicaEntity {
  entityKind: ReplicaEntityKind
  entityId: string
  payload: unknown
}

export interface LocalReplicaEntityRef {
  entityKind: ReplicaEntityKind
  entityId: string
}

export interface PendingReplicaChange {
  changeId: string
  changeBase64: string
}

export interface ReadioReplicaWasmHandle {
  upsert(
    entityKind: ReplicaEntityKind,
    entityId: string,
    payloadJSON: string
  ): Uint8Array
  delete_entity(
    entityKind: ReplicaEntityKind,
    entityId: string
  ): Uint8Array
  import_change(change: Uint8Array): void
  snapshot(): Uint8Array
  heads(): Uint8Array
  state_json(): string
  free(): void
}

export interface ReadioReplicaWasmConstructor {
  new (): ReadioReplicaWasmHandle
  from_snapshot(snapshot: Uint8Array): ReadioReplicaWasmHandle
}

export interface ReadioSyncStateWasmHandle {
  stage_local(entitiesJSON: string): void
  delete_local(entitiesJSON: string): void
  import_remote(changesJSON: string): void
  acknowledge(changeIdsJSON: string): void
  json(): string
  outbox_json(): string
  snapshot(): Uint8Array
  heads(): Uint8Array
  state_json(): string
  bookmarks_json(itemId: string): string
  settings_json(): string
  query_library_json(queryJSON: string): string
  library_item_json(itemId: string): string
  reading_position_json(itemId: string): string
  save_reading_position_json(itemId: string, mutationJSON: string): string
  free(): void
}

export interface ReadioSyncStateWasmConstructor {
  new (): ReadioSyncStateWasmHandle
  from_json(stateJSON: string): ReadioSyncStateWasmHandle
}

export interface ReadioSdkWasmModule extends RustCoreWasmModule {
  ReadioReplicaWasm: ReadioReplicaWasmConstructor
  ReadioSyncStateWasm: ReadioSyncStateWasmConstructor
}

export type ReadioSdkWasmLoader = () => Promise<ReadioSdkWasmModule>

export interface ReadioReplicaClient {
  upsert(
    entityKind: ReplicaEntityKind,
    entityId: string,
    payload: unknown
  ): Uint8Array
  delete(entityKind: ReplicaEntityKind, entityId: string): Uint8Array
  importChange(change: Uint8Array): void
  snapshot(): Uint8Array
  heads(): Uint8Array
  state(): ReplicaState
  free(): void
}

export interface ReadioSdkClient extends RustCoreClient {
  newReplica(snapshot?: Uint8Array): Promise<ReadioReplicaClient>
  newSyncState(serialized?: string): Promise<ReadioSyncStateClient>
}

export interface ReadioSyncStateClient {
  stageLocal(entities: LocalReplicaEntity[]): void
  deleteLocal(entities: LocalReplicaEntityRef[]): void
  importRemote(changes: PendingReplicaChange[]): void
  acknowledge(changeIds: string[]): void
  serialize(): string
  outbox(): PendingReplicaChange[]
  snapshot(): Uint8Array
  heads(): Uint8Array
  state(): ReplicaState
  bookmarks(itemId: string): Bookmark[]
  settings(): Record<string, string>
  queryLibrary(query: ReplicaLibraryQuery): ReplicaLibraryPage
  libraryItem(itemId: string): ReplicaLibraryItem | null
  readingPosition(itemId: string): ReplicaReadingPosition | null
  saveReadingPosition(
    itemId: string,
    mutation: ReplicaReadingPositionMutation
  ): ReplicaPositionMutationResponse
  free(): void
}

export function createReadioSdkClient(
  loadModule: ReadioSdkWasmLoader
): ReadioSdkClient {
  const core = createRustCoreClient(loadModule)

  return {
    ...core,
    async newReplica(snapshot) {
      const wasm = await loadModule()
      if (typeof wasm.ReadioReplicaWasm !== "function") {
        throw new Error("Readio SDK WASM does not expose ReadioReplicaWasm")
      }
      const replica =
        snapshot === undefined
          ? new wasm.ReadioReplicaWasm()
          : wasm.ReadioReplicaWasm.from_snapshot(snapshot)
      return wrapReplica(replica)
    },
    async newSyncState(serialized) {
      const wasm = await loadModule()
      if (typeof wasm.ReadioSyncStateWasm !== "function") {
        throw new Error("Readio SDK WASM does not expose ReadioSyncStateWasm")
      }
      const state =
        serialized === undefined
          ? new wasm.ReadioSyncStateWasm()
          : wasm.ReadioSyncStateWasm.from_json(serialized)
      return wrapSyncState(state)
    },
  }
}

function wrapSyncState(state: ReadioSyncStateWasmHandle): ReadioSyncStateClient {
  return {
    stageLocal(entities) {
      state.stage_local(JSON.stringify(entities))
    },
    deleteLocal(entities) {
      state.delete_local(JSON.stringify(entities))
    },
    importRemote(changes) {
      state.import_remote(JSON.stringify(changes))
    },
    acknowledge(changeIds) {
      state.acknowledge(JSON.stringify(changeIds))
    },
    serialize() {
      return state.json()
    },
    outbox() {
      return JSON.parse(state.outbox_json()) as PendingReplicaChange[]
    },
    snapshot() {
      return state.snapshot()
    },
    heads() {
      return state.heads()
    },
    state() {
      return JSON.parse(state.state_json()) as ReplicaState
    },
    bookmarks(itemId) {
      return JSON.parse(state.bookmarks_json(itemId)) as Bookmark[]
    },
    settings() {
      return JSON.parse(state.settings_json()) as Record<string, string>
    },
    queryLibrary(query) {
      return JSON.parse(
        state.query_library_json(JSON.stringify(query))
      ) as ReplicaLibraryPage
    },
    libraryItem(itemId) {
      return JSON.parse(
        state.library_item_json(itemId)
      ) as ReplicaLibraryItem | null
    },
    readingPosition(itemId) {
      return JSON.parse(
        state.reading_position_json(itemId)
      ) as ReplicaReadingPosition | null
    },
    saveReadingPosition(itemId, mutation) {
      return JSON.parse(
        state.save_reading_position_json(itemId, JSON.stringify(mutation))
      ) as ReplicaPositionMutationResponse
    },
    free() {
      state.free()
    },
  }
}

function wrapReplica(replica: ReadioReplicaWasmHandle): ReadioReplicaClient {
  return {
    upsert(entityKind, entityId, payload) {
      return replica.upsert(entityKind, entityId, JSON.stringify(payload))
    },
    delete(entityKind, entityId) {
      return replica.delete_entity(entityKind, entityId)
    },
    importChange(change) {
      replica.import_change(change)
    },
    snapshot() {
      return replica.snapshot()
    },
    heads() {
      return replica.heads()
    },
    state() {
      return JSON.parse(replica.state_json()) as ReplicaState
    },
    free() {
      replica.free()
    },
  }
}
