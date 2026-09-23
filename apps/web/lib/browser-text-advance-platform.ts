import type { TextAdvanceState } from "./rust-core.ts"
import type { WebTextAdvancePlatform } from "./web-text-advance-executor.ts"

export interface BrowserTextAdvanceDispatcher {
  advanceDue(generation: number): Promise<void>
}

export interface BrowserTextAdvancePlatformOptions {
  setTimer?(callback: () => void, afterMilliseconds: number): unknown
  clearTimer?(timer: unknown): void
  monotonicNow?(): number
  onProgress?(sentence: number): void | Promise<void>
  onCompleted?(): void | Promise<void>
  onStateChanged?(state: TextAdvanceState): void
}

/** Browser clock and UI effects for the Rust text-advance reducer. */
export class BrowserTextAdvancePlatform implements WebTextAdvancePlatform {
  private dispatcher: BrowserTextAdvanceDispatcher | null = null
  private timer: unknown

  constructor(private readonly options: BrowserTextAdvancePlatformOptions = {}) {}

  connect(dispatcher: BrowserTextAdvanceDispatcher): void {
    this.dispatcher = dispatcher
  }

  monotonicMilliseconds(): number {
    return this.options.monotonicNow?.() ?? performance.now()
  }

  scheduleAdvance(generation: number, afterMilliseconds: number): void {
    this.cancelScheduledAdvance()
    const schedule =
      this.options.setTimer ??
      ((callback: () => void, delay: number) => setTimeout(callback, delay))
    this.timer = schedule(() => {
      this.timer = undefined
      if (this.dispatcher) void this.dispatcher.advanceDue(generation)
    }, afterMilliseconds)
  }

  cancelScheduledAdvance(): void {
    if (this.timer === undefined) return
    const clear =
      this.options.clearTimer ??
      ((timer: unknown) => clearTimeout(timer as ReturnType<typeof setTimeout>))
    clear(this.timer)
    this.timer = undefined
  }

  persistProgress(sentence: number): void | Promise<void> {
    return this.options.onProgress?.(sentence)
  }

  textAdvanceCompleted(): void | Promise<void> {
    return this.options.onCompleted?.()
  }

  stateChanged(state: TextAdvanceState): void {
    this.options.onStateChanged?.(state)
  }

  dispose(): void {
    this.cancelScheduledAdvance()
    this.dispatcher = null
  }
}
