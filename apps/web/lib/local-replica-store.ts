import type {
  LocalReplicaEntity,
  LocalReplicaEntityRef,
  ReadioSdkClient,
  ReplicaLibraryItem,
  ReplicaLibraryPage,
  ReplicaLibraryQuery,
  ReplicaPositionMutationResponse,
  ReplicaReadingPosition,
  ReplicaReadingPositionMutation,
  ReplicaState,
} from "./readio-sdk.ts"
import type { Bookmark } from "./rust-core.ts"
import {
  FetchLibraryContentTransport,
  FetchReplicaTransport,
  WebLibraryContentCloudReconciler,
  WebReplicaSyncCoordinator,
  type LibraryContentCloudTransport,
  ReplicaSyncPersistence,
  ReplicaSyncRecord,
  type ReplicaTransport,
} from "./replica-sync.ts"

export type LocalReplicaPersistence = ReplicaSyncPersistence
export type LocalReplicaRecord = ReplicaSyncRecord

export interface LocalLibraryContentRecord {
  itemId: string
  content: string
  fileBytes: Uint8Array | null
  mediaType: string | null
  fileName: string | null
}

export interface LocalLibraryContentPersistence {
  load(itemId: string): Promise<LocalLibraryContentRecord | null>
  save(record: LocalLibraryContentRecord): Promise<void>
  delete(itemId: string): Promise<void>
}

export interface CloudSyncOptions {
  baseURL: string
  accessToken?: string
  deviceId: string
}

export interface CloudSyncTransports {
  replica: ReplicaTransport
  content: LibraryContentCloudTransport
}

export type CloudTransportFactory = (
  options: CloudSyncOptions
) => CloudSyncTransports

const EMPTY_RECORD: LocalReplicaRecord = {
  cursor: 0,
  stateJSON: "{}",
}

export interface LocalReplicaStore {
  state(): Promise<ReplicaState>
  bookmarks(itemId: string): Promise<Bookmark[]>
  settings(): Promise<Record<string, string>>
  queryLibrary(query: ReplicaLibraryQuery): Promise<ReplicaLibraryPage>
  libraryItem(itemId: string): Promise<ReplicaLibraryItem | null>
  readingPosition(itemId: string): Promise<ReplicaReadingPosition | null>
  saveReadingPosition(
    itemId: string,
    mutation: ReplicaReadingPositionMutation
  ): Promise<ReplicaPositionMutationResponse>
  libraryContent(itemId: string): Promise<LocalLibraryContentRecord | null>
  saveLibraryContent(record: LocalLibraryContentRecord): Promise<void>
  deleteLibraryContent(itemId: string): Promise<void>
  syncCloud(options: CloudSyncOptions): Promise<ReplicaState>
  upsert(entities: LocalReplicaEntity[]): Promise<ReplicaState>
  delete(entities: LocalReplicaEntityRef[]): Promise<ReplicaState>
  patch(
    upserts: LocalReplicaEntity[],
    deletions: LocalReplicaEntityRef[]
  ): Promise<ReplicaState>
}

/**
 * Executes local mutations through the Rust sync state and makes its snapshot
 * plus outbox durable in one platform transaction. Physical IndexedDB I/O is
 * injected; CRDT identity, replay, deletion, and materialization stay in Rust.
 */
export class WebLocalReplicaStore implements LocalReplicaStore {
  private pending: Promise<void> = Promise.resolve()
  private readonly sdk: ReadioSdkClient
  private readonly persistence: LocalReplicaPersistence
  private readonly contentPersistence?: LocalLibraryContentPersistence
  private readonly cloudTransportFactory: CloudTransportFactory

  constructor(options: {
    sdk: ReadioSdkClient
    persistence: LocalReplicaPersistence
    contentPersistence?: LocalLibraryContentPersistence
    cloudTransportFactory?: CloudTransportFactory
  }) {
    this.sdk = options.sdk
    this.persistence = options.persistence
    this.contentPersistence = options.contentPersistence
    this.cloudTransportFactory =
      options.cloudTransportFactory ?? defaultCloudTransportFactory
  }

  state(): Promise<ReplicaState> {
    return this.serialized(async () => {
      const record = (await this.persistence.load()) ?? EMPTY_RECORD
      const state = await this.sdk.newSyncState(record.stateJSON)
      try {
        return state.state()
      } finally {
        state.free()
      }
    })
  }

  bookmarks(itemId: string): Promise<Bookmark[]> {
    return this.serialized(async () => {
      const record = (await this.persistence.load()) ?? EMPTY_RECORD
      const state = await this.sdk.newSyncState(record.stateJSON)
      try {
        return state.bookmarks(itemId)
      } finally {
        state.free()
      }
    })
  }

  settings(): Promise<Record<string, string>> {
    return this.serialized(async () => {
      const record = (await this.persistence.load()) ?? EMPTY_RECORD
      const state = await this.sdk.newSyncState(record.stateJSON)
      try {
        return state.settings()
      } finally {
        state.free()
      }
    })
  }

  queryLibrary(query: ReplicaLibraryQuery): Promise<ReplicaLibraryPage> {
    return this.serialized(async () => {
      const record = (await this.persistence.load()) ?? EMPTY_RECORD
      const state = await this.sdk.newSyncState(record.stateJSON)
      try {
        return state.queryLibrary(query)
      } finally {
        state.free()
      }
    })
  }

  libraryItem(itemId: string): Promise<ReplicaLibraryItem | null> {
    return this.serialized(async () => {
      const record = (await this.persistence.load()) ?? EMPTY_RECORD
      const state = await this.sdk.newSyncState(record.stateJSON)
      try {
        return state.libraryItem(itemId)
      } finally {
        state.free()
      }
    })
  }

  readingPosition(itemId: string): Promise<ReplicaReadingPosition | null> {
    return this.serialized(async () => {
      const record = (await this.persistence.load()) ?? EMPTY_RECORD
      const state = await this.sdk.newSyncState(record.stateJSON)
      try {
        return state.readingPosition(itemId)
      } finally {
        state.free()
      }
    })
  }

  saveReadingPosition(
    itemId: string,
    mutation: ReplicaReadingPositionMutation
  ): Promise<ReplicaPositionMutationResponse> {
    return this.serialized(async () => {
      const record = (await this.persistence.load()) ?? EMPTY_RECORD
      const state = await this.sdk.newSyncState(record.stateJSON)
      try {
        const response = state.saveReadingPosition(itemId, mutation)
        if (response.status === "applied" && !response.idempotent_replay) {
          await this.persistence.save({
            cursor: record.cursor,
            stateJSON: state.serialize(),
          })
        }
        return response
      } finally {
        state.free()
      }
    })
  }

  libraryContent(itemId: string): Promise<LocalLibraryContentRecord | null> {
    return this.serialized(() => this.contentStore().load(itemId))
  }

  saveLibraryContent(record: LocalLibraryContentRecord): Promise<void> {
    return this.serialized(() => this.contentStore().save(record))
  }

  deleteLibraryContent(itemId: string): Promise<void> {
    return this.serialized(() => this.contentStore().delete(itemId))
  }

  syncCloud(options: CloudSyncOptions): Promise<ReplicaState> {
    return this.serialized(async () => {
      const transports = this.cloudTransportFactory(options)
      const contentReconciler = this.contentPersistence
        ? new WebLibraryContentCloudReconciler({
            contentPersistence: this.contentPersistence,
            transport: transports.content,
          })
        : undefined
      const coordinator = new WebReplicaSyncCoordinator({
        deviceId: options.deviceId,
        sdk: this.sdk,
        persistence: this.persistence,
        transport: transports.replica,
        contentReconciler,
      })
      return (await coordinator.sync([])).state
    })
  }

  upsert(entities: LocalReplicaEntity[]): Promise<ReplicaState> {
    return this.patch(entities, [])
  }

  delete(entities: LocalReplicaEntityRef[]): Promise<ReplicaState> {
    return this.patch([], entities)
  }

  patch(
    upserts: LocalReplicaEntity[],
    deletions: LocalReplicaEntityRef[]
  ): Promise<ReplicaState> {
    return this.mutate((state) => {
      state.stageLocal(upserts)
      state.deleteLocal(deletions)
    })
  }

  private mutate(
    apply: (
      state: Awaited<ReturnType<ReadioSdkClient["newSyncState"]>>
    ) => void
  ): Promise<ReplicaState> {
    return this.serialized(async () => {
      const record = (await this.persistence.load()) ?? EMPTY_RECORD
      const state = await this.sdk.newSyncState(record.stateJSON)
      try {
        apply(state)
        const nextRecord = {
          cursor: record.cursor,
          stateJSON: state.serialize(),
        }
        await this.persistence.save(nextRecord)
        return state.state()
      } finally {
        state.free()
      }
    })
  }

  private serialized<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.pending.then(operation, operation)
    this.pending = result.then(
      () => undefined,
      () => undefined
    )
    return result
  }

  private contentStore(): LocalLibraryContentPersistence {
    if (!this.contentPersistence) {
      throw new Error("Local library content persistence is unavailable")
    }
    return this.contentPersistence
  }
}

function defaultCloudTransportFactory(
  options: CloudSyncOptions
): CloudSyncTransports {
  const transportOptions = {
    baseURL: new URL(options.baseURL),
    accessToken: options.accessToken,
  }
  return {
    replica: new FetchReplicaTransport(transportOptions),
    content: new FetchLibraryContentTransport(transportOptions),
  }
}
