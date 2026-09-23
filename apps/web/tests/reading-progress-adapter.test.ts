import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

import {
  projectSentenceProgress,
  resolveSentenceProgressSeek,
  scalarSentenceCharacterCounts,
} from "../lib/reading-progress-adapter.ts"
import { createReadioWasmLoader } from "../lib/readio-core-runtime.ts"
import { createRustCoreClient } from "../lib/rust-core.ts"

async function core() {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  return createRustCoreClient(createReadioWasmLoader(binary))
}

test("sentence progress uses Unicode scalar length and keeps manual ETA unknown", async () => {
  const sentences = [
    { index: 0, text: "A🙂" },
    { index: 1, text: "中文" },
  ]

  assert.deepEqual(scalarSentenceCharacterCounts(sentences), [2, 2])
  const projection = await projectSentenceProgress(await core(), sentences, {
    currentSegment: 1,
    currentSegmentProgressBasisPoints: 0,
    timing: { type: "unknown" },
  })

  assert.equal(projection.char_offset, 3)
  assert.equal(projection.progress_basis_points, 6_000)
  assert.equal(projection.remaining_time.seconds, null)
})

test("automatic text ETA and progress-bar seek are delegated to Rust", async () => {
  const sentences = [
    { index: 0, text: "A" },
    { index: 1, text: "123456789" },
  ]
  const projection = await projectSentenceProgress(await core(), sentences, {
    currentSegment: 0,
    currentSegmentProgressBasisPoints: 10_000,
    timing: { type: "text_rate", characters_per_second: 2 },
  })
  const cursor = await resolveSentenceProgressSeek(await core(), sentences, 0.5)

  assert.equal(projection.remaining_time.kind, "text_rate")
  assert.equal(projection.remaining_time.seconds, 5)
  assert.deepEqual(cursor, {
    segment: 1,
    segment_progress_basis_points: 3_333,
    char_offset: 5,
  })
})
