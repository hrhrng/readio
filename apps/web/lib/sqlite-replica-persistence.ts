import type { Database, SqlValue } from "@sqlite.org/sqlite-wasm"

import type {
  LocalLibraryContentPersistence,
  LocalLibraryContentRecord,
} from "./local-replica-store.ts"
import type {
  ReplicaSyncPersistence,
  ReplicaSyncRecord,
} from "./replica-sync.ts"

const SCHEMA_VERSION = 2

/**
 * SQLite is the physical durability effect for the Rust-owned replica state.
 * The class is intentionally synchronous inside each async method so callers
 * can place it in one dedicated Worker and never share a connection.
 */
export class SqliteReplicaSyncPersistence
  implements ReplicaSyncPersistence
{
  constructor(private readonly database: Database) {
    initializeSchema(this.database)
  }

  async load(): Promise<ReplicaSyncRecord | null> {
    const rows = this.database.exec({
      sql: "SELECT cursor, state_json FROM readio_replica_state WHERE singleton = 1",
      rowMode: "object",
      returnValue: "resultRows",
    })
    const row = rows[0]
    if (!row) return null

    const cursor = numberValue(row.cursor)
    const stateJSON = row.state_json
    if (!Number.isSafeInteger(cursor) || cursor < 0 || typeof stateJSON !== "string") {
      throw new Error("Invalid Readio replica record in SQLite")
    }
    return { cursor, stateJSON }
  }

  async save(record: ReplicaSyncRecord): Promise<void> {
    if (!Number.isSafeInteger(record.cursor) || record.cursor < 0) {
      throw new Error("Replica cursor must be a non-negative safe integer")
    }

    this.database.exec("BEGIN IMMEDIATE")
    try {
      const statement = this.database.prepare(
        `INSERT INTO readio_replica_state(singleton, cursor, state_json)
         VALUES (1, ?, ?)
         ON CONFLICT(singleton) DO UPDATE SET
           cursor = excluded.cursor,
           state_json = excluded.state_json`
      )
      try {
        statement.bind([record.cursor, record.stateJSON]).step()
      } finally {
        statement.finalize()
      }
      this.database.exec("COMMIT")
    } catch (error) {
      this.database.exec("ROLLBACK")
      throw error
    }
  }
}

/** Stores parsed text and original file bytes beside, but outside, the CRDT. */
export class SqliteLibraryContentPersistence
  implements LocalLibraryContentPersistence
{
  constructor(private readonly database: Database) {
    initializeSchema(this.database)
  }

  async load(itemId: string): Promise<LocalLibraryContentRecord | null> {
    const rows = this.database.exec({
      sql: `SELECT item_id, content, file_bytes, media_type, file_name
            FROM readio_library_content WHERE item_id = ?`,
      bind: [itemId],
      rowMode: "object",
      returnValue: "resultRows",
    })
    const row = rows[0]
    if (!row) return null
    if (
      typeof row.item_id !== "string" ||
      typeof row.content !== "string" ||
      (row.media_type !== null && typeof row.media_type !== "string") ||
      (row.file_name !== null && typeof row.file_name !== "string")
    ) {
      throw new Error("Invalid Readio library content record in SQLite")
    }

    return {
      itemId: row.item_id,
      content: row.content,
      fileBytes: bytesValue(row.file_bytes),
      mediaType: row.media_type,
      fileName: row.file_name,
    }
  }

  async save(record: LocalLibraryContentRecord): Promise<void> {
    if (!record.itemId) throw new Error("Library content item id is required")

    const statement = this.database.prepare(
      `INSERT INTO readio_library_content(
         item_id, content, file_bytes, media_type, file_name
       ) VALUES (?, ?, ?, ?, ?)
       ON CONFLICT(item_id) DO UPDATE SET
         content = excluded.content,
         file_bytes = excluded.file_bytes,
         media_type = excluded.media_type,
         file_name = excluded.file_name`
    )
    try {
      statement
        .bind([
          record.itemId,
          record.content,
          record.fileBytes,
          record.mediaType,
          record.fileName,
        ])
        .step()
    } finally {
      statement.finalize()
    }
  }

  async delete(itemId: string): Promise<void> {
    const statement = this.database.prepare(
      "DELETE FROM readio_library_content WHERE item_id = ?"
    )
    try {
      statement.bind(itemId).step()
    } finally {
      statement.finalize()
    }
  }
}

function initializeSchema(database: Database): void {
  database.exec([
    "PRAGMA journal_mode=WAL;",
    "PRAGMA synchronous=FULL;",
    "CREATE TABLE IF NOT EXISTS readio_meta (",
    "  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),",
    "  schema_version INTEGER NOT NULL",
    ");",
    "INSERT OR IGNORE INTO readio_meta(singleton, schema_version) VALUES (1, 1);",
  ])

  const storedVersion = database.exec({
    sql: "SELECT schema_version FROM readio_meta WHERE singleton = 1",
    rowMode: 0,
    returnValue: "resultRows",
  })[0]
  const version = numberValue(storedVersion)
  if (version < 1 || version > SCHEMA_VERSION) {
    throw new Error(`Unsupported Readio SQLite schema version: ${storedVersion}`)
  }

  database.exec("BEGIN IMMEDIATE")
  try {
    database.exec([
      "CREATE TABLE IF NOT EXISTS readio_replica_state (",
      "  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),",
      "  cursor INTEGER NOT NULL CHECK (cursor >= 0),",
      "  state_json TEXT NOT NULL",
      ");",
      "CREATE TABLE IF NOT EXISTS readio_library_content (",
      "  item_id TEXT PRIMARY KEY,",
      "  content TEXT NOT NULL,",
      "  file_bytes BLOB,",
      "  media_type TEXT,",
      "  file_name TEXT",
      ");",
      `UPDATE readio_meta SET schema_version = ${SCHEMA_VERSION} WHERE singleton = 1;`,
    ])
    database.exec("COMMIT")
  } catch (error) {
    database.exec("ROLLBACK")
    throw error
  }
}

function numberValue(value: SqlValue | undefined): number {
  return typeof value === "bigint" ? Number(value) : Number(value)
}

function bytesValue(value: SqlValue | undefined): Uint8Array | null {
  if (value === null || value === undefined) return null
  if (value instanceof Uint8Array) return value.slice()
  if (value instanceof Int8Array) {
    return new Uint8Array(value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength))
  }
  if (value instanceof ArrayBuffer) return new Uint8Array(value.slice(0))
  throw new Error("Invalid Readio library file bytes in SQLite")
}
