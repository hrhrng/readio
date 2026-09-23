import { getBrowserLocalReplicaStore } from "./browser-local-replica.ts"
import type { LocalReplicaStore } from "./local-replica-store.ts"
import { ReadingPositionConflictError } from "./reading-position-api.ts"
import type { DocumentIR, RustCoreClient } from "./rust-core.ts"

interface ReaderProgressCommitterOptions {
  itemId: string
  document: DocumentIR
  core: RustCoreClient
  store?: LocalReplicaStore
  sessionId?: string
  now?: () => number
  mutationId?: () => string
}

/** Serial, document-aware writer for the reader's authoritative scalar cursor. */
export class ReaderProgressCommitter {
  private readonly store: Promise<LocalReplicaStore>
  private readonly sessionId: string
  private readonly now: () => number
  private readonly mutationId: () => string
  private readonly revisionDigest: string
  private pending: Promise<void> = Promise.resolve()
  private baseVersion: number | null = null
  private lastCommittedOffset: number | null = null

  constructor(private readonly options: ReaderProgressCommitterOptions) {
    if (!options.document.revision) {
      throw new Error("Reader progress requires a versioned document")
    }
    this.revisionDigest = options.document.revision.revision_digest
    this.store = options.store
      ? Promise.resolve(options.store)
      : getBrowserLocalReplicaStore()
    this.sessionId = options.sessionId ?? crypto.randomUUID()
    this.now = options.now ?? Date.now
    this.mutationId = options.mutationId ?? (() => crypto.randomUUID())
  }

  commitSentence(sentence: number): Promise<void> {
    return this.serialized(async () => {
      const block = this.options.document.blocks[sentence]
      if (!block) throw new RangeError(`Sentence is outside the document: ${sentence}`)

      const projection = await this.options.core.resolveBlockOffset(
        this.options.document,
        block.id,
        0,
        "forward"
      )
      const locator = await this.options.core.createTextLocator(
        this.options.document,
        projection.char_offset,
        "forward"
      )
      await this.loadCurrentPosition()
      if (projection.char_offset === this.lastCommittedOffset) return

      const store = await this.store
      const response = await store.saveReadingPosition(this.options.itemId, {
        document_revision: this.revisionDigest,
        char_offset: projection.char_offset,
        affinity: "forward",
        observed_at_unix_ms: Math.round(this.now()),
        session_id: this.sessionId,
        mutation_id: this.mutationId(),
        base_version: this.baseVersion ?? 0,
        locator,
      })
      if (response.status === "conflict") {
        throw new ReadingPositionConflictError(response)
      }
      this.baseVersion = response.version
      this.lastCommittedOffset = projection.char_offset
    })
  }

  restoreSentence(): Promise<number | null> {
    return this.serialized(async () => {
      const position = await this.loadCurrentPosition()
      if (!position) return null
      const restored = await this.options.core.restorePosition(
        {
          item_id: this.options.itemId,
          document_revision: position.document_revision,
          char_offset: position.char_offset,
          affinity: position.affinity,
          observed_at_unix_ms: position.observed_at_unix_ms,
          session_id: position.session_id,
          mutation_id: position.mutation_id,
          base_version: position.base_version,
          locator: position.locator ?? null,
        },
        this.options.document
      )
      const blockId = restored.projection.block_id
      if (!blockId) return null
      const sentence = this.options.document.blocks.findIndex(
        (block) => block.id === blockId
      )
      return sentence < 0 ? null : sentence
    })
  }

  private async loadCurrentPosition() {
    const current = await (await this.store).readingPosition(this.options.itemId)
    this.baseVersion = current?.version ?? 0
    this.lastCommittedOffset =
      current?.document_revision === this.revisionDigest
        ? current.char_offset
        : null
    return current
  }

  private serialized<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.pending.then(operation, operation)
    this.pending = result.then(
      () => undefined,
      () => undefined
    )
    return result
  }
}
