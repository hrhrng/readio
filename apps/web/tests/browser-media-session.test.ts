import assert from "node:assert/strict"
import test from "node:test"

import {
  BrowserMediaSessionController,
  type MediaSessionPort,
  type SupportedMediaSessionAction,
} from "../lib/browser-media-session.ts"

class FakeMediaSession implements MediaSessionPort {
  playbackState: MediaSessionPlaybackState = "none"
  metadata: MediaMetadata | null = null
  readonly handlers = new Map<SupportedMediaSessionAction, (() => void) | null>()
  readonly positions: (MediaPositionState | null)[] = []

  setActionHandler(
    action: SupportedMediaSessionAction,
    handler: (() => void) | null
  ): void {
    this.handlers.set(action, handler)
  }

  setPositionState(state?: MediaPositionState): void {
    this.positions.push(state ?? null)
  }
}

test("browser media controls dispatch only through the active Rust player commands", () => {
  const session = new FakeMediaSession()
  const commands: string[] = []
  const controller = new BrowserMediaSessionController(
    session,
    (metadata) => metadata as unknown as MediaMetadata
  )

  controller.connect({
    play: () => commands.push("play"),
    pause: () => commands.push("pause"),
    nextSentence: () => commands.push("next"),
    prevSentence: () => commands.push("previous"),
  })
  session.handlers.get("play")?.()
  session.handlers.get("pause")?.()
  session.handlers.get("nexttrack")?.()
  session.handlers.get("previoustrack")?.()
  session.handlers.get("stop")?.()

  assert.deepEqual(commands, ["play", "pause", "next", "previous", "pause"])
  assert.deepEqual([...session.handlers.keys()], [
    "play",
    "pause",
    "stop",
    "nexttrack",
    "previoustrack",
  ])
})

test("browser media metadata and playback state mirror the projected player state", () => {
  const session = new FakeMediaSession()
  const controller = new BrowserMediaSessionController(
    session,
    (metadata) => metadata as unknown as MediaMetadata
  )
  controller.connect({
    play() {},
    pause() {},
    nextSentence() {},
    prevSentence() {},
  })

  controller.update({
    title: "The Book",
    artist: "Read-aloud",
    artwork: "https://readio.test/cover.jpg",
    isPlaying: true,
  })

  assert.equal(session.playbackState, "playing")
  assert.deepEqual(session.metadata, {
    title: "The Book",
    artist: "Read-aloud",
    album: "Readio",
    artwork: [{ src: "https://readio.test/cover.jpg" }],
  })

  controller.update({
    title: "The Book",
    artist: "Read-aloud",
    artwork: null,
    isPlaying: false,
  })
  assert.equal(session.playbackState, "paused")
})

test("disconnect removes handlers and clears stale system media state", () => {
  const session = new FakeMediaSession()
  const controller = new BrowserMediaSessionController(
    session,
    (metadata) => metadata as unknown as MediaMetadata
  )
  controller.connect({
    play() {},
    pause() {},
    nextSentence() {},
    prevSentence() {},
  })
  controller.update({ title: "Book", artist: "Readio", artwork: null, isPlaying: true })

  controller.disconnect()

  assert.equal(session.playbackState, "none")
  assert.equal(session.metadata, null)
  assert.ok([...session.handlers.values()].every((handler) => handler === null))
})

test("system media position mirrors the sampled Rust audio cursor", () => {
  const session = new FakeMediaSession()
  const controller = new BrowserMediaSessionController(
    session,
    (metadata) => metadata as unknown as MediaMetadata
  )

  controller.update({
    title: "Book",
    artist: "Readio",
    artwork: null,
    isPlaying: true,
    durationSeconds: 120,
    positionSeconds: 121,
    playbackRate: 1.5,
  })
  assert.deepEqual(session.positions.at(-1), {
    duration: 120,
    position: 120,
    playbackRate: 1.5,
  })

  controller.update({
    title: "Book",
    artist: "Readio",
    artwork: null,
    isPlaying: false,
    durationSeconds: null,
    positionSeconds: 0,
    playbackRate: 1,
  })
  assert.equal(session.positions.at(-1), null)
})
