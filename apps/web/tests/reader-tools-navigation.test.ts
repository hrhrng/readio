import assert from "node:assert/strict"
import test from "node:test"
import {
  sentenceBlockCursor,
  sentenceIndexForBlock,
} from "../lib/reader-document.ts"

async function loadNavigation() {
  try {
    return await import("../lib/reader-tools-navigation.ts")
  } catch {
    assert.fail("reader search navigation is not implemented")
  }
}

test("walking past the last loaded hit requests the next page without wrapping", async () => {
  const navigation = await loadNavigation()

  assert.deepEqual(
    navigation.resolveSearchWalk({
      hitCount: 3,
      activeIndex: 2,
      direction: 1,
      nextCursor: "cursor-2",
    }),
    { kind: "load", cursor: "cursor-2", targetIndex: 3 }
  )
})

test("tool navigation resolves blocks through the Rust text index", () => {
  const index = {
    version: 1,
    document_revision: "reader-revision",
    sentences: [
      {
        id: "sentence-0",
        index: 0,
        block_id: "first-block",
        range: [0, 4] as [number, number],
        char_range: [0, 4] as [number, number],
        text: "One.",
        words: [],
      },
      {
        id: "sentence-1",
        index: 1,
        block_id: "first-block",
        range: [5, 9] as [number, number],
        char_range: [5, 9] as [number, number],
        text: "Two.",
        words: [],
      },
      {
        id: "sentence-2",
        index: 2,
        block_id: "second-block",
        range: [0, 6] as [number, number],
        char_range: [10, 16] as [number, number],
        text: "Three.",
        words: [],
      },
    ],
  }

  assert.equal(sentenceIndexForBlock(index, "second-block"), 2)
  assert.equal(sentenceIndexForBlock(index, "missing"), null)
})

test("bookmark creation uses the Rust sentence block cursor", () => {
  const index = {
    version: 1,
    document_revision: "reader-revision",
    sentences: [{
      id: "sentence-4",
      index: 4,
      block_id: "multi-sentence-block",
      range: [18, 29] as [number, number],
      char_range: [118, 129] as [number, number],
      text: "Target text",
      words: [],
    }],
  }

  assert.deepEqual(sentenceBlockCursor(index, 4), {
    blockId: "multi-sentence-block",
    offsetInBlock: 18,
  })
  assert.equal(sentenceBlockCursor(index, 3), null)
})

test("search result walking wraps only when there is no next page", async () => {
  const { resolveSearchWalk } = await loadNavigation()

  assert.deepEqual(
    resolveSearchWalk({
      hitCount: 3,
      activeIndex: 2,
      direction: 1,
      nextCursor: null,
    }),
    { kind: "hit", targetIndex: 0 }
  )
  assert.deepEqual(
    resolveSearchWalk({
      hitCount: 3,
      activeIndex: null,
      direction: -1,
      nextCursor: null,
    }),
    { kind: "hit", targetIndex: 2 }
  )
})

test("Cmd-F and Ctrl-F open book search outside editable controls", async () => {
  const { shouldOpenReaderSearch } = await loadNavigation()

  assert.equal(
    shouldOpenReaderSearch({
      key: "f",
      metaKey: true,
      ctrlKey: false,
      isEditable: false,
    }),
    true
  )
  assert.equal(
    shouldOpenReaderSearch({
      key: "F",
      metaKey: false,
      ctrlKey: true,
      isEditable: false,
    }),
    true
  )
  assert.equal(
    shouldOpenReaderSearch({
      key: "f",
      metaKey: true,
      ctrlKey: false,
      isEditable: true,
    }),
    false
  )
  assert.equal(
    shouldOpenReaderSearch({
      key: "f",
      metaKey: false,
      ctrlKey: false,
      isEditable: false,
    }),
    false
  )
})

test("reader overlays keep one sheet slot active at a time", async () => {
  const navigation = await loadNavigation()
  assert.equal(
    typeof navigation.reduceReaderOverlay,
    "function",
    "reader overlay state reducer is not implemented"
  )

  assert.deepEqual(
    navigation.reduceReaderOverlay(
      { tocOpen: true, activeTool: null },
      { type: "open-tool", tool: "search" }
    ),
    { tocOpen: false, activeTool: "search" }
  )
  assert.deepEqual(
    navigation.reduceReaderOverlay(
      { tocOpen: false, activeTool: "bookmarks" },
      { type: "toggle-toc" }
    ),
    { tocOpen: true, activeTool: null }
  )
  assert.deepEqual(
    navigation.reduceReaderOverlay(
      { tocOpen: true, activeTool: null },
      { type: "toggle-toc" }
    ),
    { tocOpen: false, activeTool: null }
  )
  assert.deepEqual(
    navigation.reduceReaderOverlay(
      { tocOpen: false, activeTool: "search" },
      { type: "close-tools" }
    ),
    { tocOpen: false, activeTool: null }
  )
  assert.deepEqual(
    navigation.reduceReaderOverlay(
      { tocOpen: true, activeTool: "bookmarks" },
      { type: "close-all" }
    ),
    { tocOpen: false, activeTool: null }
  )
})
