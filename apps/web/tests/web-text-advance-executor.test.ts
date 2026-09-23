import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import test from "node:test"

import * as runtime from "../lib/readio-core-runtime.ts"
import { createRustCoreClient } from "../lib/rust-core.ts"
import { createReadioWasmLoader } from "../lib/readio-core-runtime.ts"

interface TestTextPlatform {
  events: Array<Record<string, unknown>>
  now: number
  monotonicMilliseconds(): number
  scheduleAdvance(generation: number, afterMilliseconds: number): void
  cancelScheduledAdvance(): void
  persistProgress(sentence: number): void
  textAdvanceCompleted(): void
}

type TextAdvanceExecutor = {
  initialize(characterCounts: number[], charactersPerSecond: number): Promise<void>
  start(): Promise<void>
  pause(): Promise<void>
  advanceDue(generation: number): Promise<void>
  state(): {
    status: string
    current_sentence: number
    remaining_ms: number | null
  }
}

type CreateExecutor = (
  platform: TestTextPlatform,
  core?: ReturnType<typeof createRustCoreClient>
) => TextAdvanceExecutor

test("the Web text executor applies Rust pause, resume, and advance effects", async () => {
  const createWebTextAdvanceExecutor = Reflect.get(
    runtime,
    "createWebTextAdvanceExecutor"
  ) as CreateExecutor | undefined
  assert.equal(typeof createWebTextAdvanceExecutor, "function")

  const binary = await readFile(
    new URL("../wasm/readio_wasm_bg.wasm", import.meta.url)
  )
  const core = createRustCoreClient(createReadioWasmLoader(binary))
  const platform: TestTextPlatform = {
    events: [],
    now: 10_000,
    monotonicMilliseconds() {
      return this.now
    },
    scheduleAdvance(generation, afterMilliseconds) {
      this.events.push({ type: "schedule", generation, afterMilliseconds })
    },
    cancelScheduledAdvance() {
      this.events.push({ type: "cancel" })
    },
    persistProgress(sentence) {
      this.events.push({ type: "persist", sentence })
    },
    textAdvanceCompleted() {
      this.events.push({ type: "completed" })
    },
  }
  const executor = createWebTextAdvanceExecutor!(platform, core)

  await executor.initialize([80, 40], 40)
  await executor.start()
  platform.now = 10_500
  await executor.pause()
  platform.now = 20_000
  await executor.start()
  platform.now = 21_500
  await executor.advanceDue(3)

  assert.deepEqual(platform.events, [
    { type: "schedule", generation: 1, afterMilliseconds: 2_000 },
    { type: "cancel" },
    { type: "schedule", generation: 3, afterMilliseconds: 1_500 },
    { type: "persist", sentence: 1 },
    { type: "schedule", generation: 4, afterMilliseconds: 1_000 },
  ])
  assert.equal(executor.state().status, "running")
  assert.equal(executor.state().current_sentence, 1)
  assert.equal(executor.state().remaining_ms, null)
})
