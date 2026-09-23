import assert from "node:assert/strict"
import test from "node:test"

import { ReaderProgressRestoreGate } from "../lib/reader-progress-restore.ts"

test("a cancelled cursor restore may retry but a completed restore may not", () => {
  const gate = new ReaderProgressRestoreGate()

  const first = gate.begin()
  assert.notEqual(first, null)
  assert.equal(gate.begin(), null)

  gate.cancel(first!)
  const retry = gate.begin()
  assert.notEqual(retry, null)

  gate.complete(retry!)
  gate.cancel(retry!)
  assert.equal(gate.begin(), null)
})
