import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"
import JSZip from "jszip"

import {
  deleteItem,
  fetchItem,
  fetchItemFile,
  fetchItems,
  importFile,
  importUrl,
  updateChapter,
  updateProgress,
  updateSpeed,
  updateVoice,
} from "../lib/api.ts"
import {
  WebLocalReplicaStore,
  type LocalLibraryContentPersistence,
  type LocalLibraryContentRecord,
  type LocalReplicaPersistence,
  type LocalReplicaRecord,
} from "../lib/local-replica-store.ts"
import {
  createReadioSdkClient,
  type ReadioSdkWasmModule,
} from "../lib/readio-sdk.ts"
import init, * as generatedWasm from "../wasm/readio_wasm.js"
import { WebDiagnosticLog } from "../lib/diagnostics.ts"

class MemoryReplicaPersistence implements LocalReplicaPersistence {
  record: LocalReplicaRecord | null = null

  async load(): Promise<LocalReplicaRecord | null> {
    return this.record === null ? null : structuredClone(this.record)
  }

  async save(record: LocalReplicaRecord): Promise<void> {
    this.record = structuredClone(record)
  }
}

class MemoryContentPersistence implements LocalLibraryContentPersistence {
  records = new Map<string, LocalLibraryContentRecord>()

  async load(itemId: string): Promise<LocalLibraryContentRecord | null> {
    const record = this.records.get(itemId)
    return record === undefined ? null : structuredClone(record)
  }

  async save(record: LocalLibraryContentRecord): Promise<void> {
    this.records.set(record.itemId, structuredClone(record))
  }

  async delete(itemId: string): Promise<void> {
    this.records.delete(itemId)
  }
}

async function localStore() {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  await init({ module_or_path: binary })
  const sdk = createReadioSdkClient(
    async () => generatedWasm as unknown as ReadioSdkWasmModule
  )
  const contentPersistence = new MemoryContentPersistence()
  const store = new WebLocalReplicaStore({
    sdk,
    persistence: new MemoryReplicaPersistence(),
    contentPersistence,
  })
  return { contentPersistence, sdk, store }
}

test("the built-in sample opens through Rust without entering the local library", async () => {
  const { store } = await localStore()
  const sample = await fetchItem(
    "readio:sample:v1",
    store,
    {
      async builtinSample(languageTag: string) {
        assert.equal(languageTag, "zh-Hans-CN")
        return {
          item_id: "readio:sample:v1",
          legacy_item_ids: ["6b4121313cc4a35d"],
          locale: "zh",
          title: "注意力的形状",
          author: "readio 示例",
          source: "readio://sample",
          content_format: "plaintext",
          content: "共享正文",
          markdown_source: "# 共享正文",
          is_builtin_sample: true,
        }
      },
    },
    "zh-Hans-CN"
  )

  assert.deepEqual(sample, {
    id: "readio:sample:v1",
    legacy_item_ids: ["6b4121313cc4a35d"],
    locale: "zh",
    title: "注意力的形状",
    author: "readio 示例",
    content: "共享正文",
    type: "txt",
    progress: 0,
    date: "",
    category: "imported",
    folder_id: "",
    source: "readio://sample",
    file_path: null,
    cover_image: null,
    voice: null,
    speed: null,
    current_chapter: null,
    content_format: "plaintext",
    markdown_source: "# 共享正文",
    is_builtin_sample: true,
  })
  assert.equal((await fetchItems({}, store)).total, 0)
})

test("Web file import, detail, source bytes, listing, and deletion stay local", async () => {
  const { sdk, store } = await localStore()
  const imported = await importFile(
    new File(["Hello, local-first reader."], "notes.txt", {
      type: "text/plain",
    }),
    { title: "Notes", folder_id: "inbox" },
    store,
    sdk
  )

  assert.equal(imported.title, "Notes")
  assert.equal(imported.content, "Hello, local-first reader.")
  assert.equal(imported.type, "txt")
  assert.equal(imported.folder_id, "inbox")

  const originalFetch = globalThis.fetch
  globalThis.fetch = async () => new Response(null, { status: 503 })
  try {
    await updateProgress(imported.id, 37, store)
    await updateVoice(imported.id, "native:ava", store)
    await updateSpeed(imported.id, 1.25, store)
    await updateChapter(imported.id, "chapter-2", store)
  } finally {
    globalThis.fetch = originalFetch
  }
  assert.deepEqual(await fetchItem(imported.id, store), {
    ...imported,
    progress: 37,
    voice: "native:ava",
    speed: 1.25,
    current_chapter: "chapter-2",
  })

  const page = await fetchItems(
    { sort_by: "title", sort_order: "asc" },
    store
  )
  assert.equal(page.total, 1)
  assert.equal(page.items[0]?.id, imported.id)
  assert.deepEqual(
    new Uint8Array(await fetchItemFile(imported.id, store)),
    new TextEncoder().encode("Hello, local-first reader.")
  )

  await deleteItem(imported.id, store)
  assert.equal((await fetchItems({}, store)).total, 0)
  await assert.rejects(() => fetchItem(imported.id, store), /not found/i)
  await assert.rejects(() => fetchItemFile(imported.id, store), /not found/i)
})

test("Web file import reports honest stages and cancellation prevents persistence", async () => {
  const { sdk, store } = await localStore()
  const controller = new AbortController()
  const stages: string[] = []

  await assert.rejects(
    () =>
      importFile(
        new File(["This import must never be committed."], "cancelled.txt", {
          type: "text/plain",
        }),
        {
          signal: controller.signal,
          onProgress(progress) {
            stages.push(progress.stage)
            if (progress.stage === "identifying") controller.abort()
          },
        },
        store,
        sdk
      ),
    (error: unknown) =>
      error instanceof DOMException && error.name === "AbortError"
  )

  assert.deepEqual(stages, ["reading_source", "identifying"])
  assert.equal((await fetchItems({}, store)).total, 0)
})

test("Web URL import passes cancellation to acquisition and leaves no local item", async () => {
  const { sdk, store } = await localStore()
  const controller = new AbortController()
  const originalFetch = globalThis.fetch
  let requestSignal: AbortSignal | null = null
  globalThis.fetch = async (_input, init) => {
    requestSignal = init?.signal ?? null
    if (!requestSignal) throw new Error("acquisition request omitted its AbortSignal")
    return await new Promise<Response>((_resolve, reject) => {
      requestSignal.addEventListener(
        "abort",
        () => reject(new DOMException("Import cancelled", "AbortError")),
        { once: true }
      )
    })
  }

  try {
    const pending = importUrl(
      "https://example.com/slow-article",
      { signal: controller.signal },
      store,
      sdk
    )
    controller.abort()
    await assert.rejects(
      () => pending,
      (error: unknown) =>
        error instanceof DOMException && error.name === "AbortError"
    )
  } finally {
    globalThis.fetch = originalFetch
  }

  assert.equal(requestSignal, controller.signal)
  assert.equal((await fetchItems({}, store)).total, 0)
})

test("Markdown file import preserves its authored format for Rust Reader IR", async () => {
  const { sdk, store } = await localStore()
  const imported = await importFile(
    new File(["# Durable heading\n\n> Durable quote."], "notes.md", {
      type: "text/markdown",
    }),
    {},
    store,
    sdk
  )

  assert.equal(imported.content_format, "markdown")
  assert.equal(imported.content, "# Durable heading\n\n> Durable quote.")
  assert.equal((await fetchItem(imported.id, store)).content_format, "markdown")
})

test("Web file import records correlated import and parsing facts", async () => {
  const { sdk, store } = await localStore()
  const bytes = minimalTextPDF([
    "One shared diagnostic passage.",
    "A second shared diagnostic passage.",
  ])
  const diagnostics = new WebDiagnosticLog({
    now: () => 42,
    createOperationId: () => "op-web-import",
  })

  await importFile(
    new File([bytes], "shared.pdf", {
      type: "application/pdf",
    }),
    { diagnostics },
    store,
    sdk
  )

  assert.deepEqual(diagnostics.snapshot(), [
    {
      timestamp_unix_ms: 42,
      operation_id: "op-web-import",
      component: "web-reader-parser",
      category: "parsing",
      name: "parse_completed",
      fields: {
        document_format: "pdf",
        byte_count: bytes.byteLength,
        block_count: 2,
        chapter_count: 1,
        outcome: "completed",
      },
    },
    {
      timestamp_unix_ms: 42,
      operation_id: "op-web-import",
      component: "web-library",
      category: "import",
      name: "import_completed",
      fields: {
        source_kind: "file",
        document_format: "pdf",
        byte_count: bytes.byteLength,
        duplicate: false,
        outcome: "created",
      },
    },
  ])
})

test("plain text import uses the cross-platform diagnostic format token", async () => {
  const { sdk, store } = await localStore()
  const diagnostics = new WebDiagnosticLog({
    now: () => 43,
    createOperationId: () => "op-web-plain-import",
  })

  await importFile(
    new File(["Private plain text."], "notes.txt", { type: "text/plain" }),
    { diagnostics },
    store,
    sdk
  )

  const event = diagnostics.snapshot().find(({ category }) => category === "import")
  assert.equal(event?.fields.document_format, "plain_text")
  assert.doesNotMatch(JSON.stringify(diagnostics.snapshot()), /Private plain text/)
})

test("reimporting identical file bytes reuses the existing item and its progress", async () => {
  const { sdk, store } = await localStore()
  const original = await importFile(
    new File(["One durable identity."], "original.txt", {
      type: "text/plain",
    }),
    { title: "Original title" },
    store,
    sdk
  )
  await updateProgress(original.id, 63, store)

  const duplicate = await importFile(
    new File(["One durable identity."], "renamed.txt", {
      type: "text/plain",
    }),
    { title: "Replacement title" },
    store,
    sdk
  )

  assert.equal(duplicate.id, original.id)
  assert.equal(duplicate.title, "Original title")
  assert.equal(duplicate.progress, 63)
  assert.equal((await fetchItems({}, store)).total, 1)
})

test("file import persists the Rust ownership record with synced metadata", async () => {
  const { sdk, store } = await localStore()
  const imported = await importFile(
    new File(["Owned bytes."], "metadata.txt", { type: "text/plain" }),
    {},
    store,
    sdk
  )

  const metadata = (await store.state()).library_items?.[imported.id] as
    | Record<string, unknown>
    | undefined
  assert.deepEqual(metadata?.import_record, {
    item_id: imported.id,
    content_identity: {
      algorithm: "blake3-prefix-v1",
      byte_length: 12,
      digest: imported.id,
    },
    ownership: "copy",
    source_key: "browser-file:metadata.txt",
    storage_key: `local:${imported.id}`,
  })
})

test("PDF file import persists the shared Rust publication IR beside the original bytes", async () => {
  const { sdk, store } = await localStore()
  const bytes = minimalTextPDF([
    "One canonical PDF passage.",
    "It is ready without a second parser.",
  ])

  const imported = await importFile(
    new File([bytes], "portable.pdf", { type: "application/pdf" }),
    {},
    store,
    sdk
  )

  assert.equal(imported.content_format, "readio-pdf-v1")
  const publication = JSON.parse(imported.content)
  assert.equal(publication.title, "Readable PDF")
  assert.equal(publication.page_count, 1)
  assert.equal(
    publication.canonical_text,
    "One canonical PDF passage.\nIt is ready without a second parser."
  )
  assert.equal(publication.chapters.length, 1)
  assert.deepEqual(
    publication.chapters[0].document.blocks.map(
      (block: { text: string }) => block.text
    ),
    [
      "One canonical PDF passage.",
      "It is ready without a second parser.",
    ]
  )
  assert.deepEqual(
    publication.chapters[0].block_provenance.map(
      (source: { page_start: number; page_end: number }) => [
        source.page_start,
        source.page_end,
      ]
    ),
    [
      [1, 1],
      [1, 1],
    ]
  )
  assert.deepEqual(
    new Uint8Array(await fetchItemFile(imported.id, store)),
    bytes
  )
})

test("EPUB file import persists the shared spine-filtered publication IR", async () => {
  const { sdk, store } = await localStore()
  const bytes = await minimalEPUB()

  const imported = await importFile(
    new File([bytes], "fallback-name.epub", {
      type: "application/epub+zip",
    }),
    {},
    store,
    sdk
  )
  const publication = JSON.parse(imported.content) as {
    title: string
    author: string
    cover_asset_id: string
    chapters: Array<{ id: string; title: string }>
  }

  assert.equal(imported.title, "Portable EPUB")
  assert.equal(imported.author, "Rust SDK")
  assert.equal((await fetchItem(imported.id, store)).author, "Rust SDK")
  assert.equal(imported.content_format, "readio-epub-v1")
  assert.equal(
    imported.cover_image,
    `readio-epub-asset:${publication.cover_asset_id}`
  )
  assert.equal(publication.title, "Portable EPUB")
  assert.deepEqual(
    publication.chapters.map((chapter) => chapter.id),
    ["second", "first"]
  )
  assert.deepEqual(
    publication.chapters.map((chapter) => chapter.title),
    ["Second Chapter", "First Chapter"]
  )
})

test("forgetting a linked item removes only the library reference", async () => {
  const { contentPersistence, sdk, store } = await localStore()
  await store.saveLibraryContent({
    itemId: "linked-book",
    content: "Content supplied by a linked platform source.",
    fileBytes: null,
    mediaType: "text/plain",
    fileName: "outside.txt",
  })
  await store.upsert([
    {
      entityKind: "library_item",
      entityId: "linked-book",
      payload: {
        id: "linked-book",
        title: "Linked book",
        type: "txt",
        progress: 21,
        date: "2026-08-07T00:00:00.000Z",
        category: "imported",
        folder_id: "f1",
        source: "/outside.txt",
        file_path: "/outside.txt",
        cover_image: null,
        voice: null,
        speed: null,
        current_chapter: null,
        content_format: "plaintext",
        import_record: {
          item_id: "linked-book",
          content_identity: {
            algorithm: "blake3-prefix-v1",
            byte_length: 44,
            digest: "linked-content",
          },
          ownership: "link",
          source_key: "/outside.txt",
          storage_key: "/outside.txt",
        },
      },
    },
  ])

  await deleteItem("linked-book", store, sdk)

  assert.equal(await store.libraryItem("linked-book"), null)
  assert.equal(
    (await contentPersistence.load("linked-book"))?.content,
    "Content supplied by a linked platform source."
  )
})

test("URL acquisition uses Rust parsing and reuses local progress", async () => {
  const { sdk, store } = await localStore()
  const acquired = {
    final_url: "https://example.com/final-article",
    media_type: "text/html",
    html: "<html><body><nav>Menu</nav><main><h1>Rust Article</h1><p>First paragraph.</p><script>steal()</script><p>Second paragraph.</p></main></body></html>",
    // A legacy server item must never become the local-first identity.
    id: "server-random-id",
    title: "Server title",
    content: "Server projection",
    type: "web" as const,
    progress: 0,
    date: "2026-08-06T00:00:00.000Z",
    category: "imported" as const,
    folder_id: "f1",
    source: "https://example.com/article",
    file_path: null,
    cover_image: null,
    voice: null,
    speed: null,
    current_chapter: null,
    content_format: "html",
  }
  const originalFetch = globalThis.fetch
  const acquisitionRequests: string[] = []
  globalThis.fetch = async (input) => {
    acquisitionRequests.push(String(input))
    return Response.json(acquired)
  }
  try {
    const imported = await importUrl(
      "https://example.com/article",
      {},
      store,
      sdk
    )
    assert.notEqual(imported.id, "server-random-id")
    assert.equal(imported.title, "Rust Article")
    assert.equal(
      imported.content,
      "Rust Article\nFirst paragraph.\nSecond paragraph."
    )
    assert.equal(imported.source, "https://example.com/final-article")

    await updateProgress(imported.id, 71, store)
    const duplicate = await importUrl(
      "https://example.com/article",
      {},
      store,
      sdk
    )
    assert.equal(duplicate.id, imported.id)
    assert.equal(duplicate.progress, 71)
    assert.equal((await fetchItems({}, store)).total, 1)
    assert.deepEqual(acquisitionRequests, [
      "/api/cloud/v1/acquire/url",
      "/api/cloud/v1/acquire/url",
    ])
  } finally {
    globalThis.fetch = originalFetch
  }
})

test("URL import records one correlated acquisition and Rust parsing operation", async () => {
  const { sdk, store } = await localStore()
  const html = "<main><h1>Private title</h1><p>Private URL passage.</p></main>"
  const diagnostics = new WebDiagnosticLog({
    now: () => 73,
    createOperationId: () => "op-web-url-import",
  })
  const originalFetch = globalThis.fetch
  globalThis.fetch = async () => Response.json({
    final_url: "https://private.example/final",
    media_type: "text/html",
    html,
  })
  try {
    await importUrl(
      "https://private.example/source",
      { diagnostics },
      store,
      sdk
    )
  } finally {
    globalThis.fetch = originalFetch
  }

  assert.deepEqual(diagnostics.snapshot(), [
    {
      timestamp_unix_ms: 73,
      operation_id: "op-web-url-import",
      component: "web-reader-parser",
      category: "parsing",
      name: "parse_completed",
      fields: {
        document_format: "html",
        byte_count: new TextEncoder().encode(html).byteLength,
        block_count: 2,
        chapter_count: 1,
        outcome: "completed",
      },
    },
    {
      timestamp_unix_ms: 73,
      operation_id: "op-web-url-import",
      component: "web-library",
      category: "import",
      name: "import_completed",
      fields: {
        source_kind: "url",
        document_format: "html",
        byte_count: new TextEncoder().encode(html).byteLength,
        duplicate: false,
        outcome: "created",
      },
    },
  ])
  assert.doesNotMatch(
    JSON.stringify(diagnostics.snapshot()),
    /Private title|Private URL passage|private\.example/
  )
})

test("URL import surfaces the Cloud acquisition recovery message", async () => {
  const originalFetch = globalThis.fetch
  globalThis.fetch = async () => Response.json(
    {
      error: {
        code: "source_too_large",
        message: "The URL source exceeds the import size limit.",
      },
    },
    { status: 413 },
  )
  try {
    await assert.rejects(
      () => importUrl("https://example.com/large"),
      /The URL source exceeds the import size limit\./,
    )
  } finally {
    globalThis.fetch = originalFetch
  }
})

async function minimalEPUB(): Promise<Uint8Array<ArrayBuffer>> {
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
        <item id="cover" href="cover.png" media-type="image/png" properties="cover-image" />
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
    "<html><body><h1>Second Chapter</h1><p>Second body.</p></body></html>"
  )
  archive.file("OEBPS/cover.png", new Uint8Array([137, 80, 78, 71]))
  return new Uint8Array(await archive.generateAsync({ type: "uint8array" }))
}

function minimalTextPDF(lines: string[]): Uint8Array<ArrayBuffer> {
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
