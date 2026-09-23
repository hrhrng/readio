import assert from "node:assert/strict"
import test from "node:test"

import sqlite3InitModule from "@sqlite.org/sqlite-wasm"

import * as sqlitePersistence from "../lib/sqlite-replica-persistence.ts"

const { SqliteReplicaSyncPersistence } = sqlitePersistence

test("SQLite WASM stores and replaces the durable Rust replica record transactionally", async () => {
  const sqlite3 = await sqlite3InitModule()
  const database = new sqlite3.oo1.DB(":memory:", "c")

  try {
    const first = new SqliteReplicaSyncPersistence(database)
    assert.equal(await first.load(), null)

    await first.save({ cursor: 3, stateJSON: "{\"outbox\":[1]}" })
    await first.save({ cursor: 4, stateJSON: "{\"outbox\":[]}" })

    const restored = new SqliteReplicaSyncPersistence(database)
    assert.deepEqual(await restored.load(), {
      cursor: 4,
      stateJSON: "{\"outbox\":[]}",
    })
    assert.deepEqual(
      database.exec({
        sql: "SELECT schema_version FROM readio_meta",
        rowMode: 0,
        returnValue: "resultRows",
      }),
      [2]
    )
  } finally {
    database.close()
  }
})

test("SQLite WASM stores local item text and source bytes without base64", async () => {
  const sqlite3 = await sqlite3InitModule()
  const database = new sqlite3.oo1.DB(":memory:", "c")

  try {
    const ContentPersistence = Reflect.get(
      sqlitePersistence,
      "SqliteLibraryContentPersistence"
    )
    assert.equal(typeof ContentPersistence, "function")
    const first = new ContentPersistence(database)
    assert.equal(await first.load("book-1"), null)

    await first.save({
      itemId: "book-1",
      content: "Hello, 本地阅读",
      fileBytes: new Uint8Array([0, 1, 2, 255]),
      mediaType: "text/plain",
      fileName: "hello.txt",
    })

    const restored = new ContentPersistence(database)
    assert.deepEqual(await restored.load("book-1"), {
      itemId: "book-1",
      content: "Hello, 本地阅读",
      fileBytes: new Uint8Array([0, 1, 2, 255]),
      mediaType: "text/plain",
      fileName: "hello.txt",
    })

    await restored.delete("book-1")
    assert.equal(await restored.load("book-1"), null)
  } finally {
    database.close()
  }
})
