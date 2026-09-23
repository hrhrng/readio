import assert from "node:assert/strict"
import test from "node:test"

import React from "react"
import { renderToStaticMarkup } from "react-dom/server"

import TuiPage from "../app/tui/page.tsx"

// Next supplies the JSX runtime in production. The direct server render keeps
// this test on the real page without booting a second application server.
;(globalThis as typeof globalThis & { React: typeof React }).React = React

test("the TUI surface carries one accessible terminal-native Readio lockup", () => {
  const html = renderToStaticMarkup(React.createElement(TuiPage))
  const visibleText = html.replace(/<[^>]+>/g, "")

  assert.match(html, /role="img"/)
  assert.match(html, /aria-label="Readio"/)
  assert.match(visibleText, /❯_\s*readio/)
})
