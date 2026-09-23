import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"
import JSZip from "jszip"

import { createRustCoreClient, type RustCoreWasmModule } from "../lib/rust-core.ts"
import {
  createReadioSdkClient,
  type ReadioSdkWasmModule,
} from "../lib/readio-sdk.ts"
import {
  createReadioWasmLoader,
  webReadioSdk,
} from "../lib/readio-core-runtime.ts"
import { sentenceIndexForBlock } from "../lib/reader-document.ts"
import init, * as generatedWasm from "../wasm/readio_wasm.js"

test("the shipped web wasm resolves the same virtual sample as native clients", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))

  const sample = await core.builtinSample("zh-CN")

  assert.equal(sample.item_id, "readio:sample:v1")
  assert.equal(sample.locale, "zh")
  assert.equal(sample.source, "readio://sample")
  assert.equal(sample.is_builtin_sample, true)
  assert.ok(sample.content.startsWith("一 终端里的注意力"))
})

test("the shipped web wasm normalizes the server TTS catalog in Rust", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))

  const normalized = await core.normalizeTTSModelCatalog({
    schemaVersion: 1,
    defaultModelId: "missing",
    models: [
      {
        id: " workers-ai/melotts ",
        provider: " Workers-AI ",
        label: " MeloTTS ",
        languages: ["ZH_hans_cn"],
        voices: [{ id: " default ", label: " Default " }],
        defaultVoiceId: "missing",
      },
    ],
  })

  assert.deepEqual(normalized, {
    schemaVersion: 1,
    defaultModelId: "workers-ai/melotts",
    models: [
      {
        id: "workers-ai/melotts",
        provider: "workers-ai",
        label: "MeloTTS",
        languages: ["zh-Hans-CN"],
        voices: [{ id: "default", label: "Default" }],
        defaultVoiceId: "default",
      },
    ],
  })
})

test("the shipped web wasm runs shared parsing and search code", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  await init({ module_or_path: binary })
  const core = createRustCoreClient(
    async () => generatedWasm as unknown as RustCoreWasmModule
  )

  const document = await core.parseHTML("<p>Quiet rust, quiet web.</p>")
  const result = await core.searchDocument(document, {
    document_revision: document.revision!.revision_digest,
    query: "QUIET",
    cursor: null,
    limit: 10,
  })

  assert.equal(result.total_occurrences, 2)
  assert.deepEqual(
    result.hits.map((hit) => hit.char_range),
    [
      [0, 5],
      [12, 17],
    ]
  )
})

test("the shipped web wasm parses text-layer PDF bytes in Rust", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))

  const parsed = await core.parsePDF(
    minimalTextPDF([
      "Portable PDF parsing belongs in Rust.",
      "The browser receives canonical text.",
    ])
  )

  assert.equal(parsed.title, "Readable PDF")
  assert.equal(parsed.author, null)
  assert.equal(
    parsed.canonical_text,
    "Portable PDF parsing belongs in Rust.\nThe browser receives canonical text."
  )
  assert.equal(parsed.page_count, 1)
  assert.equal(parsed.chapters.length, 1)
  assert.deepEqual(
    parsed.chapters[0].document.blocks.map((block) => block.text),
    [
      "Portable PDF parsing belongs in Rust.",
      "The browser receives canonical text.",
    ]
  )
  assert.deepEqual(
    parsed.chapters[0].block_provenance.map((source) => [
      source.page_start,
      source.page_end,
    ]),
    [
      [1, 1],
      [1, 1],
    ]
  )
})

test("the shipped web wasm parses EPUB spine order in Rust", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))

  const parsed = await core.parseEPUB(await minimalEPUB())

  assert.equal(parsed.title, "Portable EPUB")
  assert.equal(parsed.author, "Rust SDK")
  assert.deepEqual(
    parsed.chapters.map((chapter) => chapter.id),
    ["second", "first"]
  )
  assert.deepEqual(
    parsed.chapters.map((chapter) => chapter.title),
    ["Second Chapter", "First Chapter"]
  )
  assert.deepEqual(
    parsed.toc?.map((entry) => [entry.title, entry.depth, entry.chapter_id]),
    [
      ["Part Two", 0, "second"],
      ["Focused body", 1, "second"],
      ["Part One", 0, "first"],
    ]
  )
  assert.equal(parsed.toc?.[1].block_id, parsed.chapters[0].document.blocks[1].id)
})

test("the shipped web wasm resolves the shared reading mode and effort pace", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  await init({ module_or_path: binary })
  const core = createRustCoreClient(
    async () => generatedWasm as unknown as RustCoreWasmModule
  ) as unknown as {
    resolveReadingPace?: (preferences: {
      mode: "manual" | "auto" | "read_aloud"
      base_text_characters_per_second: number
      effort: "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
    }) => Promise<Record<string, unknown>>
  }

  assert.equal(typeof core.resolveReadingPace, "function")
  assert.deepEqual(
    await core.resolveReadingPace!({
      mode: "read_aloud",
      base_text_characters_per_second: 50,
      effort: "medium",
    }),
    {
      mode: "read_aloud",
      effort: "medium",
      multiplier: 1.5,
      text_characters_per_second: 75,
      audio_playback_rate: 1.5,
      advances_automatically: true,
      uses_audio: true,
    }
  )
})

test("the production wasm loader exposes honest reading progress and scalar seek", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))

  assert.deepEqual(
    await core.projectReadingProgress({
      segment_character_counts: [2, 8],
      current_segment: 1,
      current_segment_progress_basis_points: 5_000,
      timing: { type: "unknown" },
    }),
    {
      char_offset: 7,
      char_count: 11,
      progress_basis_points: 6_363,
      remaining_characters: 4,
      remaining_time: { kind: "unknown", seconds: null, sample_count: 0 },
    }
  )
  assert.deepEqual(await core.resolveReadingSeek([1, 9], 5_000), {
    segment: 1,
    segment_progress_basis_points: 3_333,
    char_offset: 5,
  })
})

test("the shipped web wasm pauses at the sampled media cursor and resumes there", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))
  const loading = await core.reducePlayback(
    await core.createPlaybackState(2),
    { type: "play_requested" }
  )
  const playing = await core.reducePlayback(loading.state, {
    type: "audio_ready",
    sentence: 0,
    generation: 1,
    duration_ms: 2_000,
  })
  const paused = await core.reducePlayback(playing.state, {
    type: "pause_requested",
    audio_position_ms: 701,
  })
  const resumed = await core.reducePlayback(paused.state, {
    type: "play_requested",
  })

  assert.equal(paused.state.status, "paused")
  assert.equal(paused.state.current_position_ms, 701)
  assert.deepEqual(paused.effects, [
    { type: "pause_audio" },
    { type: "persist_progress", sentence: 0, position_ms: 701 },
  ])
  assert.equal(resumed.state.status, "playing")
  assert.deepEqual(resumed.effects[0], {
    type: "play_audio",
    sentence: 0,
    generation: 3,
    offset_ms: 701,
  })
})

test("the production wasm loader reuses one initialized module", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const load = createReadioWasmLoader(binary)

  const first = await load()
  const second = await load()

  assert.equal(first, second)
  assert.equal(typeof first.search_document, "function")
  assert.equal(typeof first.resolve_block_offset, "function")
})

test("the shipped web wasm owns locator recovery and highlight coordinates", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))
  const document = await core.parseHTML(
    "<p>First sentence.</p><p>Second bright sentence.</p>"
  )
  const locator = await core.createTextLocator(document, 23, "forward")
  const restored = await core.restoreTextLocator(locator, document)
  const index = await core.indexDocumentText(document)
  const target = await core.resolveHighlight(document, index, {
    document_revision: document.revision!.revision_digest,
    sentence_index: 1,
    word_index: 1,
  })

  assert.equal(restored.kind, "exact_revision")
  assert.deepEqual(target.sentence_range, [0, 23])
  assert.deepEqual(target.word_range, [7, 13])
})

test("the shipped web wasm returns complete plain-text Reader IR", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))

  const reader = await core.parsePlainTextReader("A < B & C\n\nA < B & C")
  const { document } = reader

  assert.deepEqual(
    document.blocks.map((block) => block.text),
    ["A < B & C", "A < B & C"]
  )
  assert.notEqual(document.blocks[0]?.id, document.blocks[1]?.id)
  assert.deepEqual(
    reader.text_index.sentences.map((sentence) => sentence.text),
    ["A < B & C", "A < B & C"]
  )
  assert.equal(
    sentenceIndexForBlock(reader.text_index, document.blocks[1]!.id),
    1
  )
  assert.equal(sentenceIndexForBlock(reader.text_index, "missing"), null)
})

test("the shipped web wasm returns semantic Markdown Reader IR", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))

  const reader = await core.parseMarkdownReader(
    "# Quiet tools\n\nRead **one thing** well.\n\n> Attention has a shape."
  )

  assert.deepEqual(
    reader.document.blocks.map((block) => [block.kind.type, block.text]),
    [
      ["heading", "Quiet tools"],
      ["paragraph", "Read one thing well."],
      ["quote", "Attention has a shape."],
    ]
  )
  assert.equal(reader.document.blocks[1]?.inline_spans?.[0]?.bold, true)
  assert.equal(reader.navigation[0]?.title, "Quiet tools")
})

test("the shipped web SDK exchanges Loro updates and restores snapshots", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  await init({ module_or_path: binary })
  const sdk = createReadioSdkClient(
    async () => generatedWasm as unknown as ReadioSdkWasmModule
  )
  const desktopChange = await (
    await sdk.newReplica()
  ).upsert("bookmark", "bookmark-1", {
    item_id: "book-1",
    char_offset: 15,
  })
  const web = await sdk.newReplica()

  web.importChange(desktopChange)

  assert.deepEqual(web.state().bookmarks?.["bookmark-1"], {
    item_id: "book-1",
    char_offset: 15,
  })
  assert.ok(web.heads().byteLength > 0)
  const restored = await sdk.newReplica(web.snapshot())
  assert.deepEqual(restored.state(), web.state())
  assert.deepEqual(restored.heads(), web.heads())
})

test("the production web runtime exposes the unified SDK facade", async () => {
  const replica = await webReadioSdk.newReplica()
  const change = replica.upsert("setting", "theme", { value: "dark" })

  assert.ok(change.byteLength > 0)
  assert.deepEqual(replica.state().settings?.theme, { value: "dark" })
})

test("the shipped web SDK persists the shared Rust sync state machine", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  await init({ module_or_path: binary })
  const sdk = createReadioSdkClient(
    async () => generatedWasm as unknown as ReadioSdkWasmModule
  )
  const entity = {
    entityKind: "reading_position" as const,
    entityId: "book-1",
    payload: { char_offset: 42 },
  }
  const local = await sdk.newSyncState()
  local.stageLocal([entity])
  assert.equal(local.outbox().length, 1)

  const restored = await sdk.newSyncState(local.serialize())
  restored.stageLocal([entity])
  assert.equal(restored.outbox().length, 1)

  const remote = await sdk.newSyncState()
  remote.importRemote(restored.outbox())
  assert.equal(remote.outbox().length, 0)
  assert.deepEqual(remote.state().reading_positions?.["book-1"], {
    char_offset: 42,
  })
})

async function minimalEPUB(): Promise<Uint8Array> {
  const archive = new JSZip()
  archive.file("mimetype", "application/epub+zip")
  archive.file(
    "META-INF/container.xml",
    `<?xml version="1.0"?>
    <container xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
      <rootfiles><rootfile full-path="OEBPS/content.opf" /></rootfiles>
    </container>`
  )
  archive.file(
    "OEBPS/content.opf",
    `<?xml version="1.0"?>
    <package xmlns="http://www.idpf.org/2007/opf" version="3.0">
      <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
        <dc:title>Portable EPUB</dc:title>
        <dc:creator>Rust SDK</dc:creator>
      </metadata>
      <manifest>
        <item id="first" href="first.xhtml" media-type="application/xhtml+xml" />
        <item id="notes" href="notes.xhtml" media-type="application/xhtml+xml" />
        <item id="second" href="second.xhtml" media-type="application/xhtml+xml" />
        <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav" />
      </manifest>
      <spine>
        <itemref idref="second" />
        <itemref idref="notes" linear="no" />
        <itemref idref="first" />
      </spine>
    </package>`
  )
  archive.file(
    "OEBPS/first.xhtml",
    "<html><body><h1>First Chapter</h1><p>First body.</p></body></html>"
  )
  archive.file(
    "OEBPS/notes.xhtml",
    "<html><body><h1>Notes</h1><p>Excluded note.</p></body></html>"
  )
  archive.file(
    "OEBPS/second.xhtml",
    "<html><body><h1>Second Chapter</h1><p id=\"body\">Second body.</p></body></html>"
  )
  archive.file(
    "OEBPS/nav.xhtml",
    `<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
      <body><nav epub:type="toc"><ol>
        <li><a href="second.xhtml">Part Two</a><ol>
          <li><a href="second.xhtml#body">Focused body</a></li>
        </ol></li>
        <li><a href="first.xhtml">Part One</a></li>
      </ol></nav></body>
    </html>`
  )
  return archive.generateAsync({ type: "uint8array" })
}

function minimalTextPDF(lines: string[]): Uint8Array {
  const encoder = new TextEncoder()
  const escapedLines = lines.map((line) =>
    line.replaceAll("\\", "\\\\").replaceAll("(", "\\(").replaceAll(")", "\\)")
  )
  const stream = encoder.encode(
    `BT\n/F1 12 Tf\n16 TL\n72 720 Td\n${escapedLines
      .map((line) => `(${line}) Tj\nT*`)
      .join("\n")}\nET\n`
  )
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [5 0 R] /Count 1 >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    "<< /Title (Readable PDF) >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents 6 0 R >>",
    `<< /Length ${stream.byteLength} >>\nstream\n${new TextDecoder().decode(
      stream
    )}\nendstream`,
  ]
  let output = "%PDF-1.4\n"
  const offsets: number[] = []
  for (const [index, object] of objects.entries()) {
    offsets.push(encoder.encode(output).byteLength)
    output += `${index + 1} 0 obj\n${object}\nendobj\n`
  }
  const xref = encoder.encode(output).byteLength
  output += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`
  output += offsets.map((offset) => `${String(offset).padStart(10, "0")} 00000 n \n`).join("")
  output += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R /Info 4 0 R >>\nstartxref\n${xref}\n%%EOF\n`
  return encoder.encode(output)
}
