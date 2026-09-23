import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

import {
  scalarTextRuns,
} from "../components/reader/content-blocks.tsx"
import { createReadioWasmLoader } from "../lib/readio-core-runtime.ts"
import { resolveReaderHighlight } from "../lib/reader-highlight.ts"
import { createRustCoreClient } from "../lib/rust-core.ts"

test("renderer splits styled text at Rust Unicode-scalar boundaries", () => {
  const runs = scalarTextRuns(
    [
      { text: "A🙂 " },
      { text: "中文" },
    ],
    [1, 4]
  )

  assert.deepEqual(
    runs.flatMap((run) => run.segments.map((segment) => [
      segment.text,
      segment.range,
      segment.highlighted,
    ])),
    [
      ["A", [0, 1], false],
      ["🙂 ", [1, 3], true],
      ["中", [3, 4], true],
      ["文", [4, 5], false],
    ]
  )
})

test("web sends playback progress to Rust and consumes the returned word range", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))
  const document = await core.parseHTML("<p>one two three four.</p>")
  const index = await core.indexDocumentText(document)

  const target = await resolveReaderHighlight(
    core,
    document,
    index,
    0,
    0.5
  )

  assert.deepEqual(target?.word_range, [8, 13])
})
