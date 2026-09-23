import type {
  RustCoreClient,
  TextAdvanceEffect,
  TextAdvanceEvent,
  TextAdvanceState,
} from "./rust-core.ts"

export interface WebTextAdvancePlatform {
  monotonicMilliseconds(): number
  scheduleAdvance(generation: number, afterMilliseconds: number): void | Promise<void>
  cancelScheduledAdvance(): void | Promise<void>
  persistProgress(sentence: number): void | Promise<void>
  textAdvanceCompleted(): void | Promise<void>
  stateChanged?(state: TextAdvanceState): void
}

/**
 * Serial Web adapter for Rust-owned automatic text progression.
 *
 * The browser supplies character counts and monotonic clock samples. It never
 * computes a dwell deadline, increments a sentence, or accepts a stale timer.
 */
export class WebTextAdvanceExecutor {
  private currentState: TextAdvanceState | null = null
  private characterCounts: number[] = []
  private charactersPerSecond = 0
  private pending: Promise<void> = Promise.resolve()

  constructor(
    private readonly core: RustCoreClient,
    private readonly platform: WebTextAdvancePlatform
  ) {}

  initialize(
    characterCounts: number[],
    charactersPerSecond: number
  ): Promise<void> {
    return this.serialized(async () => {
      this.characterCounts = characterCounts.map((count) =>
        Math.max(0, Math.round(count))
      )
      this.charactersPerSecond = charactersPerSecond
      this.currentState = await this.core.createTextAdvanceState(
        this.characterCounts.length
      )
      this.platform.stateChanged?.(this.currentState)
    })
  }

  start(): Promise<void> {
    return this.dispatch({
      type: "start_requested",
      character_count: this.characterCount(this.state().current_sentence),
      text_characters_per_second: this.charactersPerSecond,
      monotonic_ms: this.monotonicNow(),
    })
  }

  pause(): Promise<void> {
    return this.dispatch({
      type: "pause_requested",
      monotonic_ms: this.monotonicNow(),
    })
  }

  seek(sentence: number, autoplay: boolean): Promise<void> {
    const target = this.clampedSentence(sentence)
    return this.dispatch({
      type: "seek_requested",
      sentence: target,
      character_count: this.characterCount(target),
      text_characters_per_second: this.charactersPerSecond,
      monotonic_ms: this.monotonicNow(),
      autoplay,
    })
  }

  advanceDue(generation: number): Promise<void> {
    const next = this.state().current_sentence + 1
    return this.dispatch({
      type: "advance_due",
      generation,
      monotonic_ms: this.monotonicNow(),
      next_character_count: this.characterCount(next),
      text_characters_per_second: this.charactersPerSecond,
    })
  }

  state(): TextAdvanceState {
    if (!this.currentState) {
      throw new Error("Web text advance executor is not initialized")
    }
    return this.currentState
  }

  private dispatch(event: TextAdvanceEvent): Promise<void> {
    return this.serialized(async () => {
      const transition = await this.core.reduceTextAdvance(this.state(), event)
      this.currentState = transition.state
      this.platform.stateChanged?.(transition.state)
      for (const effect of transition.effects) await this.apply(effect)
    })
  }

  private async apply(effect: TextAdvanceEffect): Promise<void> {
    switch (effect.type) {
      case "schedule_advance":
        await this.platform.scheduleAdvance(effect.generation, effect.after_ms)
        return
      case "cancel_scheduled_advance":
        await this.platform.cancelScheduledAdvance()
        return
      case "persist_progress":
        await this.platform.persistProgress(effect.sentence)
        return
      case "text_advance_completed":
        await this.platform.textAdvanceCompleted()
        return
    }
  }

  private characterCount(sentence: number): number {
    return this.characterCounts[sentence] ?? 0
  }

  private clampedSentence(sentence: number): number {
    if (this.characterCounts.length === 0) return 0
    return Math.min(
      Math.max(0, Math.round(sentence)),
      this.characterCounts.length - 1
    )
  }

  private monotonicNow(): number {
    return Math.max(0, Math.round(this.platform.monotonicMilliseconds()))
  }

  private serialized(operation: () => Promise<void>): Promise<void> {
    const result = this.pending.then(operation, operation)
    this.pending = result.catch(() => undefined)
    return result
  }
}
