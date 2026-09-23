import assert from "node:assert/strict"
import test from "node:test"

import { presentCloudTTSFailure } from "../lib/tts-error-presentation.ts"

test("Web TTS failures name the provider, sentence, reason, and recovery", () => {
  assert.equal(
    presentCloudTTSFailure(2, "voice model is unavailable"),
    "Cloud TTS failed for sentence 3: voice model is unavailable. Retry or choose another voice."
  )
})

test("Web TTS failures do not duplicate punctuation from provider reasons", () => {
  assert.equal(
    presentCloudTTSFailure(0, "authentication expired."),
    "Cloud TTS failed for sentence 1: authentication expired. Retry or choose another voice."
  )
})
