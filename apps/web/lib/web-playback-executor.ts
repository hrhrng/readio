import type {
  PlaybackEffect,
  PlaybackEvent,
  PlaybackState,
  RustCoreClient,
} from "./rust-core.ts"
import { webDiagnostics, type WebDiagnosticLog } from "./diagnostics.ts"

export interface WebPlaybackPlatform {
  audioPositionMilliseconds(): number
  fetchAudio(
    sentence: number,
    generation: number,
    priority: "user"
  ):
    | { durationMilliseconds: number }
    | null
    | void
    | Promise<{ durationMilliseconds: number } | null | void>
  playAudio(
    sentence: number,
    generation: number,
    offsetMilliseconds: number
  ): void | Promise<void>
  scheduleTick(
    generation: number,
    afterMilliseconds: number
  ): void | Promise<void>
  pauseAudio(): void | Promise<void>
  persistProgress(
    sentence: number,
    positionMilliseconds: number
  ): void | Promise<void>
  prefetchAudio(sentences: number[]): void | Promise<void>
  playbackFailed(sentence: number, message: string): void | Promise<void>
  playbackCompleted(): void | Promise<void>
  stateChanged?(state: PlaybackState): void
}

/**
 * The browser supplies audio/clock/storage facts; Rust owns every transition.
 * Events are serialized so an async audio completion cannot overtake a pause.
 */
export class WebPlaybackExecutor {
  private currentState: PlaybackState | null = null
  private pending: Promise<void> = Promise.resolve()
  private readonly diagnostics: Pick<WebDiagnosticLog, "beginOperation" | "record">
  private readonly operationId: string

  constructor(
    private readonly core: RustCoreClient,
    private readonly platform: WebPlaybackPlatform,
    diagnostics: Pick<WebDiagnosticLog, "beginOperation" | "record"> = webDiagnostics
  ) {
    this.diagnostics = diagnostics
    this.operationId = diagnostics.beginOperation()
  }

  initialize(sentenceCount: number): Promise<void> {
    return this.serialized(async () => {
      this.currentState = await this.core.createPlaybackState(sentenceCount)
    })
  }

  play(): Promise<void> {
    return this.dispatch({ type: "play_requested" })
  }

  seek(
    sentence: number,
    positionMilliseconds: number,
    autoplay: boolean
  ): Promise<void> {
    return this.dispatch({
      type: "seek_requested",
      sentence,
      position_ms: positionMilliseconds,
      autoplay,
    })
  }

  audioSourceChanged(): Promise<void> {
    return this.dispatch({ type: "audio_source_changed" })
  }

  pause(): Promise<void> {
    return this.serialized(async () => {
      await this.reduce({
        type: "pause_requested",
        audio_position_ms: this.platform.audioPositionMilliseconds(),
      })
    })
  }

  audioReady(
    sentence: number,
    generation: number,
    durationMilliseconds: number
  ): Promise<void> {
    return this.dispatch({
      type: "audio_ready",
      sentence,
      generation,
      duration_ms: durationMilliseconds,
    })
  }

  audioFailed(
    sentence: number,
    generation: number,
    message: string
  ): Promise<void> {
    return this.dispatch({ type: "audio_failed", sentence, generation, message })
  }

  clockTick(
    generation: number,
    monotonicMilliseconds: number,
    unixMilliseconds: number
  ): Promise<void> {
    return this.dispatch({
      type: "clock_tick",
      generation,
      monotonic_ms: monotonicMilliseconds,
      unix_ms: unixMilliseconds,
      audio_position_ms: this.platform.audioPositionMilliseconds(),
    })
  }

  setSleepDeadline(deadlineUnixMilliseconds: number): Promise<void> {
    return this.dispatch({
      type: "sleep_timer_set",
      deadline_unix_ms: deadlineUnixMilliseconds,
    })
  }

  clearSleepDeadline(): Promise<void> {
    return this.dispatch({ type: "sleep_timer_cleared" })
  }

  audioEnded(sentence: number, generation: number): Promise<void> {
    return this.dispatch({ type: "audio_ended", sentence, generation })
  }

  state(): PlaybackState {
    if (!this.currentState) {
      throw new Error("Web playback executor is not initialized")
    }
    return this.currentState
  }

  private dispatch(event: PlaybackEvent): Promise<void> {
    return this.serialized(() => this.reduce(event))
  }

  private async reduce(event: PlaybackEvent): Promise<void> {
    const transition = await this.core.reducePlayback(this.state(), event)
    this.currentState = transition.state
    this.platform.stateChanged?.(transition.state)
    const eventGeneration = "generation" in event
      ? event.generation
      : transition.state.generation
    this.diagnostics.record({
      operationId: this.operationId,
      component: "web-player",
      category: "playback",
      name: "clock_sample",
      fields: {
        mode: "read_aloud",
        generation: eventGeneration,
        sentence_index: transition.state.current_sentence,
        media_position_ms: this.platform.audioPositionMilliseconds(),
        ...(transition.state.current_duration_ms === null
          ? {}
          : { media_duration_ms: transition.state.current_duration_ms }),
        playing: transition.state.status === "playing",
      },
    })
    if (event.type === "audio_failed") {
      this.diagnostics.record({
        operationId: this.operationId,
        component: "web-player",
        category: "fallback",
        name: "provider_fallback_stopped",
        fields: {
          from_provider: "cloud",
          to_provider: "none",
          attempt: 1,
          outcome: "stopped",
          reason_code: "provider_failed",
        },
      })
    }
    for (const effect of transition.effects) await this.apply(effect)
  }

  private async apply(effect: PlaybackEffect): Promise<void> {
    switch (effect.type) {
      case "fetch_audio":
        void Promise.resolve(
          this.platform.fetchAudio(
            effect.sentence,
            effect.generation,
            effect.priority
          )
        )
          .then((audio) => {
            if (audio) {
              return this.audioReady(
                effect.sentence,
                effect.generation,
                audio.durationMilliseconds
              )
            }
          })
          .catch((error: unknown) =>
            this.audioFailed(
              effect.sentence,
              effect.generation,
              error instanceof Error ? error.message : "Audio fetch failed"
            )
          )
        return
      case "play_audio":
        try {
          await this.platform.playAudio(
            effect.sentence,
            effect.generation,
            effect.offset_ms
          )
        } catch (error) {
          void this.audioFailed(
            effect.sentence,
            effect.generation,
            error instanceof Error ? error.message : "Audio playback failed"
          )
        }
        return
      case "schedule_tick":
        await this.platform.scheduleTick(effect.generation, effect.after_ms)
        return
      case "pause_audio":
        await this.platform.pauseAudio()
        return
      case "persist_progress":
        await this.platform.persistProgress(
          effect.sentence,
          effect.position_ms
        )
        return
      case "prefetch_audio":
        void Promise.resolve(this.platform.prefetchAudio(effect.sentences)).catch(
          () => undefined
        )
        return
      case "playback_failed":
        await this.platform.playbackFailed(effect.sentence, effect.message)
        return
      case "playback_completed":
        await this.platform.playbackCompleted()
        return
    }
  }

  private serialized(operation: () => Promise<void>): Promise<void> {
    const result = this.pending.then(operation, operation)
    this.pending = result.catch(() => undefined)
    return result
  }
}
