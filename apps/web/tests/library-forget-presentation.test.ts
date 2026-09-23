import assert from "node:assert/strict"
import test from "node:test"

import { resolveLibraryForgetPresentation } from "../lib/library-forget-presentation.ts"

test("linked files are explicitly preserved when a Web item is forgotten", () => {
  assert.deepEqual(
    resolveLibraryForgetPresentation("Linked Notes", "link"),
    {
      title: "Remove from library?",
      description:
        "“Linked Notes” will leave your Readio library. The linked source file stays on your device.",
      actionLabel: "Remove",
      completion: "“Linked Notes” was removed from your library; the source file was kept",
    }
  )
})

test("Readio-owned copies name the bytes that deletion removes", () => {
  assert.deepEqual(
    resolveLibraryForgetPresentation("Owned Book", "copy"),
    {
      title: "Delete Readio copy?",
      description:
        "“Owned Book” and its Readio-managed copy will be deleted. Your original source is unaffected.",
      actionLabel: "Delete",
      completion: "“Owned Book” and its Readio-managed copy were deleted",
    }
  )
})

test("legacy records stay conservative instead of claiming unknown file ownership", () => {
  const presentation = resolveLibraryForgetPresentation("Legacy Book", null)
  assert.equal(presentation.title, "Remove from library?")
  assert.match(presentation.description, /stored Readio copy, if present/)
})

test("Web forget consequences use the active Simplified Chinese catalog", () => {
  assert.deepEqual(
    resolveLibraryForgetPresentation("链接笔记", "link", "zh-Hans"),
    {
      title: "从书库移除？",
      description:
        "“链接笔记”将从 Readio 书库中移除，设备上的链接源文件会保留。",
      actionLabel: "移除",
      completion: "“链接笔记”已从书库移除，源文件已保留",
    }
  )
})
