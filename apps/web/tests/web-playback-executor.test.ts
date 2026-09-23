import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

import * as runtime from "../lib/readio-core-runtime.ts"
import { createRustCoreClient } from "../lib/rust-core.ts"
import { createReadioWasmLoader } from "../lib/readio-core-runtime.ts"
import { WebDiagnosticLog } from "../lib/diagnostics.ts"

interface TestPlatform {
  events: Array<Record<string, unknown>>
  positionMilliseconds: number
  audioPositionMilliseconds(): number
  fetchAudio(sentence: number, generation: number, priority: "user"): void
  playAudio(sentence: number, generation: number, offsetMilliseconds: number): void
  scheduleTick(generation: number, afterMilliseconds: number): void
  pauseAudio(): void
  persistProgress(sentence: number, positionMilliseconds: number): void
  prefetchAudio(sentences: number[]): void
  playbackFailed(sentence: number, message: string): void
  playbackCompleted(): void
}

type PlaybackExecutor = {
  initialize(sentenceCount: number): Promise<void>
  play(): Promise<void>
  pause(): Promise<void>
  setSleepDeadline(deadlineUnixMilliseconds: number): Promise<void>
  clearSleepDeadline(): Promise<void>
  audioReady(sentence: number, generation: number, durationMilliseconds: number): Promise<void>
  state(): {
    status: string
    current_position_ms: number
    sleep_deadline_unix_ms: number | null
  }
}

type CreateExecutor = (
  platform: TestPlatform,
  core?: ReturnType<typeof createRustCoreClient>,
  diagnostics?: WebDiagnosticLog
) => PlaybackExecutor

test("the Web playback executor applies Rust pause and resume effects to platform audio", async () => {
  const createWebPlaybackExecutor = Reflect.get(
    runtime,
    "createWebPlaybackExecutor"
  ) as CreateExecutor | undefined
  assert.equal(typeof createWebPlaybackExecutor, "function")
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))
  const platform: TestPlatform = {
    events: [],
    positionMilliseconds: 0,
    audioPositionMilliseconds() {
      return this.positionMilliseconds
    },
    fetchAudio(sentence, generation, priority) {
      this.events.push({ type: "fetch", sentence, generation, priority })
    },
    playAudio(sentence, generation, offsetMilliseconds) {
      this.events.push({ type: "play", sentence, generation, offsetMilliseconds })
    },
    scheduleTick(generation, afterMilliseconds) {
      this.events.push({ type: "tick", generation, afterMilliseconds })
    },
    pauseAudio() {
      this.events.push({ type: "pause" })
    },
    persistProgress(sentence, positionMilliseconds) {
      this.events.push({ type: "persist", sentence, positionMilliseconds })
    },
    prefetchAudio(sentences) {
      this.events.push({ type: "prefetch", sentences })
    },
    playbackFailed(sentence, message) {
      this.events.push({ type: "failed", sentence, message })
    },
    playbackCompleted() {
      this.events.push({ type: "completed" })
    },
  }
  const executor = createWebPlaybackExecutor!(platform, core)

  await executor.initialize(2)
  await executor.play()
  await executor.audioReady(0, 1, 2_000)
  await executor.setSleepDeadline(50_000)
  assert.equal(executor.state().sleep_deadline_unix_ms, 50_000)
  await executor.clearSleepDeadline()
  assert.equal(executor.state().sleep_deadline_unix_ms, null)
  platform.positionMilliseconds = 701
  await executor.pause()
  await executor.play()

  assert.deepEqual(platform.events, [
    { type: "fetch", sentence: 0, generation: 1, priority: "user" },
    { type: "prefetch", sentences: [1] },
    { type: "play", sentence: 0, generation: 1, offsetMilliseconds: 0 },
    { type: "tick", generation: 1, afterMilliseconds: 250 },
    { type: "pause" },
    { type: "persist", sentence: 0, positionMilliseconds: 701 },
    { type: "play", sentence: 0, generation: 3, offsetMilliseconds: 701 },
    { type: "tick", generation: 3, afterMilliseconds: 250 },
  ])
  assert.equal(executor.state().status, "playing")
  assert.equal(executor.state().current_position_ms, 701)
})

test("a stalled audio fetch cannot block a user pause", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))
  let releaseFetch!: () => void
  const stalledFetch = new Promise<void>((resolve) => {
    releaseFetch = resolve
  })
  const platform: TestPlatform = {
    events: [],
    positionMilliseconds: 0,
    audioPositionMilliseconds() {
      return this.positionMilliseconds
    },
    fetchAudio(sentence, generation, priority) {
      this.events.push({ type: "fetch", sentence, generation, priority })
      return stalledFetch
    },
    playAudio() {},
    scheduleTick() {},
    pauseAudio() {
      this.events.push({ type: "pause" })
    },
    persistProgress(sentence, positionMilliseconds) {
      this.events.push({ type: "persist", sentence, positionMilliseconds })
    },
    prefetchAudio() {},
    playbackFailed() {},
    playbackCompleted() {},
  }
  const executor = runtime.createWebPlaybackExecutor(platform, core)

  await executor.initialize(2)
  const play = executor.play()
  await new Promise<void>((resolve) => setImmediate(resolve))
  const pause = executor.pause()
  const pausedBeforeFetchSettled = await Promise.race([
    pause.then(() => true),
    new Promise<boolean>((resolve) => setImmediate(() => resolve(false))),
  ])
  releaseFetch()
  await Promise.all([play, pause])

  assert.equal(pausedBeforeFetchSettled, true)
  assert.equal(executor.state().status, "paused")
  assert.deepEqual(platform.events, [
    { type: "fetch", sentence: 0, generation: 1, priority: "user" },
    { type: "pause" },
    { type: "persist", sentence: 0, positionMilliseconds: 0 },
  ])
})

test("Web playback records the real media clock and an explicit no-provider fallback", async () => {
  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))
  const diagnostics = new WebDiagnosticLog({
    now: () => 126,
    createOperationId: () => "op-web-playback",
  })
  const platform: TestPlatform = {
    events: [],
    positionMilliseconds: 375,
    audioPositionMilliseconds() {
      return this.positionMilliseconds
    },
    fetchAudio() {},
    playAudio() {},
    scheduleTick() {},
    pauseAudio() {},
    persistProgress() {},
    prefetchAudio() {},
    playbackFailed() {},
    playbackCompleted() {},
  }
  const executor = runtime.createWebPlaybackExecutor(
    platform,
    core,
    diagnostics
  )

  await executor.initialize(1)
  await executor.play()
  await executor.audioFailed(0, 1, "provider unavailable")

  const events = diagnostics.snapshot()
  assert.ok(events.every((event) => event.operation_id === "op-web-playback"))
  assert.ok(
    events.some(
      (event) =>
        event.category === "playback" &&
        event.name === "clock_sample" &&
        event.fields.generation === 1 &&
        event.fields.sentence_index === 0 &&
        event.fields.media_position_ms === 375 &&
        event.fields.playing === false
    )
  )
  assert.ok(
    events.some(
      (event) =>
        event.category === "fallback" &&
        event.name === "provider_fallback_stopped" &&
        event.fields.from_provider === "cloud" &&
        event.fields.to_provider === "none" &&
        event.fields.outcome === "stopped"
    )
  )
})
