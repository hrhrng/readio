import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

import * as settingsUtils from "../lib/settings-utils.ts"
import { createRustCoreClient, type RustCoreClient } from "../lib/rust-core.ts"
import { createReadioWasmLoader } from "../lib/readio-core-runtime.ts"

type ResolveStoredReadingPace = (
  settings: Record<string, string>,
  core?: RustCoreClient
) => ReturnType<RustCoreClient["resolveReadingPace"]>
type ResolveInitialPlaybackRate = (
  itemSpeed: number | null,
  settings: Record<string, string>,
  core?: RustCoreClient
) => Promise<number>

test("stored Web reading choices resolve through the shared Rust pace model", async () => {
  const resolveStoredReadingPace = Reflect.get(
    settingsUtils,
    "resolveStoredReadingPace"
  ) as ResolveStoredReadingPace | undefined
  assert.equal(typeof resolveStoredReadingPace, "function")
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))

  assert.deepEqual(
    await resolveStoredReadingPace!(
      {
        reading_mode: "read_aloud",
        reading_base_cps: "48",
        reading_effort: "xhigh",
        reading_effort_xhigh: "1.25",
      },
      core
    ),
    {
      mode: "read_aloud",
      effort: "xhigh",
      multiplier: 1.25,
      text_characters_per_second: 60,
      audio_playback_rate: 1.25,
      advances_automatically: true,
      uses_audio: true,
    }
  )
})

test("invalid stored Web reading choices fall back to Rust defaults", async () => {
  const resolveStoredReadingPace = Reflect.get(
    settingsUtils,
    "resolveStoredReadingPace"
  ) as ResolveStoredReadingPace | undefined
  assert.equal(typeof resolveStoredReadingPace, "function")
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))

  assert.deepEqual(
    await resolveStoredReadingPace!(
      {
        reading_mode: "voice-ish",
        reading_base_cps: "not-a-number",
        reading_effort: "sideways",
      },
      core
    ),
    {
      mode: "manual",
      effort: "high",
      multiplier: 1,
      text_characters_per_second: 46,
      audio_playback_rate: 1,
      advances_automatically: false,
      uses_audio: false,
    }
  )
})

test("reader playback uses a book override or the Rust effort audio rate", async () => {
  const resolveInitialPlaybackRate = Reflect.get(
    settingsUtils,
    "resolveInitialPlaybackRate"
  ) as ResolveInitialPlaybackRate | undefined
  assert.equal(typeof resolveInitialPlaybackRate, "function")
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))
  const settings = {
    reading_effort: "medium",
    reading_base_cps: "50",
  }

  assert.equal(await resolveInitialPlaybackRate!(1.8, settings, core), 1.8)
  assert.equal(await resolveInitialPlaybackRate!(null, settings, core), 1.5)
})
