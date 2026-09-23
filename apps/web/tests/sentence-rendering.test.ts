import assert from "node:assert/strict"
import test from "node:test"
import React from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { JSDOM } from "jsdom"

;(globalThis as typeof globalThis & { React: typeof React }).React = React

test("visual sentence boundaries preserve English spacing without changing CJK", async () => {
  const sentences = await import("../lib/sentences.ts")
  assert.equal(
    typeof sentences.visualSentenceSeparator,
    "function",
    "visual sentence separator is not implemented"
  )

  assert.equal(
    sentences.visualSentenceSeparator(
      "Section 1: model ecosystem.",
      "Section 2: toolchain updates."
    ),
    " "
  )
  assert.equal(sentences.visualSentenceSeparator("第一节。", "第二节。"), "")
  assert.equal(sentences.visualSentenceSeparator(null, "Opening sentence."), "")
})

test("Reader IR sentence projection preserves the visual separator", async () => {
  const { renderSentences } = await import(
    "../components/reader/content-blocks.tsx"
  )
  const html = renderToStaticMarkup(
    React.createElement(
      React.Fragment,
      null,
      ...renderSentences(
        [
          {
            index: 0,
            text: "First sentence.",
            runs: [{ text: "First sentence." }],
          },
          {
            index: 1,
            text: "Second sentence.",
            runs: [{ text: "Second sentence." }],
          },
        ],
        -1,
        0,
        () => {}
      )
    )
  )

  assert.equal(
    new JSDOM(html).window.document.body.textContent,
    "First sentence. Second sentence."
  )
})

test("rich reader converts Rust block ranges to sentence-local scalar ranges", async () => {
  const { ReaderHighlightProvider, renderSentences } = await import(
    "../components/reader/content-blocks.tsx"
  )
  const TestReaderHighlightProvider = ReaderHighlightProvider as React.ComponentType<
    Omit<React.ComponentProps<typeof ReaderHighlightProvider>, "children"> & {
      children?: React.ReactNode
    }
  >
  const html = renderToStaticMarkup(
    React.createElement(
      TestReaderHighlightProvider,
      {
        target: {
          sentence_id: "sentence-0",
          sentence_index: 0,
          block_id: "block-0",
          sentence_range: [20, 32],
          word_range: [29, 30],
          sentence_char_range: [100, 112],
          word_char_range: [109, 110],
        },
      },
      renderSentences(
        [
          {
            index: 0,
            text: "🙂 hello, 世界!",
            runs: [{ text: "🙂 hello, 世界!" }],
          },
        ],
        0,
        0.34,
        () => {}
      )
    )
  )
  const document = new JSDOM(html).window.document

  assert.equal(
    Array.from(document.querySelectorAll(".bg-highlight-word-bg"))
      .map((element) => element.textContent)
      .join(""),
    "世"
  )
})

test("Rust scalar word ranges do not paint adjacent punctuation", async () => {
  const { ReaderHighlightProvider, renderSentences } = await import(
    "../components/reader/content-blocks.tsx"
  )
  const TestReaderHighlightProvider = ReaderHighlightProvider as React.ComponentType<
    Omit<React.ComponentProps<typeof ReaderHighlightProvider>, "children"> & {
      children?: React.ReactNode
    }
  >
  const target = {
    sentence_id: "sentence-0",
    sentence_index: 0,
    block_id: "block-0",
    sentence_range: [0, 12] as [number, number],
    word_range: [2, 7] as [number, number],
    sentence_char_range: [0, 12] as [number, number],
    word_char_range: [2, 7] as [number, number],
  }
  const html = renderToStaticMarkup(
    React.createElement(
      TestReaderHighlightProvider,
      { target },
      renderSentences(
        [{
          index: 0,
          text: "🙂 hello, 世界!",
          runs: [{ text: "🙂 hello, 世界!" }],
        }],
        0,
        0,
        () => {}
      )
    )
  )

  const document = new JSDOM(html).window.document
  assert.equal(
    Array.from(document.querySelectorAll(".bg-highlight-word-bg"))
      .map((element) => element.textContent)
      .join(""),
    "hello"
  )
})

test("one Reader IR view renders every chapter and semantic block", async () => {
  const { ReaderDocumentView } = await import(
    "../components/reader/reader-document-view.tsx"
  )
  const html = renderToStaticMarkup(
    React.createElement(ReaderDocumentView, {
      projection: {
        reader: {} as never,
        readerDocument: {} as never,
        textIndex: {} as never,
        sentences: [],
        toc: [],
        chapters: [
          {
            id: "opening",
            title: "Opening",
            blocks: [
              {
                type: "heading",
                level: 1,
                anchorId: "opening-heading",
                sentences: [{
                  index: 0,
                  text: "Opening",
                  runs: [{ text: "Opening" }],
                }],
              },
              {
                type: "paragraph",
                anchorId: "opening-copy",
                sentences: [{
                  index: 1,
                  text: "First paragraph.",
                  runs: [{ text: "First paragraph." }],
                }],
              },
            ],
          },
          {
            id: "details",
            title: "Details",
            blocks: [
              {
                type: "blockquote",
                anchorId: "details-quote",
                sentences: [{
                  index: 2,
                  text: "A quoted detail.",
                  runs: [{ text: "A quoted detail." }],
                }],
              },
              { type: "code", text: "let shared = true;" },
            ],
          },
        ],
      },
      currentSentenceIndex: -1,
      currentWordProgress: 0,
      onSentenceClick: () => {},
    })
  )
  const document = new JSDOM(html).window.document

  assert.equal(document.querySelectorAll("[data-reader-document]").length, 1)
  assert.deepEqual(
    Array.from(document.querySelectorAll("[data-reader-chapter]"))
      .map((chapter) => chapter.getAttribute("data-reader-chapter")),
    ["opening", "details"]
  )
  assert.equal(document.querySelector("h1")?.textContent, "Opening")
  assert.equal(document.querySelector("blockquote")?.textContent, "A quoted detail.")
  assert.equal(document.querySelector("code")?.textContent, "let shared = true;")
  assert.equal(document.querySelectorAll("hr").length, 0)
})
