import assert from "node:assert/strict"
import test from "node:test"
import React, { act } from "react"
import { createRoot } from "react-dom/client"
import { renderToStaticMarkup } from "react-dom/server"
import { JSDOM } from "jsdom"
import { AppRouterContext } from "next/dist/shared/lib/app-router-context.shared-runtime"
import { BookCover } from "../components/book-card"
import { BookList } from "../components/book-list"
import { LocaleProvider } from "../components/locale-provider"
import { ToastProvider } from "../components/toast-provider"
import type { LibraryItemSummary } from "../lib/types"

;(globalThis as typeof globalThis & { React: typeof React }).React = React

const items: LibraryItemSummary[] = [
  {
    id: "the-left-hand-of-darkness",
    title: "The Left Hand of Darkness",
    type: "epub",
    progress: 42,
    date: "2026-08-06",
    category: "imported",
    folder_id: "",
    source: "Local file",
    file_path: null,
    cover_image: null,
    voice: null,
    speed: null,
    content_format: null,
  },
  {
    id: "a-room-of-ones-own",
    title: "A Room of One's Own",
    type: "pdf",
    progress: 0,
    date: "2026-08-05",
    category: "imported",
    folder_id: "",
    source: "Local file",
    file_path: null,
    cover_image: null,
    voice: null,
    speed: null,
    content_format: null,
  },
]

const testRouter = {
  back() {},
  forward() {},
  refresh() {},
  hmrRefresh() {},
  push() {},
  replace() {},
  prefetch: async () => {},
}

const TestToastProvider = ToastProvider as React.ComponentType<{
  children?: React.ReactNode
}>

function renderBookList(layout: "shelf" | "list" = "shelf") {
  return new JSDOM(
    renderToStaticMarkup(
      React.createElement(
        AppRouterContext.Provider,
        { value: testRouter },
        React.createElement(
          LocaleProvider,
          { initialLocale: "en", initialPreference: "en" },
          React.createElement(
            TestToastProvider,
            null,
            React.createElement(BookList, { items, layout })
          )
        )
      )
    )
  ).window.document
}

test("library items are presented as an accessible cover shelf", () => {
  const document = renderBookList()
  const shelf = document.querySelector("section.library-shelf")
  const grid = shelf?.querySelector('[role="list"]')

  assert.equal(shelf?.getAttribute("aria-label"), "Library shelf")
  assert.equal(grid?.className, "library-shelf-grid")
  assert.deepEqual(
    Array.from(grid?.querySelectorAll(':scope > [role="listitem"]') ?? []).map(
      (card) => card.className
    ),
    ["book-tile", "book-tile"]
  )
  assert.deepEqual(
    Array.from(grid?.querySelectorAll(".book-title") ?? []).map(
      (title) => title.textContent
    ),
    items.map((item) => item.title)
  )
})

test("the alternate library view is a book list, not a data table", () => {
  const document = renderBookList("list")
  const list = document.querySelector("section.library-rows")
  const rows = list?.querySelector('[role="list"]')

  assert.equal(list?.getAttribute("aria-label"), "Library list")
  assert.equal(rows?.className, "library-rows-list")
  assert.equal(document.querySelector("table"), null)
  assert.deepEqual(
    Array.from(rows?.querySelectorAll(':scope > [role="listitem"]') ?? []).map(
      (row) => row.className
    ),
    ["book-row", "book-row"]
  )
})

test("a local EPUB cover resolves through Rust and releases its Blob URL", async () => {
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
  const root = createRoot(dom.window.document.querySelector("#root")!)
  const item: LibraryItemSummary = {
    ...items[0],
    cover_image: "readio-epub-asset:epub-asset-cover",
  }

  try {
    await act(async () => {
      root.render(
        React.createElement(BookCover, {
          item,
          assetIO: {
            async loadArchive(itemId: string) {
              assert.equal(itemId, item.id)
              return new Uint8Array([80, 75, 3, 4])
            },
            async extractAsset(archive: Uint8Array, assetId: string) {
              assert.deepEqual([...archive], [80, 75, 3, 4])
              assert.equal(assetId, "epub-asset-cover")
              return new Uint8Array([137, 80, 78, 71])
            },
            objectURLs: {
              createObjectURL: () => "blob:book-cover",
              revokeObjectURL: (url: string) => revoked.push(url),
            },
          },
        })
      )
      await new Promise((resolve) => setTimeout(resolve, 0))
    })

    assert.equal(
      dom.window.document.querySelector("img")?.getAttribute("src"),
      "blob:book-cover"
    )
  } finally {
    act(() => root.unmount())
    dom.window.close()
    Object.assign(globalThis, previous)
    Reflect.deleteProperty(globalThis, "IS_REACT_ACT_ENVIRONMENT")
  }
  assert.deepEqual(revoked, ["blob:book-cover"])
})
