import assert from "node:assert/strict"
import test from "node:test"

test("resuming one sentence reuses its media element at the exact Rust offset", async () => {
  const modulePath = `../lib/${"browser-playback-platform"}.ts`
  const platformModule = await import(modulePath).catch(() => ({}))
  const BrowserPlaybackPlatform = Reflect.get(
    platformModule,
    "BrowserPlaybackPlatform"
  ) as
    | (new (options: Record<string, unknown>) => {
        fetchAudio(
          sentence: number,
          generation: number,
          priority: "user"
        ): Promise<{ durationMilliseconds: number } | null | void>
        playAudio(
          sentence: number,
          generation: number,
          offsetMilliseconds: number
        ): Promise<void>
        pauseAudio(): void
        refreshPlaybackRate(): void
      })
    | undefined
  assert.equal(typeof BrowserPlaybackPlatform, "function")

  const created: Array<{
    currentTime: number
    duration: number
    playbackRate: number
    paused: boolean
    onloadedmetadata: (() => void) | null
    ontimeupdate: (() => void) | null
    onended: (() => void) | null
    onerror: (() => void) | null
    play(): Promise<void>
    pause(): void
  }> = []
  let playbackRate = 1.5
  const platform = new BrowserPlaybackPlatform!({
    loadAudio: async () => ({
      url: "blob:sentence-0",
      durationMilliseconds: 2_000,
    }),
    createAudio: () => {
      const audio = {
        currentTime: 0,
        duration: 2,
        playbackRate: 1,
        paused: true,
        onloadedmetadata: null,
        ontimeupdate: null,
        onended: null,
        onerror: null,
        async play() {
          this.paused = false
        },
        pause() {
          this.paused = true
        },
      }
      created.push(audio)
      return audio
    },
    playbackRate: () => playbackRate,
  })

  await platform.fetchAudio(0, 1, "user")
  await platform.playAudio(0, 1, 0)
  playbackRate = 1.75
  platform.refreshPlaybackRate()
  created[0].currentTime = 0.701
  platform.pauseAudio()
  await platform.playAudio(0, 3, 701)

  assert.equal(created.length, 1)
  assert.equal(created[0].currentTime, 0.701)
  assert.equal(created[0].playbackRate, 1.75)
  assert.equal(created[0].paused, false)
})

test("audio readiness waits for real media metadata when transport has no duration", async () => {
  const { BrowserPlaybackPlatform } = await import(
    "../lib/browser-playback-platform.ts"
  )
  const audio = {
    currentTime: 0,
    duration: Number.NaN,
    playbackRate: 1,
    onloadedmetadata: null as (() => void) | null,
    ontimeupdate: null as (() => void) | null,
    onended: null as (() => void) | null,
    onerror: null as (() => void) | null,
    async play() {},
    pause() {},
  }
  let created = 0
  const platform = new BrowserPlaybackPlatform({
    loadAudio: async () => ({ url: "blob:metadata-only" }),
    createAudio: () => {
      created += 1
      return audio
    },
    playbackRate: () => 1,
  })

  const pending = platform.fetchAudio(0, 1, "user")
  await Promise.resolve()
  audio.duration = 2.4
  audio.onloadedmetadata?.()

  assert.deepEqual(await pending, { durationMilliseconds: 2_400 })
  await platform.playAudio(0, 1, 0)
  assert.equal(created, 1)
})
