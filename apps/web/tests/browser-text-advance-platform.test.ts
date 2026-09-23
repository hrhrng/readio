import assert from "node:assert/strict"
import test from "node:test"

import { BrowserTextAdvancePlatform } from "../lib/browser-text-advance-platform.ts"

test("the browser text platform replaces timers and returns only generation facts", async () => {
  const callbacks = new Map<number, () => void>()
  const cleared: number[] = []
  let nextTimer = 1
  const observedGenerations: number[] = []
  const platform = new BrowserTextAdvancePlatform({
    setTimer(callback) {
      const timer = nextTimer++
      callbacks.set(timer, callback)
      return timer
    },
    clearTimer(timer) {
      cleared.push(timer as number)
      callbacks.delete(timer as number)
    },
  })
  platform.connect({
    async advanceDue(generation) {
      observedGenerations.push(generation)
    },
  })

  platform.scheduleAdvance(1, 2_000)
  platform.scheduleAdvance(2, 1_000)
  callbacks.get(2)?.()
  await new Promise<void>((resolve) => setImmediate(resolve))

  assert.deepEqual(cleared, [1])
  assert.deepEqual(observedGenerations, [2])
})
