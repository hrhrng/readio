import assert from "node:assert/strict"
import test from "node:test"
import React, { act, useRef, useState } from "react"
import { createRoot } from "react-dom/client"
import { renderToStaticMarkup } from "react-dom/server"
import { JSDOM } from "jsdom"
import DOMPurify from "dompurify"
import { Sheet } from "../components/sheet.tsx"
import { ConfirmDialog } from "../components/confirm-dialog.tsx"
import { LocaleProvider } from "../components/locale-provider.tsx"
import { ContentRouter } from "../components/reader/content-router.tsx"
import { PdfReader } from "../components/reader/pdf-reader.tsx"
import { EpubReader } from "../components/reader/epub-reader.tsx"
import { WebDiagnosticLog } from "../lib/diagnostics.ts"
import type { DocumentBlock, ReaderIR } from "../lib/rust-core.ts"

;(globalThis as typeof globalThis & { React: typeof React }).React = React
const TestSheet = Sheet as React.ComponentType<
  Omit<React.ComponentProps<typeof Sheet>, "children"> & {
    children?: React.ReactNode
  }
>

function withEnglishLocale(children: React.ReactNode) {
  return React.createElement(
    LocaleProvider,
    { initialLocale: "en", initialPreference: "en" },
    children
  )
}

function singleChapterReader(
  id: string,
  title: string,
  blocks: DocumentBlock[]
): ReaderIR {
  let charOffset = 0
  return {
    ir_version: 1,
    document: { ir_version: 1, blocks, assets: [], anchors: [] },
    text_index: {
      version: 1,
      document_revision: "reader-surface-fixture",
      sentences: blocks.map((block, index) => {
        const length = Array.from(block.text).length
        const sentence = {
          id: `surface-sentence-${index}`,
          index,
          block_id: block.id,
          range: [0, length] as [number, number],
          char_range: [charOffset, charOffset + length] as [number, number],
          text: block.text,
          words: [],
        }
        charOffset += length + 1
        return sentence
      }),
    },
    chapters: [{
      id,
      title,
      block_range: [0, blocks.length],
      sentence_range: [0, blocks.length],
      char_range: [0, Math.max(0, charOffset - 1)],
    }],
    navigation: [{
      id,
      title,
      depth: 0,
      chapter_id: id,
      block_id: null,
      sentence_index: blocks.length > 0 ? 0 : null,
    }],
  }
}

test("persisted Rust EPUB IR renders without running a second archive parser", () => {
  const blocks = [
    {
      id: "heading-1",
      kind: { type: "heading", level: 1 },
      text: "Shared chapter",
    },
    {
      id: "paragraph-1",
      kind: { type: "paragraph" },
      text: "Rendered from Rust IR.",
    },
  ] satisfies DocumentBlock[]
  const content = JSON.stringify({
    title: "Portable EPUB",
    author: "Rust SDK",
    chapters: [
      {
        id: "chapter-1",
        href: "chapter.xhtml",
        title: "Chapter one",
        document: {
          ir_version: 1,
          blocks,
          assets: [],
        },
      },
    ],
    reader: singleChapterReader("chapter-1", "Chapter one", blocks),
  })

  const html = renderToStaticMarkup(
    React.createElement(ContentRouter, {
      item: {
        id: "book-1",
        title: "Portable EPUB",
        content,
        type: "epub",
        progress: 0,
        date: "2026-08-07T00:00:00.000Z",
        category: "imported",
        folder_id: "f1",
        source: "book.epub",
        file_path: null,
        cover_image: null,
        voice: null,
        speed: null,
        current_chapter: null,
        content_format: "readio-epub-v1",
      },
      onSentencesExtracted: () => {},
      currentSentenceIndex: -1,
      currentWordProgress: 0,
      onSentenceClick: () => {},
      onChaptersExtracted: () => {},
    })
  )

  const rendered = new JSDOM(html).window.document
  assert.equal(rendered.querySelector("h1")?.textContent, "Shared chapter")
  assert.equal(
    rendered.querySelector("p")?.textContent,
    "Rendered from Rust IR."
  )
  assert.equal(rendered.querySelector('[aria-label="Loading EPUB"]'), null)
})

test("legacy plaintext PDF prefers retained source bytes and Rust reparsing", () => {
  const html = renderToStaticMarkup(
    React.createElement(ContentRouter, {
      item: {
        id: "legacy-pdf-router",
        title: "Legacy PDF",
        content: "Flattened legacy text",
        type: "pdf",
        progress: 0,
        date: "2026-08-07T00:00:00.000Z",
        category: "imported",
        folder_id: "f1",
        source: "legacy.pdf",
        file_path: "local:legacy-pdf-router",
        cover_image: null,
        voice: null,
        speed: null,
        current_chapter: null,
        content_format: "plaintext",
      },
      onSentencesExtracted: () => {},
      currentSentenceIndex: -1,
      currentWordProgress: 0,
      onSentenceClick: () => {},
      onChaptersExtracted: () => {},
    })
  )
  const rendered = new JSDOM(html).window.document

  assert.ok(rendered.querySelector('[aria-label="Loading PDF"]'))
  assert.equal(rendered.body.textContent?.includes("Flattened legacy text"), false)
})

test("persisted Rust PDF IR renders chapters without running PDF.js again", () => {
  const blocks = [
    {
      id: "pdf-heading-1",
      kind: { type: "heading", level: 2 },
      text: "Chapter one",
    },
    {
      id: "pdf-paragraph-1",
      kind: { type: "paragraph" },
      text: "Canonical PDF sentence.",
    },
  ] satisfies DocumentBlock[]
  const content = JSON.stringify({
    title: "Portable PDF",
    author: "Rust SDK",
    canonical_text: "Chapter one\nCanonical PDF sentence.",
    page_count: 1,
    chapters: [
      {
        title: "Chapter one",
        page_start: 1,
        page_end: 1,
        document: {
          ir_version: 1,
          blocks,
          assets: [],
        },
        block_provenance: [
          { block_id: "pdf-heading-1", page_start: 1, page_end: 1 },
          { block_id: "pdf-paragraph-1", page_start: 1, page_end: 1 },
        ],
      },
    ],
    reader: singleChapterReader(
      "pdf-chapter-1-p1",
      "Chapter one",
      blocks
    ),
  })
  const html = renderToStaticMarkup(
    React.createElement(ContentRouter, {
      item: {
        id: "pdf-1",
        title: "Portable PDF",
        content,
        type: "pdf",
        progress: 0,
        date: "2026-08-07T00:00:00.000Z",
        category: "imported",
        folder_id: "f1",
        source: "book.pdf",
        file_path: null,
        cover_image: null,
        voice: null,
        speed: null,
        current_chapter: null,
        content_format: "readio-pdf-v1",
      },
      onSentencesExtracted: () => {},
      currentSentenceIndex: -1,
      currentWordProgress: 0,
      onSentenceClick: () => {},
      onChaptersExtracted: () => {},
    })
  )
  const rendered = new JSDOM(html).window.document

  assert.equal(rendered.querySelector("h2")?.textContent, "Chapter one")
  assert.equal(rendered.querySelector("p")?.textContent, "Canonical PDF sentence.")
  assert.equal(rendered.querySelector('[aria-label="Loading PDF"]'), null)
})

test("legacy PDF records reparse original bytes through Rust instead of PDF.js", async () => {
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const root = createRoot(dom.window.document.querySelector("#root")!)
  let loadedItem = ""

  try {
    await act(async () => {
      root.render(
        React.createElement(PdfReader, {
          itemId: "legacy-pdf",
          onSentencesExtracted: () => {},
          currentSentenceIndex: -1,
          currentWordProgress: 0,
          onSentenceClick: () => {},
          compatibilityIO: {
            async parseItem(itemId: string) {
              loadedItem = itemId
              const blocks = [
                {
                  id: "legacy-block",
                  kind: { type: "paragraph" as const },
                  text: "Rust compatibility path.",
                },
              ]
              return {
                title: "Legacy",
                author: null,
                canonical_text: "Rust compatibility path.",
                page_count: 1,
                chapters: [
                  {
                    title: null,
                    page_start: 1,
                    page_end: 1,
                    document: {
                      ir_version: 1,
                      blocks,
                      assets: [],
                    },
                    block_provenance: [
                      {
                        block_id: "legacy-block",
                        page_start: 1,
                        page_end: 1,
                      },
                    ],
                  },
                ],
                reader: singleChapterReader(
                  "pdf-chapter-1-p1",
                  "",
                  blocks
                ),
              }
            },
          },
        })
      )
      await Promise.resolve()
      await Promise.resolve()
    })

    assert.equal(loadedItem, "legacy-pdf")
    assert.equal(
      dom.window.document.querySelector("p")?.textContent,
      "Rust compatibility path."
    )
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("legacy EPUB records reparse original bytes through Rust instead of epub.js", async () => {
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const root = createRoot(dom.window.document.querySelector("#root")!)
  let loadedItem = ""

  try {
    await act(async () => {
      root.render(
        React.createElement(EpubReader, {
          itemId: "legacy-epub",
          onSentencesExtracted: () => {},
          currentSentenceIndex: -1,
          currentWordProgress: 0,
          onSentenceClick: () => {},
          compatibilityIO: {
            async parseItem(itemId: string) {
              loadedItem = itemId
              const blocks = [
                {
                  id: "legacy-epub-block",
                  kind: { type: "paragraph" as const },
                  text: "Rust EPUB compatibility path.",
                },
              ]
              return {
                title: "Legacy EPUB",
                author: null,
                chapters: [
                  {
                    id: "legacy-chapter",
                    href: "chapter.xhtml",
                    title: "Legacy chapter",
                    document: {
                      ir_version: 1,
                      blocks,
                      assets: [],
                    },
                  },
                ],
                reader: singleChapterReader(
                  "legacy-chapter",
                  "Legacy chapter",
                  blocks
                ),
              }
            },
          },
        })
      )
      await Promise.resolve()
      await Promise.resolve()
    })

    assert.equal(loadedItem, "legacy-epub")
    assert.equal(
      dom.window.document.querySelector("p")?.textContent,
      "Rust EPUB compatibility path."
    )
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("an EPUB image resolves its stable Rust asset ID before rendering", async () => {
  const { BlockRenderer } = await import(
    "../components/reader/content-blocks.tsx"
  )
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const root = createRoot(dom.window.document.querySelector("#root")!)

  try {
    await act(async () => {
      root.render(
        React.createElement(BlockRenderer, {
          block: {
            type: "image",
            src: null,
            assetId: "epub-asset-figure",
            mediaType: "image/png",
            alt: "A diagram",
          },
          currentSentenceIndex: -1,
          currentWordProgress: 0,
          onSentenceClick: () => {},
          resolveImageAsset: async (assetId: string) => {
            assert.equal(assetId, "epub-asset-figure")
            return "blob:readio-figure"
          },
        })
      )
      await Promise.resolve()
    })

    const image = dom.window.document.querySelector("img")
    assert.equal(image?.getAttribute("src"), "blob:readio-figure")
    assert.equal(image?.getAttribute("alt"), "A diagram")
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("the shared EPUB reader loads archive bytes lazily and revokes image URLs", async () => {
  const { SharedEpubReader } = await import(
    "../components/reader/shared-epub-reader.tsx"
  )
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const revoked: string[] = []
  let archiveLoads = 0
  const root = createRoot(dom.window.document.querySelector("#root")!)
  const imageBlocks = [{
    id: "image-block",
    kind: { type: "image", asset_id: "epub-asset-figure" } as const,
    text: "A diagram",
  }]
  const reader = singleChapterReader("chapter", "Chapter", imageBlocks)
  reader.document.assets = [{
    id: "epub-asset-figure",
    kind: "image",
    source: "ops/figure.png",
    alt: "A diagram",
  }]
  const content = JSON.stringify({
    title: "Illustrated",
    author: null,
    cover_asset_id: null,
    assets: [
      {
        id: "epub-asset-figure",
        href: "ops/figure.png",
        media_type: "image/png",
        byte_length: 4,
      },
    ],
    chapters: [
      {
        id: "chapter",
        href: "ops/chapter.xhtml",
        title: "Chapter",
        document: {
          ir_version: 1,
          blocks: imageBlocks,
          assets: [
            {
              id: "epub-asset-figure",
              kind: "image",
              source: "ops/figure.png",
              alt: "A diagram",
            },
          ],
        },
      },
    ],
    reader,
  })

  try {
    await act(async () => {
      root.render(
        React.createElement(SharedEpubReader, {
          itemId: "book-1",
          content,
          onSentencesExtracted: () => {},
          currentSentenceIndex: -1,
          currentWordProgress: 0,
          onSentenceClick: () => {},
          assetIO: {
            async loadArchive(itemId: string) {
              archiveLoads += 1
              assert.equal(itemId, "book-1")
              return new Uint8Array([80, 75, 3, 4])
            },
            async extractAsset(archive: Uint8Array, assetId: string) {
              assert.deepEqual([...archive], [80, 75, 3, 4])
              assert.equal(assetId, "epub-asset-figure")
              return new Uint8Array([137, 80, 78, 71])
            },
            objectURLs: {
              createObjectURL: () => "blob:illustrated",
              revokeObjectURL: (url: string) => revoked.push(url),
            },
          },
        })
      )
      await Promise.resolve()
      await Promise.resolve()
    })

    assert.equal(archiveLoads, 1)
    assert.equal(
      dom.window.document.querySelector("img")?.getAttribute("src"),
      "blob:illustrated"
    )
  } finally {
    act(() => root.unmount())
    assert.deepEqual(revoked, ["blob:illustrated"])
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("the shared EPUB reader publishes its Rust Reader IR to the player", async () => {
  const { SharedEpubReader } = await import(
    "../components/reader/shared-epub-reader.tsx"
  )
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const root = createRoot(dom.window.document.querySelector("#root")!)
  const blocks = [{
    id: "reader-block",
    kind: { type: "paragraph" as const },
    text: "Reader-owned sentence.",
  }]
  const reader = singleChapterReader("chapter", "Chapter", blocks)
  let published: ReaderIR | null = null

  try {
    await act(async () => {
      root.render(
        React.createElement(SharedEpubReader, {
          itemId: "reader-ir-book",
          content: JSON.stringify({
            title: "Reader IR",
            author: null,
            chapters: [{
              id: "chapter",
              href: "chapter.xhtml",
              title: "Chapter",
              document: { ir_version: 1, blocks, assets: [] },
            }],
            reader,
          }),
          onSentencesExtracted: () => {},
          onReaderIRExtracted: (value: ReaderIR) => {
            published = value
          },
          currentSentenceIndex: -1,
          currentWordProgress: 0,
          onSentenceClick: () => {},
        })
      )
      await Promise.resolve()
    })

    assert.deepEqual(published, reader)
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("the shared EPUB reader republishes every replacement Reader IR", async () => {
  const { SharedEpubReader } = await import(
    "../components/reader/shared-epub-reader.tsx"
  )
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const root = createRoot(dom.window.document.querySelector("#root")!)
  const publishedTexts: string[] = []
  const onReaderIRExtracted = (reader: ReaderIR) => {
    publishedTexts.push(reader.text_index.sentences[0]?.text ?? "")
  }
  const renderReader = (text: string) => {
    const blocks = [{
      id: `block-${text}`,
      kind: { type: "paragraph" as const },
      text,
    }]
    const reader = singleChapterReader("chapter", "Chapter", blocks)
    return React.createElement(SharedEpubReader, {
      itemId: "replaceable-book",
      content: JSON.stringify({
        title: "Replaceable",
        author: null,
        chapters: [{
          id: "chapter",
          href: "chapter.xhtml",
          title: "Chapter",
          document: { ir_version: 1, blocks, assets: [] },
        }],
        reader,
      }),
      onSentencesExtracted: () => {},
      onReaderIRExtracted,
      currentSentenceIndex: -1,
      currentWordProgress: 0,
      onSentenceClick: () => {},
    })
  }

  try {
    await act(async () => {
      root.render(renderReader("First IR."))
      await Promise.resolve()
    })
    await act(async () => {
      root.render(renderReader("Second IR."))
      await Promise.resolve()
    })

    assert.deepEqual(publishedTexts, ["First IR.", "Second IR."])
    assert.equal(dom.window.document.querySelector("p")?.textContent, "Second IR.")
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("the shared PDF reader publishes its Rust Reader IR to the player", async () => {
  const { SharedPdfReader } = await import(
    "../components/reader/shared-pdf-reader.tsx"
  )
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const root = createRoot(dom.window.document.querySelector("#root")!)
  const blocks = [{
    id: "pdf-reader-block",
    kind: { type: "paragraph" as const },
    text: "PDF reader sentence.",
  }]
  const reader = singleChapterReader("pdf-chapter-1-p1", "Part", blocks)
  let published: ReaderIR | null = null

  try {
    await act(async () => {
      root.render(
        React.createElement(SharedPdfReader, {
          content: JSON.stringify({
            title: "Reader PDF",
            author: null,
            canonical_text: "PDF reader sentence.",
            page_count: 1,
            chapters: [{
              title: "Part",
              page_start: 1,
              page_end: 1,
              document: { ir_version: 1, blocks, assets: [] },
              block_provenance: [{
                block_id: "pdf-reader-block",
                page_start: 1,
                page_end: 1,
              }],
            }],
            reader,
          }),
          onSentencesExtracted: () => {},
          onReaderIRExtracted: (value: ReaderIR) => {
            published = value
          },
          currentSentenceIndex: -1,
          currentWordProgress: 0,
          onSentenceClick: () => {},
        })
      )
      await Promise.resolve()
    })

    assert.deepEqual(published, reader)
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("the content router forwards publication Reader IR without rebuilding it", async () => {
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const root = createRoot(dom.window.document.querySelector("#root")!)
  const blocks = [{
    id: "routed-block",
    kind: { type: "paragraph" as const },
    text: "Routed by Reader IR.",
  }]
  const reader = singleChapterReader("chapter", "Chapter", blocks)
  let published: ReaderIR | null = null

  try {
    await act(async () => {
      root.render(
        React.createElement(ContentRouter, {
          item: {
            id: "routed-book",
            title: "Routed book",
            content: JSON.stringify({
              title: "Routed book",
              author: null,
              chapters: [{
                id: "chapter",
                href: "chapter.xhtml",
                title: "Chapter",
                document: { ir_version: 1, blocks, assets: [] },
              }],
              reader,
            }),
            type: "epub",
            progress: 0,
            date: "2026-08-08T00:00:00.000Z",
            category: "imported",
            folder_id: "f1",
            source: "book.epub",
            file_path: null,
            cover_image: null,
            voice: null,
            speed: null,
            current_chapter: null,
            content_format: "readio-epub-v1",
          },
          onSentencesExtracted: () => {},
          onReaderIRExtracted: (value: ReaderIR) => {
            published = value
          },
          currentSentenceIndex: -1,
          currentWordProgress: 0,
          onSentenceClick: () => {},
        })
      )
      await Promise.resolve()
    })

    assert.deepEqual(published, reader)
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("the document reader renders plain text only after Rust returns Reader IR", async () => {
  const { SharedDocumentReader } = await import(
    "../components/reader/shared-document-reader.tsx"
  )
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const root = createRoot(dom.window.document.querySelector("#root")!)
  const blocks = [{
    id: "rust-text-block",
    kind: { type: "paragraph" as const },
    text: "First. Second.",
  }]
  const reader = singleChapterReader("document", "Document", blocks)
  reader.text_index.sentences = [
    {
      id: "first",
      index: 0,
      block_id: "rust-text-block",
      range: [0, 6],
      char_range: [0, 6],
      text: "First.",
      words: [],
    },
    {
      id: "second",
      index: 1,
      block_id: "rust-text-block",
      range: [7, 14],
      char_range: [7, 14],
      text: "Second.",
      words: [],
    },
  ]
  reader.chapters[0].sentence_range = [0, 2]
  let parsedInput = ""
  let published: ReaderIR | null = null
  const diagnostics = new WebDiagnosticLog({
    now: () => 91,
    createOperationId: () => "op-web-reader-parse",
  })

  try {
    await act(async () => {
      root.render(
        React.createElement(SharedDocumentReader, {
          content: "PLATFORM MUST NOT SPLIT THIS",
          format: "text",
          diagnostics,
          readerIO: {
            async parseHTML() {
              throw new Error("wrong parser")
            },
            async parseMarkdown() {
              throw new Error("wrong parser")
            },
            async parsePlainText(input: string) {
              parsedInput = input
              return reader
            },
          },
          onSentencesExtracted: () => {},
          onReaderIRExtracted: (value: ReaderIR) => {
            published = value
          },
          currentSentenceIndex: -1,
          currentWordProgress: 0,
          onSentenceClick: () => {},
        })
      )
      await Promise.resolve()
      await Promise.resolve()
    })

    assert.equal(parsedInput, "PLATFORM MUST NOT SPLIT THIS")
    assert.equal(dom.window.document.querySelector("p")?.textContent, "First. Second.")
    assert.equal(published, reader)
    assert.deepEqual(diagnostics.snapshot(), [
      {
        timestamp_unix_ms: 91,
        operation_id: "op-web-reader-parse",
        component: "web-reader-parser",
        category: "parsing",
        name: "parse_completed",
        fields: {
          document_format: "plain_text",
          byte_count: 28,
          block_count: 1,
          chapter_count: 1,
          outcome: "completed",
        },
      },
    ])
    assert.doesNotMatch(
      JSON.stringify(diagnostics.snapshot()),
      /PLATFORM MUST NOT SPLIT THIS/
    )
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("the document reader renders HTML only after Rust returns Reader IR", async () => {
  const { SharedDocumentReader } = await import(
    "../components/reader/shared-document-reader.tsx"
  )
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const root = createRoot(dom.window.document.querySelector("#root")!)
  const blocks = [{
    id: "rust-html-heading",
    kind: { type: "heading" as const, level: 2 },
    text: "Rust heading",
  }]
  const reader = singleChapterReader("document", "Rust heading", blocks)
  let parsedInput = ""

  try {
    await act(async () => {
      root.render(
        React.createElement(SharedDocumentReader, {
          content: "<h2>PLATFORM MUST NOT PARSE THIS</h2>",
          format: "html",
          readerIO: {
            async parseHTML(input: string) {
              parsedInput = input
              return reader
            },
            async parseMarkdown() {
              throw new Error("wrong parser")
            },
            async parsePlainText() {
              throw new Error("wrong parser")
            },
          },
          onSentencesExtracted: () => {},
          currentSentenceIndex: -1,
          currentWordProgress: 0,
          onSentenceClick: () => {},
        })
      )
      await Promise.resolve()
      await Promise.resolve()
    })

    assert.equal(parsedInput, "<h2>PLATFORM MUST NOT PARSE THIS</h2>")
    assert.equal(dom.window.document.querySelector("h2")?.textContent, "Rust heading")
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("the content router sends text through the Rust document reader", async () => {
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const root = createRoot(dom.window.document.querySelector("#root")!)
  const blocks = [{
    id: "router-text-block",
    kind: { type: "paragraph" as const },
    text: "Rust-routed text.",
  }]
  const reader = singleChapterReader("document", "Document", blocks)
  let published: ReaderIR | null = null

  try {
    await act(async () => {
      root.render(
        React.createElement(ContentRouter, {
          item: {
            id: "text-book",
            title: "Text book",
            content: "WRONG browser sentence.",
            type: "txt",
            progress: 0,
            date: "2026-08-08T00:00:00.000Z",
            category: "imported",
            folder_id: "f1",
            source: "book.txt",
            file_path: null,
            cover_image: null,
            voice: null,
            speed: null,
            current_chapter: null,
            content_format: "plaintext",
          },
          documentReaderIO: {
            async parseHTML() {
              throw new Error("wrong parser")
            },
            async parseMarkdown() {
              throw new Error("wrong parser")
            },
            async parsePlainText() {
              return reader
            },
          },
          onSentencesExtracted: () => {},
          onReaderIRExtracted: (value: ReaderIR) => {
            published = value
          },
          currentSentenceIndex: -1,
          currentWordProgress: 0,
          onSentenceClick: () => {},
        })
      )
      await Promise.resolve()
      await Promise.resolve()
    })

    assert.equal(dom.window.document.querySelector("p")?.textContent, "Rust-routed text.")
    assert.equal(published, reader)
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("the content router sends authored Markdown through the Rust document reader", async () => {
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const root = createRoot(dom.window.document.querySelector("#root")!)
  const blocks = [{
    id: "router-markdown-heading",
    kind: { type: "heading" as const, level: 1 },
    text: "Rust-routed Markdown",
  }]
  const reader = singleChapterReader("document", "Rust-routed Markdown", blocks)
  let parsedInput = ""
  const diagnostics = new WebDiagnosticLog({
    now: () => 92,
    createOperationId: () => "op-web-markdown-parse",
  })

  try {
    await act(async () => {
      root.render(
        React.createElement(ContentRouter, {
          item: {
            id: "readio:sample:v1",
            title: "Sample",
            content: "Rust-routed Markdown",
            markdown_source: "# Rust-routed Markdown",
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
            is_builtin_sample: true,
          },
          documentReaderIO: {
            async parseHTML() {
              throw new Error("wrong parser")
            },
            async parseMarkdown(input: string) {
              parsedInput = input
              return reader
            },
            async parsePlainText() {
              throw new Error("wrong parser")
            },
          },
          diagnostics,
          onSentencesExtracted: () => {},
          currentSentenceIndex: -1,
          currentWordProgress: 0,
          onSentenceClick: () => {},
        })
      )
      await Promise.resolve()
      await Promise.resolve()
    })

    assert.equal(parsedInput, "# Rust-routed Markdown")
    assert.equal(
      dom.window.document.querySelector("h1")?.textContent,
      "Rust-routed Markdown"
    )
    assert.deepEqual(diagnostics.snapshot(), [
      {
        timestamp_unix_ms: 92,
        operation_id: "op-web-markdown-parse",
        component: "web-reader-parser",
        category: "parsing",
        name: "parse_completed",
        fields: {
          document_format: "markdown",
          byte_count: 22,
          block_count: 1,
          chapter_count: 1,
          outcome: "completed",
        },
      },
    ])
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("the content router sends HTML through the Rust document reader", async () => {
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    DOMParser: dom.window.DOMParser,
    IS_REACT_ACT_ENVIRONMENT: true,
  })
  const root = createRoot(dom.window.document.querySelector("#root")!)
  const purifier = DOMPurify as unknown as {
    sanitize?: (html: string) => string
  }
  const previousSanitize = purifier.sanitize
  purifier.sanitize = (html) => html
  const blocks = [{
    id: "router-html-block",
    kind: { type: "paragraph" as const },
    text: "Rust-routed HTML.",
  }]
  const reader = singleChapterReader("document", "Document", blocks)
  let published: ReaderIR | null = null

  try {
    await act(async () => {
      root.render(
        React.createElement(ContentRouter, {
          item: {
            id: "html-book",
            title: "HTML book",
            content: "<p>WRONG browser sentence.</p>",
            type: "web",
            progress: 0,
            date: "2026-08-08T00:00:00.000Z",
            category: "imported",
            folder_id: "f1",
            source: "https://example.com",
            file_path: null,
            cover_image: null,
            voice: null,
            speed: null,
            current_chapter: null,
            content_format: "html",
          },
          documentReaderIO: {
            async parseHTML() {
              return reader
            },
            async parseMarkdown() {
              throw new Error("wrong parser")
            },
            async parsePlainText() {
              throw new Error("wrong parser")
            },
          },
          onSentencesExtracted: () => {},
          onReaderIRExtracted: (value: ReaderIR) => {
            published = value
          },
          currentSentenceIndex: -1,
          currentWordProgress: 0,
          onSentenceClick: () => {},
        })
      )
      await Promise.resolve()
      await Promise.resolve()
    })

    assert.equal(dom.window.document.querySelector("p")?.textContent, "Rust-routed HTML.")
    assert.equal(published, reader)
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "DOMParser")
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
    if (previousSanitize) purifier.sanitize = previousSanitize
    else Reflect.deleteProperty(purifier, "sanitize")
  }
})

test("the reader player owns a dedicated frame slot outside the document scroll", async () => {
  const { ReaderPlayerSlot } = await import(
    "../components/reader/player-bar.tsx"
  )

  assert.equal(typeof ReaderPlayerSlot, "function")
  const html = renderToStaticMarkup(
    React.createElement(
      ReaderPlayerSlot,
      null,
      React.createElement("div", {
        className: "reader-player",
        "aria-label": "Reading controls",
      })
    )
  )
  const document = new JSDOM(html).window.document
  const slot = document.querySelector(".reader-player-slot")

  assert.equal(slot?.children.length, 1)
  assert.equal(
    slot?.firstElementChild?.getAttribute("aria-label"),
    "Reading controls"
  )
})

test("reader chrome exposes distinct search and bookmark tool slots", async () => {
  const topBar = await import("../components/reader/top-bar.tsx")

  assert.equal(
    typeof topBar.ReaderToolActions,
    "function",
    "ReaderToolActions must own the reader tool buttons"
  )
  const html = renderToStaticMarkup(
    withEnglishLocale(
      React.createElement(topBar.ReaderToolActions, {
        activeTool: "search",
        tocOpen: false,
        onOpenTool: () => {},
        onToggleToc: () => {},
        onToggleSettings: () => {},
      })
    )
  )
  const document = new JSDOM(html).window.document
  assert.equal(
    document.querySelector('[aria-label="Find in book"]')?.getAttribute(
      "aria-pressed"
    ),
    "true"
  )
  assert.equal(
    document.querySelector('[aria-label="Open bookmarks"]')?.getAttribute(
      "aria-pressed"
    ),
    "false"
  )
})

test("sheet traps keyboard focus and returns it to the opening control", () => {
  const dom = new JSDOM("<!doctype html><html><body><div id='root'></div></body></html>", {
    url: "http://localhost",
  })
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
    Event: globalThis.Event,
    MouseEvent: globalThis.MouseEvent,
    KeyboardEvent: globalThis.KeyboardEvent,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    Event: dom.window.Event,
    MouseEvent: dom.window.MouseEvent,
    KeyboardEvent: dom.window.KeyboardEvent,
    IS_REACT_ACT_ENVIRONMENT: true,
  })

  function Harness() {
    const [open, setOpen] = useState(false)
    const [confirmOpen, setConfirmOpen] = useState(false)
    return React.createElement(
      React.Fragment,
      null,
      React.createElement(
        "button",
        { id: "trigger", onClick: () => setOpen(true) },
        "Open"
      ),
      React.createElement(
        TestSheet,
        {
          open,
          title: "Reading tools",
          onClose: () => setOpen(false),
        },
        React.createElement(
          React.Fragment,
          null,
          React.createElement(
            "button",
            { id: "delete-request", onClick: () => setConfirmOpen(true) },
            "Delete"
          ),
          React.createElement("button", { id: "sheet-last" }, "Last action")
        )
      ),
      React.createElement(ConfirmDialog, {
        open: confirmOpen,
        title: "Delete bookmark?",
        destructive: true,
        onConfirm: () => setConfirmOpen(false),
        onCancel: () => setConfirmOpen(false),
      })
    )
  }

  const root = createRoot(dom.window.document.querySelector("#root")!)
  try {
    act(() => root.render(withEnglishLocale(React.createElement(Harness))))
    const trigger = dom.window.document.querySelector<HTMLButtonElement>("#trigger")!
    trigger.focus()
    act(() => {
      trigger.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true }))
    })

    const close = dom.window.document.querySelector<HTMLButtonElement>(
      '[aria-label="Close panel"]'
    )!
    const last = dom.window.document.querySelector<HTMLButtonElement>("#sheet-last")!
    assert.equal(
      dom.window.document.activeElement?.getAttribute("aria-label"),
      "Close panel"
    )

    close.focus()
    close.dispatchEvent(
      new dom.window.KeyboardEvent("keydown", {
        key: "Tab",
        shiftKey: true,
        bubbles: true,
      })
    )
    assert.equal(dom.window.document.activeElement?.id, "sheet-last")

    last.focus()
    last.dispatchEvent(
      new dom.window.KeyboardEvent("keydown", { key: "Tab", bubbles: true })
    )
    assert.equal(
      dom.window.document.activeElement?.getAttribute("aria-label"),
      "Close panel"
    )

    const deleteRequest = dom.window.document.querySelector<HTMLButtonElement>(
      "#delete-request"
    )!
    deleteRequest.focus()
    act(() => {
      deleteRequest.dispatchEvent(
        new dom.window.MouseEvent("click", { bubbles: true })
      )
    })
    assert.ok(dom.window.document.querySelector('[role="alertdialog"]'))
    act(() => {
      dom.window.document.dispatchEvent(
        new dom.window.KeyboardEvent("keydown", {
          key: "Escape",
          bubbles: true,
        })
      )
    })
    assert.equal(dom.window.document.querySelector('[role="alertdialog"]'), null)
    assert.ok(dom.window.document.querySelector(".ui-sheet"))
    assert.equal(
      dom.window.document.activeElement?.id,
      "delete-request",
      "closing the nested alert must return focus to its trigger"
    )

    act(() => {
      close.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true }))
    })
    assert.equal(dom.window.document.activeElement?.id, "trigger")
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("confirm dialog restores focus when its transient menu source unmounts", () => {
  const dom = new JSDOM("<!doctype html><html><body><div id='root'></div></body></html>", {
    url: "http://localhost",
  })
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
    Event: globalThis.Event,
    MouseEvent: globalThis.MouseEvent,
    KeyboardEvent: globalThis.KeyboardEvent,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    Event: dom.window.Event,
    MouseEvent: dom.window.MouseEvent,
    KeyboardEvent: dom.window.KeyboardEvent,
    IS_REACT_ACT_ENVIRONMENT: true,
  })

  function Harness() {
    const triggerRef = useRef<HTMLButtonElement>(null)
    const [menuOpen, setMenuOpen] = useState(true)
    const [confirmOpen, setConfirmOpen] = useState(false)
    return React.createElement(
      React.Fragment,
      null,
      React.createElement(
        "button",
        { id: "menu-trigger", ref: triggerRef },
        "More"
      ),
      menuOpen
        ? React.createElement(
            "button",
            {
              id: "delete-menu-item",
              onClick: () => {
                setMenuOpen(false)
                setConfirmOpen(true)
              },
            },
            "Delete"
          )
        : null,
      React.createElement(ConfirmDialog, {
        open: confirmOpen,
        title: "Delete item?",
        returnFocusRef: triggerRef,
        onConfirm: () => setConfirmOpen(false),
        onCancel: () => setConfirmOpen(false),
      })
    )
  }

  const root = createRoot(dom.window.document.querySelector("#root")!)
  try {
    act(() => root.render(withEnglishLocale(React.createElement(Harness))))
    const deleteItem = dom.window.document.querySelector<HTMLButtonElement>(
      "#delete-menu-item"
    )!
    deleteItem.focus()
    act(() => {
      deleteItem.dispatchEvent(
        new dom.window.MouseEvent("click", { bubbles: true })
      )
    })
    const cancel = Array.from(
      dom.window.document.querySelectorAll<HTMLButtonElement>("button")
    ).find((button) => button.textContent === "Cancel")!
    act(() => {
      cancel.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true }))
    })

    assert.equal(dom.window.document.activeElement?.id, "menu-trigger")
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("the reader TOC moves the authoritative cursor to its Rust target", async () => {
  const { TOCPanel } = await import("../components/reader/toc-panel.tsx")
  const dom = new JSDOM("<!doctype html><html><body><div id='root'></div></body></html>", {
    url: "http://localhost",
  })
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
    Event: globalThis.Event,
    MouseEvent: globalThis.MouseEvent,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    Event: dom.window.Event,
    MouseEvent: dom.window.MouseEvent,
    IS_REACT_ACT_ENVIRONMENT: true,
  })

  function Harness() {
    const [cursor, setCursor] = useState(0)
    return React.createElement(
      React.Fragment,
      null,
      React.createElement("output", { id: "reader-cursor" }, String(cursor)),
      React.createElement(TOCPanel, {
        items: [
          { id: "chapter", title: "Chapter", depth: 0, sentenceIndex: 0 },
          {
            id: "chapter#details",
            title: "Details",
            depth: 1,
            sentenceIndex: 2,
          },
        ],
        currentSentenceIndex: cursor,
        onNavigate: setCursor,
      })
    )
  }

  const root = createRoot(dom.window.document.querySelector("#root")!)
  try {
    act(() => root.render(withEnglishLocale(React.createElement(Harness))))
    const details = Array.from(
      dom.window.document.querySelectorAll<HTMLButtonElement>("button")
    ).find((button) => button.textContent === "Details")!
    act(() => {
      details.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true }))
    })

    assert.equal(
      dom.window.document.querySelector("#reader-cursor")?.textContent,
      "2"
    )
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})

test("reader chapter controls walk top-level targets and disable both edges", async () => {
  const { TOCPanel } = await import("../components/reader/toc-panel.tsx")
  const dom = new JSDOM("<!doctype html><html><body><div id='root'></div></body></html>", {
    url: "http://localhost",
  })
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
    Event: globalThis.Event,
    MouseEvent: globalThis.MouseEvent,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    Event: dom.window.Event,
    MouseEvent: dom.window.MouseEvent,
    IS_REACT_ACT_ENVIRONMENT: true,
  })

  function Harness() {
    const [cursor, setCursor] = useState(4)
    return React.createElement(
      React.Fragment,
      null,
      React.createElement("output", { id: "chapter-cursor" }, String(cursor)),
      React.createElement(TOCPanel, {
        items: [
          { id: "one", title: "One", depth: 0, sentenceIndex: 0 },
          { id: "one#detail", title: "Detail", depth: 1, sentenceIndex: 2 },
          { id: "two", title: "Two", depth: 0, sentenceIndex: 4 },
          { id: "three", title: "Three", depth: 0, sentenceIndex: 6 },
        ],
        currentSentenceIndex: cursor,
        onNavigate: setCursor,
      })
    )
  }

  const root = createRoot(dom.window.document.querySelector("#root")!)
  try {
    act(() => root.render(withEnglishLocale(React.createElement(Harness))))
    const previousChapter = dom.window.document.querySelector<HTMLButtonElement>(
      '[aria-label="Previous chapter"]'
    )!
    const nextChapter = dom.window.document.querySelector<HTMLButtonElement>(
      '[aria-label="Next chapter"]'
    )!
    assert.equal(previousChapter.disabled, false)
    assert.equal(nextChapter.disabled, false)

    act(() => {
      nextChapter.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true }))
    })
    assert.equal(dom.window.document.querySelector("#chapter-cursor")?.textContent, "6")
    assert.equal(nextChapter.disabled, true)

    act(() => {
      previousChapter.dispatchEvent(
        new dom.window.MouseEvent("click", { bubbles: true })
      )
    })
    assert.equal(dom.window.document.querySelector("#chapter-cursor")?.textContent, "4")
    act(() => {
      previousChapter.dispatchEvent(
        new dom.window.MouseEvent("click", { bubbles: true })
      )
    })
    assert.equal(dom.window.document.querySelector("#chapter-cursor")?.textContent, "0")
    assert.equal(previousChapter.disabled, true)
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
})
