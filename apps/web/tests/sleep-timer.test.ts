import assert from "node:assert/strict"
import test from "node:test"
import React from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { JSDOM } from "jsdom"

import { SleepTimerControl } from "../components/reader/sleep-timer-control.tsx"
import { LocaleProvider } from "../components/locale-provider.tsx"
import {
  formatSleepTimerRemaining,
  sleepDeadlineFromMinutes,
} from "../lib/sleep-timer.ts"

;(globalThis as typeof globalThis & { React: typeof React }).React = React

test("sleep timer derives one wall-clock deadline and reports real rounded-up minutes", () => {
  assert.equal(sleepDeadlineFromMinutes(30, 1_000), 1_801_000)
  assert.equal(formatSleepTimerRemaining(1_801_000, 1_000), "30m left")
  assert.equal(formatSleepTimerRemaining(1_801_000, 1_800_001), "1m left")
  assert.equal(formatSleepTimerRemaining(null, 1_000), "Off")
})

test("read-aloud sleep control exposes off and bounded preset choices", () => {
  const html = renderToStaticMarkup(
    React.createElement(
      LocaleProvider,
      {
        initialLocale: "en",
        initialPreference: "en",
      },
      React.createElement(SleepTimerControl, {
        deadlineUnixMilliseconds: 1_801_000,
        nowUnixMilliseconds: 1_000,
        onChange() {},
      })
    )
  )
  const document = new JSDOM(html).window.document
  const select = document.querySelector('select[aria-label="Sleep timer"]')

  assert.ok(select)
  assert.equal(select?.querySelector('option[value="active"]')?.textContent, "30m left")
  assert.deepEqual(
    Array.from(select?.querySelectorAll("option") ?? []).map((option) => option.value),
    ["active", "off", "15", "30", "60"]
  )
})
