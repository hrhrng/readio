import type { ReadingMode } from "./rust-core.ts"

export interface ReadingModePlayerPort {
  currentSentenceIndex: number
  pause(): void
  playFromSentence(sentence: number): void
  seekToSentence(sentence: number): void
}

/**
 * Transfers one reader cursor between effect owners exactly once.
 *
 * Audio pause must run before leaving read-aloud so Rust samples and persists
 * the real media cursor. A non-autoplay text seek already invalidates Auto's
 * timer, so Auto → Manual does not need a redundant pause command.
 */
export function transitionReadingMode(
  previous: ReadingMode,
  next: ReadingMode,
  audio: ReadingModePlayerPort,
  text: ReadingModePlayerPort
): void {
  if (previous === next) return

  const sentence =
    previous === "read_aloud"
      ? audio.currentSentenceIndex
      : text.currentSentenceIndex

  if (previous === "read_aloud") audio.pause()
  if (previous === "auto" && next !== "manual") text.pause()

  switch (next) {
    case "read_aloud":
      audio.seekToSentence(sentence)
      return
    case "auto":
      text.playFromSentence(sentence)
      return
    case "manual":
      text.seekToSentence(sentence)
  }
}
