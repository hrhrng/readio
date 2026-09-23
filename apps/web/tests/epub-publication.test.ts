import assert from "node:assert/strict"
import test from "node:test"

import {
  projectParsedEPUB,
  projectReaderIR,
} from "../lib/epub-publication.ts"
import { projectParsedPDF } from "../lib/pdf-publication.ts"
import { createEpubAssetResolver } from "../lib/epub-asset-resolver.ts"
import type {
  DocumentAsset,
  DocumentBlock,
  ParsedEPUB,
  ReaderChapter,
  ReaderIR,
  ReaderNavigation,
} from "../lib/rust-core.ts"

function readerFixture({
  blocks,
  assets = [],
  chapters,
  sentences,
  navigation = [],
}: {
  blocks: DocumentBlock[]
  assets?: DocumentAsset[]
  chapters: ReaderChapter[]
  sentences: { blockId: string; range: [number, number]; text: string }[]
  navigation?: ReaderNavigation[]
}): ReaderIR {
  return {
    ir_version: 1,
    document: { ir_version: 1, blocks, assets, anchors: [] },
    text_index: {
      version: 1,
      document_revision: "fixture-reader-revision",
      sentences: sentences.map((sentence, index) => ({
        id: `fixture-sentence-${index}`,
        index,
        block_id: sentence.blockId,
        range: sentence.range,
        char_range: sentence.range,
        text: sentence.text,
        words: [],
      })),
    },
    chapters,
    navigation,
  }
}

test("the Web projection preserves Rust EPUB order and semantic block kinds", () => {
  const publication: ParsedEPUB = {
    title: "Portable EPUB",
    author: "Rust SDK",
    chapters: [
      {
        id: "second",
        href: "second.xhtml",
        title: "Second",
        document: {
          ir_version: 1,
          blocks: [
            {
              id: "heading-2",
              kind: { type: "heading", level: 2 },
              text: "Second chapter",
            },
            {
              id: "quote-2",
              kind: { type: "quote" },
              text: "Quoted thought.",
            },
          ],
          assets: [],
        },
      },
      {
        id: "first",
        href: "first.xhtml",
        title: "First",
        document: {
          ir_version: 1,
          blocks: [
            {
              id: "body-1",
              kind: { type: "paragraph" },
              text: "First sentence. Next sentence.",
            },
            {
              id: "code-1",
              kind: { type: "code" },
              text: "let portable = true;",
            },
          ],
          assets: [],
        },
      },
    ],
    reader: readerFixture({
      blocks: [
        {
          id: "heading-2",
          kind: { type: "heading", level: 2 },
          text: "Second chapter",
        },
        {
          id: "quote-2",
          kind: { type: "quote" },
          text: "Quoted thought.",
        },
        {
          id: "body-1",
          kind: { type: "paragraph" },
          text: "First sentence. Next sentence.",
        },
        {
          id: "code-1",
          kind: { type: "code" },
          text: "let portable = true;",
        },
      ],
      chapters: [
        {
          id: "second",
          title: "Second",
          block_range: [0, 2],
          sentence_range: [0, 2],
          char_range: [0, 30],
        },
        {
          id: "first",
          title: "First",
          block_range: [2, 4],
          sentence_range: [2, 4],
          char_range: [31, 82],
        },
      ],
      sentences: [
        { blockId: "heading-2", range: [0, 14], text: "Second chapter" },
        { blockId: "quote-2", range: [0, 15], text: "Quoted thought." },
        { blockId: "body-1", range: [0, 15], text: "First sentence." },
        { blockId: "body-1", range: [16, 30], text: "Next sentence." },
      ],
    }),
  }

  const projected = projectParsedEPUB(publication)

  assert.deepEqual(
    projected.chapters.map((chapter) => chapter.id),
    ["second", "first"]
  )
  assert.deepEqual(
    projected.chapters.flatMap((chapter) =>
      chapter.blocks.map((block) => block.type)
    ),
    ["heading", "blockquote", "paragraph", "code"]
  )
  assert.deepEqual(
    projected.sentences.map((sentence) => [sentence.index, sentence.text]),
    [
      [0, "Second chapter"],
      [1, "Quoted thought."],
      [2, "First sentence."],
      [3, "Next sentence."],
    ]
  )
})

test("the Web GUI advances from Reader IR instead of reparsing chapter text", () => {
  const document = {
    ir_version: 1,
    revision: {
      content_identity: { algorithm: "fixture", byte_length: 1, digest: "source" },
      ir_version: 1,
      parser_version: "fixture-reader-ir-1",
      revision_digest: "reader-revision",
      char_count: 21,
    },
    blocks: [
      {
        id: "reader-block",
        kind: { type: "paragraph" },
        text: "Right one. Right two.",
        inline_spans: [],
      },
    ],
    assets: [],
    anchors: [],
  }
  const publication = {
    title: "Reader IR",
    author: null,
    chapters: [
      {
        id: "chapter",
        href: "chapter.xhtml",
        title: "Chapter",
        document: {
          ir_version: 1,
          blocks: [
            {
              id: "legacy-block",
              kind: { type: "paragraph" },
              text: "WRONG platform text.",
            },
          ],
          assets: [],
        },
      },
    ],
    reader: {
      ir_version: 1,
      document,
      text_index: {
        version: 1,
        document_revision: "reader-revision",
        sentences: [
          {
            id: "sentence-1",
            index: 0,
            block_id: "reader-block",
            range: [0, 10],
            char_range: [0, 10],
            text: "Right one.",
            words: [],
          },
          {
            id: "sentence-2",
            index: 1,
            block_id: "reader-block",
            range: [11, 21],
            char_range: [11, 21],
            text: "Right two.",
            words: [],
          },
        ],
      },
      chapters: [
        {
          id: "chapter",
          title: "Chapter",
          block_range: [0, 1],
          sentence_range: [0, 2],
          char_range: [0, 21],
        },
      ],
      navigation: [
        {
          id: "chapter",
          title: "Chapter",
          depth: 0,
          chapter_id: "chapter",
          block_id: null,
          sentence_index: 0,
        },
      ],
    },
  } as unknown as ParsedEPUB

  const projected = projectParsedEPUB(publication)

  assert.deepEqual(projected.sentences.map((sentence) => sentence.text), [
    "Right one.",
    "Right two.",
  ])
  assert.equal(projected.readerDocument, document)
  assert.equal(projected.textIndex, publication.reader.text_index)
})

test("a single-document GUI projects the same Reader IR without a publication wrapper", () => {
  const reader = readerFixture({
    blocks: [{
      id: "plain-block",
      kind: { type: "paragraph" },
      text: "One. Two.",
    }],
    chapters: [{
      id: "document",
      title: "Document",
      block_range: [0, 1],
      sentence_range: [0, 2],
      char_range: [0, 9],
    }],
    sentences: [
      { blockId: "plain-block", range: [0, 4], text: "One." },
      { blockId: "plain-block", range: [5, 9], text: "Two." },
    ],
  })

  const projected = projectReaderIR(reader)

  assert.equal(projected.reader, reader)
  assert.deepEqual(projected.sentences.map((sentence) => sentence.text), [
    "One.",
    "Two.",
  ])
  assert.equal(projected.chapters[0].blocks[0].type, "paragraph")
})

test("the EPUB GUI adapter accepts complete Reader IR without legacy chapter diagnostics", () => {
  const reader = readerFixture({
    blocks: [{
      id: "epub-reader-block",
      kind: { type: "paragraph" },
      text: "Reader-owned EPUB.",
    }],
    chapters: [{
      id: "chapter",
      title: "Chapter",
      block_range: [0, 1],
      sentence_range: [0, 1],
      char_range: [0, 18],
    }],
    sentences: [{
      blockId: "epub-reader-block",
      range: [0, 18],
      text: "Reader-owned EPUB.",
    }],
  })

  const projected = projectParsedEPUB({ reader } as unknown as ParsedEPUB)

  assert.equal(projected.reader, reader)
  assert.equal(projected.chapters[0]?.blocks[0]?.type, "paragraph")
})

test("the PDF GUI adapter accepts complete Reader IR without page diagnostics", () => {
  const reader = readerFixture({
    blocks: [{
      id: "pdf-reader-block",
      kind: { type: "paragraph" },
      text: "Reader-owned PDF.",
    }],
    chapters: [{
      id: "part",
      title: "Part",
      block_range: [0, 1],
      sentence_range: [0, 1],
      char_range: [0, 17],
    }],
    sentences: [{
      blockId: "pdf-reader-block",
      range: [0, 17],
      text: "Reader-owned PDF.",
    }],
  })

  const projected = projectParsedPDF({ reader } as never)

  assert.equal(projected.reader, reader)
  assert.equal(projected.chapters[0]?.blocks[0]?.type, "paragraph")
})

test("a single-document GUI renders Rust-approved remote image sources", () => {
  const reader = readerFixture({
    blocks: [{
      id: "remote-image",
      kind: { type: "image", asset_id: "image-1" },
      text: "Diagram",
    }],
    assets: [{
      id: "image-1",
      kind: "image",
      source: "https://cdn.example.com/diagram.png",
      alt: "Diagram",
    }],
    chapters: [{
      id: "document",
      title: "Document",
      block_range: [0, 1],
      sentence_range: [0, 0],
      char_range: [0, 7],
    }],
    sentences: [],
  })

  assert.deepEqual(projectReaderIR(reader).chapters[0].blocks, [{
    type: "image",
    src: "https://cdn.example.com/diagram.png",
    assetId: "image-1",
    alt: "Diagram",
  }])
})

test("the Web projection rejects malformed persisted publication JSON", () => {
  assert.throws(
    () => projectParsedEPUB(JSON.parse('{"chapters":null}')),
    /Invalid Readio EPUB publication/
  )
})

test("the Web projection renders Rust scalar emphasis ranges as styled runs", () => {
  const publication = {
    title: "Rich EPUB",
    author: null,
    chapters: [
      {
        id: "rich",
        href: "rich.xhtml",
        title: "Rich",
        document: {
          ir_version: 1,
          blocks: [
            {
              id: "rich-paragraph",
              kind: { type: "paragraph" },
              text: "前 粗🙂 normal 斜中 both.",
              inline_spans: [
                { range: [2, 4], bold: true, italic: false },
                { range: [12, 14], bold: false, italic: true },
                { range: [15, 19], bold: true, italic: true },
              ],
            },
          ],
          assets: [],
        },
      },
    ],
    reader: readerFixture({
      blocks: [
        {
          id: "rich-paragraph",
          kind: { type: "paragraph" },
          text: "前 粗🙂 normal 斜中 both.",
          inline_spans: [
            { range: [2, 4], bold: true, italic: false },
            { range: [12, 14], bold: false, italic: true },
            { range: [15, 19], bold: true, italic: true },
          ],
        },
      ],
      chapters: [{
        id: "rich",
        title: "Rich",
        block_range: [0, 1],
        sentence_range: [0, 1],
        char_range: [0, 20],
      }],
      sentences: [{
        blockId: "rich-paragraph",
        range: [0, 20],
        text: "前 粗🙂 normal 斜中 both.",
      }],
    }),
  } as unknown as ParsedEPUB

  const projected = projectParsedEPUB(publication)
  const block = projected.chapters[0].blocks[0]
  assert.equal(block.type, "paragraph")
  if (block.type !== "paragraph") throw new Error("expected paragraph")

  assert.deepEqual(block.sentences[0].runs, [
    { text: "前 " },
    { text: "粗🙂", bold: true },
    { text: " normal " },
    { text: "斜中", italic: true },
    { text: " " },
    { text: "both", bold: true, italic: true },
    { text: "." },
  ])
})

test("the Web projection preserves EPUB image asset identity without inventing a URL", () => {
  const publication = {
    title: "Illustrated EPUB",
    author: null,
    cover_asset_id: "epub-asset-cover",
    assets: [
      {
        id: "epub-asset-cover",
        href: "ops/cover.png",
        media_type: "image/png",
        byte_length: 12,
      },
      {
        id: "epub-asset-figure",
        href: "ops/figure.png",
        media_type: "image/png",
        byte_length: 14,
      },
    ],
    chapters: [
      {
        id: "illustrated",
        href: "ops/chapter.xhtml",
        title: "Illustrated",
        document: {
          ir_version: 1,
          blocks: [
            {
              id: "image-block",
              kind: { type: "image", asset_id: "epub-asset-figure" },
              text: "A diagram",
            },
          ],
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
    reader: readerFixture({
      blocks: [{
        id: "image-block",
        kind: { type: "image", asset_id: "epub-asset-figure" },
        text: "A diagram",
      }],
      assets: [{
        id: "epub-asset-figure",
        kind: "image",
        source: "ops/figure.png",
        alt: "A diagram",
      }],
      chapters: [{
        id: "illustrated",
        title: "Illustrated",
        block_range: [0, 1],
        sentence_range: [0, 0],
        char_range: [0, 9],
      }],
      sentences: [],
    }),
  } satisfies ParsedEPUB

  assert.deepEqual(projectParsedEPUB(publication).chapters[0].blocks, [
    {
      type: "image",
      src: null,
      assetId: "epub-asset-figure",
      mediaType: "image/png",
      alt: "A diagram",
    },
  ])
})

test("the EPUB asset resolver creates one typed Blob from Rust bytes and caches it", async () => {
  let archiveLoads = 0
  let extractions = 0
  const blobs: Blob[] = []
  const resolver = createEpubAssetResolver({
    assets: [
      {
        id: "epub-asset-figure",
        href: "ops/figure.png",
        media_type: "image/png",
        byte_length: 4,
      },
    ],
    async loadArchive() {
      archiveLoads += 1
      return new Uint8Array([80, 75, 3, 4])
    },
    async extractAsset(archive, assetId) {
      extractions += 1
      assert.deepEqual([...archive], [80, 75, 3, 4])
      assert.equal(assetId, "epub-asset-figure")
      return new Uint8Array([137, 80, 78, 71])
    },
    objectURLs: {
      createObjectURL(blob) {
        blobs.push(blob)
        return "blob:readio-figure"
      },
      revokeObjectURL() {},
    },
  })

  assert.equal(await resolver.resolve("epub-asset-figure"), "blob:readio-figure")
  assert.equal(await resolver.resolve("epub-asset-figure"), "blob:readio-figure")
  assert.equal(archiveLoads, 1)
  assert.equal(extractions, 1)
  assert.equal(blobs[0].type, "image/png")
  assert.deepEqual(
    [...new Uint8Array(await blobs[0].arrayBuffer())],
    [137, 80, 78, 71]
  )
})

test("disposing an EPUB asset resolver revokes every object URL it owns", async () => {
  const revoked: string[] = []
  const resolver = createEpubAssetResolver({
    assets: [
      {
        id: "epub-asset-figure",
        href: "ops/figure.png",
        media_type: "image/png",
        byte_length: 1,
      },
    ],
    loadArchive: async () => new Uint8Array([80, 75]),
    extractAsset: async () => new Uint8Array([1]),
    objectURLs: {
      createObjectURL: () => "blob:owned-by-readio",
      revokeObjectURL: (url) => revoked.push(url),
    },
  })

  await resolver.resolve("epub-asset-figure")
  resolver.dispose()

  assert.deepEqual(revoked, ["blob:owned-by-readio"])
})

test("the Web projection keeps Rust EPUB navigation depth and block targets", () => {
  const publication = {
    title: "Navigable EPUB",
    author: null,
    toc: [
      {
        id: "toc-000000",
        title: "Chapter",
        depth: 0,
        href: "oebps/chapter.xhtml",
        chapter_id: "chapter",
        block_id: null,
      },
      {
        id: "toc-000001",
        title: "Details",
        depth: 1,
        href: "oebps/chapter.xhtml#details",
        chapter_id: "chapter",
        block_id: "details-block",
      },
    ],
    chapters: [
      {
        id: "chapter",
        href: "oebps/chapter.xhtml",
        title: "Chapter",
        document: {
          ir_version: 1,
          blocks: [
            {
              id: "lead-block",
              kind: { type: "paragraph" },
              text: "Lead sentence.",
            },
            {
              id: "details-block",
              kind: { type: "heading", level: 2 },
              text: "Details",
            },
          ],
          assets: [],
        },
      },
    ],
    reader: readerFixture({
      blocks: [
        {
          id: "lead-block",
          kind: { type: "paragraph" },
          text: "Lead sentence.",
        },
        {
          id: "details-block",
          kind: { type: "heading", level: 2 },
          text: "Details",
        },
      ],
      chapters: [{
        id: "chapter",
        title: "Chapter",
        block_range: [0, 2],
        sentence_range: [0, 2],
        char_range: [0, 22],
      }],
      sentences: [
        { blockId: "lead-block", range: [0, 14], text: "Lead sentence." },
        { blockId: "details-block", range: [0, 7], text: "Details" },
      ],
      navigation: [
        {
          id: "toc-000000",
          title: "Chapter",
          depth: 0,
          chapter_id: "chapter",
          block_id: null,
          sentence_index: 0,
        },
        {
          id: "toc-000001",
          title: "Details",
          depth: 1,
          chapter_id: "chapter",
          block_id: "details-block",
          sentence_index: 1,
        },
      ],
    }),
  } as unknown as ParsedEPUB

  assert.deepEqual(projectParsedEPUB(publication).toc, [
    { id: "chapter", title: "Chapter", depth: 0, sentenceIndex: 0 },
    {
      id: "chapter#details-block",
      title: "Details",
      depth: 1,
      sentenceIndex: 1,
    },
  ])
})
