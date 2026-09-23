import assert from "node:assert/strict"
import test from "node:test"

import {
  transitionReadingMode,
  type ReadingModePlayerPort,
} from "../lib/reading-mode-transition.ts"

function player(
  name: string,
  currentSentenceIndex: number,
  commands: string[]
): ReadingModePlayerPort {
  return {
    currentSentenceIndex,
    pause: () => commands.push(`${name}.pause`),
    playFromSentence: (sentence) => commands.push(`${name}.play:${sentence}`),
    seekToSentence: (sentence) => commands.push(`${name}.seek:${sentence}`),
  }
}

test("switching from read-aloud to auto pauses audio and starts text once at the carried sentence", () => {
  const commands: string[] = []

  transitionReadingMode(
    "read_aloud",
    "auto",
    player("audio", 4, commands),
    player("text", 1, commands)
  )

  assert.deepEqual(commands, ["audio.pause", "text.play:4"])
})

test("switching from auto to read-aloud cancels text and seeks audio without starting it", () => {
  const commands: string[] = []

  transitionReadingMode(
    "auto",
    "read_aloud",
    player("audio", 2, commands),
    player("text", 7, commands)
  )

  assert.deepEqual(commands, ["text.pause", "audio.seek:7"])
})

test("switching auto to manual uses one non-autoplay text seek to cancel the timer", () => {
  const commands: string[] = []

  transitionReadingMode(
    "auto",
    "manual",
    player("audio", 2, commands),
    player("text", 7, commands)
  )

  assert.deepEqual(commands, ["text.seek:7"])
})
