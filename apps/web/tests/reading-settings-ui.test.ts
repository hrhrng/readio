import assert from "node:assert/strict"
import test from "node:test"
import React from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { JSDOM } from "jsdom"

import * as settingsDialog from "../components/settings-dialog.tsx"
import { LocaleProvider } from "../components/locale-provider.tsx"
import type {
  EffortLevel,
  ReadingMode,
  ReadingPace,
} from "../lib/rust-core.ts"

;(globalThis as typeof globalThis & { React: typeof React }).React = React

type ReadingSectionProps = {
  fontSize: number
  mode: ReadingMode
  effort: EffortLevel
  baseTextRate: number
  pace: ReadingPace
  onFontSizeChange: (size: number) => void
  onModeChange: (mode: ReadingMode) => void
  onEffortChange: (effort: EffortLevel) => void
  onBaseTextRateChange: (rate: number) => void
}

test("Web Reading settings expose three modes and the Rust six-level pace", () => {
  const ReadingSection = Reflect.get(settingsDialog, "ReadingSection") as
    | React.ComponentType<ReadingSectionProps>
    | undefined
  assert.equal(typeof ReadingSection, "function")
  const html = renderToStaticMarkup(
    React.createElement(
      LocaleProvider,
      {
        initialLocale: "en",
        initialPreference: "en",
      },
      React.createElement(ReadingSection!, {
          fontSize: 18,
          mode: "read_aloud",
          effort: "xhigh",
          baseTextRate: 48,
          pace: {
            mode: "read_aloud",
            effort: "xhigh",
            multiplier: 1.25,
            text_characters_per_second: 60,
            audio_playback_rate: 1.25,
            advances_automatically: true,
            uses_audio: true,
          },
          onFontSizeChange() {},
          onModeChange() {},
          onEffortChange() {},
          onBaseTextRateChange() {},
        })
    )
  )
  const document = new JSDOM(html).window.document

  assert.deepEqual(
    Array.from(
      document.querySelectorAll('[aria-label="Reading mode"] button')
    ).map((button) => button.textContent?.trim()),
    ["Manual", "Auto", "Read aloud"]
  )
  assert.equal(
    document
      .querySelector('[aria-label="Reading mode"] button:nth-child(3)')
      ?.getAttribute("aria-pressed"),
    "true"
  )
  assert.deepEqual(
    Array.from(
      document.querySelectorAll('[aria-label="Reading effort"] option')
    ).map((option) => option.getAttribute("value")),
    ["minimal", "low", "medium", "high", "xhigh", "max"]
  )
  assert.match(document.body.textContent ?? "", /60 chars\/s/)
  assert.match(document.body.textContent ?? "", /1\.25× audio/)
  assert.doesNotMatch(document.body.textContent ?? "", /TTS speed/)
})
