import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"
import React from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { JSDOM } from "jsdom"

import { LocaleProvider } from "../components/locale-provider.tsx"
import * as settingsDialog from "../components/settings-dialog.tsx"
import { WebDiagnosticLog, supportBundleFile } from "../lib/diagnostics.ts"
import { createReadioWasmLoader } from "../lib/readio-core-runtime.ts"
import { createRustCoreClient } from "../lib/rust-core.ts"

;(globalThis as typeof globalThis & { React: typeof React }).React = React

test("Web diagnostics uses Rust to export correlated clocks without private content", async () => {
  let now = 1_000
  const log = new WebDiagnosticLog({
    now: () => now++,
    createOperationId: () => "op-web-1",
  })
  const operationId = log.beginOperation()

  log.record({
    operationId,
    component: "web-cloud-tts",
    category: "tts",
    name: "provider_completed",
    fields: {
      provider: "workers-ai",
      trace_id: "trace-42",
      duration_ms: 842,
      book_text: "A private sentence from the current book.",
      authorization: "Bearer private-token",
    },
  })
  log.record({
    operationId,
    component: "web-player",
    category: "playback",
    name: "clock_sample",
    fields: {
      generation: 3,
      sentence_index: 7,
      media_position_ms: 1_250,
      media_duration_ms: 4_000,
    },
  })

  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))

  const bundle = await log.buildSupportBundle({
    core,
    appVersion: "0.2.0-beta.12",
  })
  const exported = JSON.stringify(bundle)

  assert.equal(bundle.platform, "web")
  assert.equal(bundle.events.length, 2)
  assert.ok(bundle.events.every((event) => event.operation_id === operationId))
  assert.equal(bundle.events[1]?.fields.media_position_ms, 1_250)
  assert.equal(bundle.redaction.content_included, false)
  assert.equal(bundle.redaction.secrets_included, false)
  assert.doesNotMatch(exported, /private sentence|private-token|authorization|book_text/i)
})

test("Web diagnostics bounds its raw in-memory ring before Rust export", () => {
  let operation = 0
  const log = new WebDiagnosticLog({
    capacity: 3,
    now: () => operation,
    createOperationId: () => `op-${operation++}`,
  })

  for (let index = 0; index < 5; index += 1) {
    log.record({
      operationId: log.beginOperation(),
      component: "web-sync",
      category: "sync",
      name: "pull_completed",
      fields: { change_count: index },
    })
  }

  assert.deepEqual(
    log.snapshot().map((event) => event.operation_id),
    ["op-2", "op-3", "op-4"]
  )
  log.clear()
  assert.deepEqual(log.snapshot(), [])
})

test("Web settings explains and downloads the redacted support bundle", () => {
  const DiagnosticsSection = Reflect.get(
    settingsDialog,
    "DiagnosticsSection"
  ) as React.ComponentType | undefined
  assert.equal(typeof DiagnosticsSection, "function")

  const html = renderToStaticMarkup(
    React.createElement(
      LocaleProvider,
      { initialLocale: "en", initialPreference: "en" },
      React.createElement(DiagnosticsSection!)
    )
  )
  const document = new JSDOM(html).window.document

  assert.equal(
    document.querySelector("button")?.textContent?.trim(),
    "Download support bundle"
  )
  assert.match(document.body.textContent ?? "", /never includes book text or secrets/i)

  const file = supportBundleFile({
    schema_version: 1,
    generated_at_unix_ms: 0,
    platform: "web",
    app_version: "0.2.0",
    redaction: {
      content_included: false,
      secrets_included: false,
      dropped_field_count: 0,
      dropped_event_count: 0,
    },
    events: [],
  })
  assert.equal(file.filename, "readio-support-1970-01-01T00-00-00-000Z.json")
  assert.match(file.contents, /"content_included": false/)
})
