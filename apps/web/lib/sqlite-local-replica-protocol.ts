import type {
  CloudSyncOptions,
  LocalLibraryContentRecord,
  LocalReplicaStore,
} from "./local-replica-store.ts"
import type {
  LocalReplicaEntity,
  LocalReplicaEntityRef,
  ReplicaLibraryItem,
  ReplicaLibraryPage,
  ReplicaLibraryQuery,
  ReplicaPositionMutationResponse,
  ReplicaReadingPosition,
  ReplicaReadingPositionMutation,
  ReplicaState,
} from "./readio-sdk.ts"
import type { Bookmark } from "./rust-core.ts"

export type LocalReplicaRequest =
  | { id: number; type: "state" }
  | { id: number; type: "bookmarks"; itemId: string }
  | { id: number; type: "settings" }
  | { id: number; type: "queryLibrary"; query: ReplicaLibraryQuery }
  | { id: number; type: "libraryItem"; itemId: string }
  | { id: number; type: "readingPosition"; itemId: string }
  | {
      id: number
      type: "saveReadingPosition"
      itemId: string
      mutation: ReplicaReadingPositionMutation
    }
  | { id: number; type: "libraryContent"; itemId: string }
  | {
      id: number
      type: "saveLibraryContent"
      record: LocalLibraryContentRecord
    }
  | { id: number; type: "deleteLibraryContent"; itemId: string }
  | { id: number; type: "syncCloud"; options: CloudSyncOptions }
  | { id: number; type: "upsert"; entities: LocalReplicaEntity[] }
  | { id: number; type: "delete"; entities: LocalReplicaEntityRef[] }
  | {
      id: number
      type: "patch"
      upserts: LocalReplicaEntity[]
      deletions: LocalReplicaEntityRef[]
    }

type LocalReplicaRequestBody =
  | { type: "state" }
  | { type: "bookmarks"; itemId: string }
  | { type: "settings" }
  | { type: "queryLibrary"; query: ReplicaLibraryQuery }
  | { type: "libraryItem"; itemId: string }
  | { type: "readingPosition"; itemId: string }
  | {
      type: "saveReadingPosition"
      itemId: string
      mutation: ReplicaReadingPositionMutation
    }
  | { type: "libraryContent"; itemId: string }
  | { type: "saveLibraryContent"; record: LocalLibraryContentRecord }
  | { type: "deleteLibraryContent"; itemId: string }
  | { type: "syncCloud"; options: CloudSyncOptions }
  | { type: "upsert"; entities: LocalReplicaEntity[] }
  | { type: "delete"; entities: LocalReplicaEntityRef[] }
  | {
      type: "patch"
      upserts: LocalReplicaEntity[]
      deletions: LocalReplicaEntityRef[]
    }

export type LocalReplicaResponse =
  | {
      id: number
      ok: true
      value:
        | ReplicaState
        | Bookmark[]
        | Record<string, string>
        | ReplicaLibraryPage
        | ReplicaLibraryItem
        | ReplicaReadingPosition
        | ReplicaPositionMutationResponse
        | LocalLibraryContentRecord
        | null
    }
  | { id: number; ok: false; error: string }

export interface LocalReplicaMessageEndpoint {
  postMessage(message: LocalReplicaRequest): void
  onmessage: ((event: MessageEvent<LocalReplicaResponse>) => void) | null
  onmessageerror?: ((event: MessageEvent) => void) | null
  terminate?: () => void
  close?: () => void
}

export class SqliteWorkerLocalReplicaStore implements LocalReplicaStore {
  private nextRequestId = 1
  private readonly pending = new Map<
    number,
    {
      resolve: (
        value:
          | ReplicaState
          | Bookmark[]
          | Record<string, string>
          | ReplicaLibraryPage
          | ReplicaLibraryItem
          | ReplicaReadingPosition
          | ReplicaPositionMutationResponse
          | LocalLibraryContentRecord
          | null
      ) => void
      reject: (error: Error) => void
    }
  >()

  constructor(private readonly endpoint: LocalReplicaMessageEndpoint) {
    this.endpoint.onmessage = (event) => {
      const response = event.data
      const pending = this.pending.get(response.id)
      if (!pending) return
      this.pending.delete(response.id)
      if (response.ok) pending.resolve(response.value)
      else pending.reject(new Error(response.error))
    }
    this.endpoint.onmessageerror = () => {
      this.rejectPending(new Error("Could not decode SQLite Worker response"))
    }
  }

  state(): Promise<ReplicaState> {
    return this.request<ReplicaState>({ type: "state" })
  }

  bookmarks(itemId: string): Promise<Bookmark[]> {
    return this.request<Bookmark[]>({ type: "bookmarks", itemId })
  }

  settings(): Promise<Record<string, string>> {
    return this.request<Record<string, string>>({ type: "settings" })
  }

  queryLibrary(query: ReplicaLibraryQuery): Promise<ReplicaLibraryPage> {
    return this.request<ReplicaLibraryPage>({ type: "queryLibrary", query })
  }

  libraryItem(itemId: string): Promise<ReplicaLibraryItem | null> {
    return this.request<ReplicaLibraryItem | null>({
      type: "libraryItem",
      itemId,
    })
  }

  readingPosition(itemId: string): Promise<ReplicaReadingPosition | null> {
    return this.request<ReplicaReadingPosition | null>({
      type: "readingPosition",
      itemId,
    })
  }

  saveReadingPosition(
    itemId: string,
    mutation: ReplicaReadingPositionMutation
  ): Promise<ReplicaPositionMutationResponse> {
    return this.request<ReplicaPositionMutationResponse>({
      type: "saveReadingPosition",
      itemId,
      mutation,
    })
  }

  libraryContent(itemId: string): Promise<LocalLibraryContentRecord | null> {
    return this.request<LocalLibraryContentRecord | null>({
      type: "libraryContent",
      itemId,
    })
  }

  async saveLibraryContent(record: LocalLibraryContentRecord): Promise<void> {
    await this.request<null>({ type: "saveLibraryContent", record })
  }

  async deleteLibraryContent(itemId: string): Promise<void> {
    await this.request<null>({ type: "deleteLibraryContent", itemId })
  }

  syncCloud(options: CloudSyncOptions): Promise<ReplicaState> {
    return this.request<ReplicaState>({ type: "syncCloud", options })
  }

  upsert(entities: LocalReplicaEntity[]): Promise<ReplicaState> {
    return this.request<ReplicaState>({ type: "upsert", entities })
  }

  delete(entities: LocalReplicaEntityRef[]): Promise<ReplicaState> {
    return this.request<ReplicaState>({ type: "delete", entities })
  }

  patch(
    upserts: LocalReplicaEntity[],
    deletions: LocalReplicaEntityRef[]
  ): Promise<ReplicaState> {
    return this.request<ReplicaState>({ type: "patch", upserts, deletions })
  }

  close(): void {
    this.rejectPending(new Error("SQLite Worker closed"))
    if (this.endpoint.terminate) this.endpoint.terminate()
    else this.endpoint.close?.()
  }

  private request<
    T extends
      | ReplicaState
      | Bookmark[]
      | Record<string, string>
      | ReplicaLibraryPage
      | ReplicaLibraryItem
      | ReplicaReadingPosition
      | ReplicaPositionMutationResponse
      | LocalLibraryContentRecord
      | null
  >(
    request: LocalReplicaRequestBody
  ): Promise<T> {
    const id = this.nextRequestId++
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: (value) => resolve(value as T),
        reject,
      })
      this.endpoint.postMessage({ id, ...request } as LocalReplicaRequest)
    })
  }

  private rejectPending(error: Error): void {
    for (const pending of this.pending.values()) pending.reject(error)
    this.pending.clear()
  }
}

export async function handleLocalReplicaRequest(
  store: LocalReplicaStore,
  request: LocalReplicaRequest
): Promise<LocalReplicaResponse> {
  try {
    switch (request.type) {
      case "state":
        return { id: request.id, ok: true, value: await store.state() }
      case "bookmarks":
        return {
          id: request.id,
          ok: true,
          value: await store.bookmarks(request.itemId),
        }
      case "settings":
        return { id: request.id, ok: true, value: await store.settings() }
      case "queryLibrary":
        return {
          id: request.id,
          ok: true,
          value: await store.queryLibrary(request.query),
        }
      case "libraryItem":
        return {
          id: request.id,
          ok: true,
          value: await store.libraryItem(request.itemId),
        }
      case "readingPosition":
        return {
          id: request.id,
          ok: true,
          value: await store.readingPosition(request.itemId),
        }
      case "saveReadingPosition":
        return {
          id: request.id,
          ok: true,
          value: await store.saveReadingPosition(
            request.itemId,
            request.mutation
          ),
        }
      case "libraryContent":
        return {
          id: request.id,
          ok: true,
          value: await store.libraryContent(request.itemId),
        }
      case "saveLibraryContent":
        await store.saveLibraryContent(request.record)
        return { id: request.id, ok: true, value: null }
      case "deleteLibraryContent":
        await store.deleteLibraryContent(request.itemId)
        return { id: request.id, ok: true, value: null }
      case "syncCloud":
        return {
          id: request.id,
          ok: true,
          value: await store.syncCloud(request.options),
        }
      case "upsert":
        return {
          id: request.id,
          ok: true,
          value: await store.upsert(request.entities),
        }
      case "delete":
        return {
          id: request.id,
          ok: true,
          value: await store.delete(request.entities),
        }
      case "patch":
        return {
          id: request.id,
          ok: true,
          value: await store.patch(request.upserts, request.deletions),
        }
    }
  } catch (error) {
    return {
      id: request.id,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    }
  }
}
