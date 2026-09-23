import assert from "node:assert/strict"
import test from "node:test"

import { closeWebPlayback } from "../lib/web-playback-lifecycle.ts"

test("closing an active reader persists through Rust before disposing browser audio", async () => {
  const events: string[] = []
  let releasePause!: () => void
  const pauseCompleted = new Promise<void>((resolve) => {
    releasePause = resolve
  })

  const closing = closeWebPlayback(
    {
      state: () => ({ status: "playing" }),
      async pause() {
        events.push("pause-start")
        await pauseCompleted
        events.push("pause-persisted")
      },
    },
    {
      dispose() {
        events.push("disposed")
      },
    }
  )

  await Promise.resolve()
  assert.deepEqual(events, ["pause-start"])
  releasePause()
  await closing

  assert.deepEqual(events, ["pause-start", "pause-persisted", "disposed"])
})

test("closing a stopped reader disposes immediately without inventing a pause", async () => {
  const events: string[] = []

  await closeWebPlayback(
    {
      state: () => ({ status: "paused" }),
      async pause() {
        events.push("pause")
      },
    },
    {
      dispose() {
        events.push("disposed")
      },
    }
  )

  assert.deepEqual(events, ["disposed"])
})

test("browser audio still disposes when a final persistence attempt fails", async () => {
  const events: string[] = []

  await assert.rejects(
    closeWebPlayback(
      {
        state: () => ({ status: "loading" }),
        async pause() {
          events.push("pause")
          throw new Error("storage unavailable")
        },
      },
      {
        dispose() {
          events.push("disposed")
        },
      }
    ),
    /storage unavailable/
  )

  assert.deepEqual(events, ["pause", "disposed"])
})
