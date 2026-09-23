import type { PlaybackState } from "./rust-core.ts"
import type { WebPlaybackPlatform } from "./web-playback-executor.ts"

export interface BrowserAudioClip {
  url: string
  /** Optional only when the source itself provides decoded media metadata. */
  durationMilliseconds?: number
}

export interface BrowserAudioElement {
  currentTime: number
  duration: number
  playbackRate: number
  onloadedmetadata: (() => void) | null
  ontimeupdate: (() => void) | null
  onended: (() => void) | null
  onerror: (() => void) | null
  play(): Promise<void>
  pause(): void
}

export interface BrowserPlaybackDispatcher {
  audioEnded(sentence: number, generation: number): Promise<void>
  audioFailed(
    sentence: number,
    generation: number,
    message: string
  ): Promise<void>
  clockTick(
    generation: number,
    monotonicMilliseconds: number,
    unixMilliseconds: number
  ): Promise<void>
}

export interface BrowserPlaybackPlatformOptions {
  loadAudio(
    sentence: number,
    priority: "user"
  ): Promise<BrowserAudioClip | null | void>
  createAudio(url: string): BrowserAudioElement
  playbackRate(): number
  prefetchAudio?(sentences: number[]): void | Promise<void>
  persistProgress?(
    sentence: number,
    positionMilliseconds: number
  ): void | Promise<void>
  onPlaybackFailed?(sentence: number, message: string): void | Promise<void>
  onPlaybackCompleted?(): void | Promise<void>
  onStateChanged?(state: PlaybackState): void
  onTimeUpdate?(
    sentence: number,
    positionMilliseconds: number,
    durationMilliseconds: number
  ): void
  setTimer?(callback: () => void, afterMilliseconds: number): unknown
  clearTimer?(timer: unknown): void
  monotonicNow?(): number
  unixNow?(): number
}

interface ActiveAudio {
  sentence: number
  url: string
  element: BrowserAudioElement
}

interface PreparedAudioClip {
  url: string
  durationMilliseconds: number
  element?: BrowserAudioElement
}

/** Browser-only effects for the portable Rust playback state machine. */
export class BrowserPlaybackPlatform implements WebPlaybackPlatform {
  private readonly clips = new Map<string, PreparedAudioClip>()
  private active: ActiveAudio | null = null
  private dispatcher: BrowserPlaybackDispatcher | null = null
  private timer: unknown

  constructor(private readonly options: BrowserPlaybackPlatformOptions) {}

  connect(dispatcher: BrowserPlaybackDispatcher): void {
    this.dispatcher = dispatcher
  }

  audioPositionMilliseconds(): number {
    return Math.max(0, Math.round((this.active?.element.currentTime ?? 0) * 1_000))
  }

  async fetchAudio(
    sentence: number,
    generation: number,
    priority: "user"
  ): Promise<{ durationMilliseconds: number } | null | void> {
    const clip = await this.options.loadAudio(sentence, priority)
    if (!clip) return clip
    const suppliedDuration = clip.durationMilliseconds
    const prepared =
      Number.isFinite(suppliedDuration) && (suppliedDuration ?? 0) > 0
        ? {
            url: clip.url,
            durationMilliseconds: Math.round(suppliedDuration!),
          }
        : await this.prepareFromMediaMetadata(clip.url)
    this.clips.set(this.clipKey(sentence, generation), prepared)
    return { durationMilliseconds: prepared.durationMilliseconds }
  }

  async playAudio(
    sentence: number,
    generation: number,
    offsetMilliseconds: number
  ): Promise<void> {
    const fetched = this.clips.get(this.clipKey(sentence, generation))
    const canReuse =
      this.active?.sentence === sentence &&
      (fetched === undefined || fetched.url === this.active.url)

    if (!canReuse) {
      if (!fetched) {
        throw new Error(`Audio clip is unavailable for sentence ${sentence}`)
      }
      this.detachActiveAudio()
      this.active = {
        sentence,
        url: fetched.url,
        element: fetched.element ?? this.options.createAudio(fetched.url),
      }
    }

    const active = this.active
    if (!active) throw new Error(`Audio clip is unavailable for sentence ${sentence}`)
    const element = active.element
    element.playbackRate = this.options.playbackRate()
    const requestedSeconds = Math.max(0, offsetMilliseconds) / 1_000
    element.currentTime =
      Number.isFinite(element.duration) && element.duration > 0
        ? Math.min(requestedSeconds, element.duration)
        : requestedSeconds
    this.bindAudioEvents(active, generation)
    await element.play()
  }

  scheduleTick(generation: number, afterMilliseconds: number): void {
    this.clearScheduledTick()
    const schedule = this.options.setTimer ?? ((callback, delay) => setTimeout(callback, delay))
    this.timer = schedule(() => {
      this.timer = undefined
      const dispatcher = this.dispatcher
      if (!dispatcher) return
      void dispatcher.clockTick(
        generation,
        this.options.monotonicNow?.() ?? performance.now(),
        this.options.unixNow?.() ?? Date.now()
      )
    }, afterMilliseconds)
  }

  pauseAudio(): void {
    this.clearScheduledTick()
    this.active?.element.pause()
  }

  refreshPlaybackRate(): void {
    if (this.active) {
      this.active.element.playbackRate = this.options.playbackRate()
    }
  }

  persistProgress(sentence: number, positionMilliseconds: number): void | Promise<void> {
    return this.options.persistProgress?.(sentence, positionMilliseconds)
  }

  prefetchAudio(sentences: number[]): void | Promise<void> {
    return this.options.prefetchAudio?.(sentences)
  }

  playbackFailed(sentence: number, message: string): void | Promise<void> {
    return this.options.onPlaybackFailed?.(sentence, message)
  }

  playbackCompleted(): void | Promise<void> {
    return this.options.onPlaybackCompleted?.()
  }

  stateChanged(state: PlaybackState): void {
    this.options.onStateChanged?.(state)
  }

  dispose(): void {
    this.clearScheduledTick()
    const activeElement = this.active?.element
    for (const clip of this.clips.values()) {
      if (!clip.element || clip.element === activeElement) continue
      clip.element.pause()
      clip.element.onloadedmetadata = null
      clip.element.ontimeupdate = null
      clip.element.onended = null
      clip.element.onerror = null
    }
    this.detachActiveAudio()
    this.clips.clear()
    this.dispatcher = null
  }

  private bindAudioEvents(active: ActiveAudio, generation: number): void {
    const { element, sentence } = active
    element.onloadedmetadata = () => {
      this.emitTimeUpdate(active)
    }
    element.ontimeupdate = () => {
      this.emitTimeUpdate(active)
    }
    element.onended = () => {
      this.clearScheduledTick()
      const dispatcher = this.dispatcher
      if (dispatcher) void dispatcher.audioEnded(sentence, generation)
    }
    element.onerror = () => {
      const dispatcher = this.dispatcher
      if (dispatcher) {
        void dispatcher.audioFailed(sentence, generation, "Audio playback failed")
      }
    }
  }

  private async prepareFromMediaMetadata(url: string): Promise<PreparedAudioClip> {
    const element = this.options.createAudio(url)
    const immediateDuration = this.mediaDurationMilliseconds(element)
    const durationMilliseconds =
      immediateDuration ??
      (await new Promise<number>((resolve, reject) => {
        const cleanup = () => {
          element.onloadedmetadata = null
          element.onerror = null
        }
        element.onloadedmetadata = () => {
          const duration = this.mediaDurationMilliseconds(element)
          cleanup()
          if (duration === null) {
            reject(new Error("Audio metadata did not contain a finite duration"))
          } else {
            resolve(duration)
          }
        }
        element.onerror = () => {
          cleanup()
          reject(new Error("Audio metadata could not be loaded"))
        }
      }))
    return { url, durationMilliseconds, element }
  }

  private mediaDurationMilliseconds(element: BrowserAudioElement): number | null {
    if (!Number.isFinite(element.duration) || element.duration <= 0) return null
    return Math.max(1, Math.round(element.duration * 1_000))
  }

  private emitTimeUpdate(active: ActiveAudio): void {
    const { element, sentence } = active
    this.options.onTimeUpdate?.(
      sentence,
      Math.max(0, element.currentTime * 1_000),
      Number.isFinite(element.duration) ? Math.max(0, element.duration * 1_000) : 0
    )
  }

  private detachActiveAudio(): void {
    if (!this.active) return
    const element = this.active.element
    element.pause()
    element.onloadedmetadata = null
    element.ontimeupdate = null
    element.onended = null
    element.onerror = null
    this.active = null
  }

  private clearScheduledTick(): void {
    if (this.timer === undefined) return
    const clear = this.options.clearTimer ?? ((timer) => clearTimeout(timer as ReturnType<typeof setTimeout>))
    clear(this.timer)
    this.timer = undefined
  }

  private clipKey(sentence: number, generation: number): string {
    return `${sentence}:${generation}`
  }
}
