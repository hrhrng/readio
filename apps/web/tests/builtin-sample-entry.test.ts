import assert from "node:assert/strict"
import test from "node:test"
import React from "react"
import { renderToStaticMarkup } from "react-dom/server"

import { BuiltinSampleLink } from "../components/builtin-sample-link.tsx"
import { LocaleProvider } from "../components/locale-provider.tsx"
import { shouldOfferBuiltinSample } from "../lib/builtin-sample-entry.ts"
import { resolvePlayerItemId } from "../lib/player-route.ts"

;(globalThis as typeof globalThis & { React: typeof React }).React = React

test("the empty-library action opens the reserved virtual sample", () => {
  const html = renderToStaticMarkup(
    React.createElement(
      LocaleProvider,
      {
        initialLocale: "en",
        initialPreference: "en",
      },
      React.createElement(BuiltinSampleLink)
    )
  )

  assert.match(html, /href="\/player\/readio%3Asample%3Av1"/)
  assert.match(html, />Open sample</)
  assert.match(html, /ui-button--primary/)
})

test("the player resolves URL-encoded item identities before library lookup", () => {
  assert.equal(
    resolvePlayerItemId("readio%3Asample%3Av1"),
    "readio:sample:v1"
  )
  assert.equal(resolvePlayerItemId("readio:sample:v1"), "readio:sample:v1")
})

test("the virtual sample stays out of populated and filtered library results", () => {
  assert.equal(shouldOfferBuiltinSample({ itemCount: 0 }), true)
  assert.equal(shouldOfferBuiltinSample({ itemCount: 1 }), false)
  assert.equal(
    shouldOfferBuiltinSample({ itemCount: 0, type: "epub" }),
    false
  )
  assert.equal(
    shouldOfferBuiltinSample({ itemCount: 0, progressMin: 1 }),
    false
  )
  assert.equal(
    shouldOfferBuiltinSample({ itemCount: 0, search: "attention" }),
    false
  )
})
