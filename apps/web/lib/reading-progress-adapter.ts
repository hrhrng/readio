import type {
  ReadingProgressProjection,
  ReadingSeekCursor,
  ReadingTimingEvidence,
  RustCoreClient,
} from "./rust-core.ts"
import type { Sentence } from "./types.ts"

type ReadingProgressCore = Pick<
  RustCoreClient,
  "projectReadingProgress" | "resolveReadingSeek"
>

interface SentenceProgressInput {
  currentSegment: number
  currentSegmentProgressBasisPoints: number
  timing: ReadingTimingEvidence
}

/** JavaScript code-point iteration matches Rust's canonical Unicode-scalar offsets. */
export function scalarSentenceCharacterCounts(sentences: Sentence[]): number[] {
  return sentences.map((sentence) => Array.from(sentence.text).length)
}

export function projectSentenceProgress(
  core: ReadingProgressCore,
  sentences: Sentence[],
  input: SentenceProgressInput
): Promise<ReadingProgressProjection> {
  return core.projectReadingProgress({
    segment_character_counts: scalarSentenceCharacterCounts(sentences),
    current_segment: input.currentSegment,
    current_segment_progress_basis_points:
      input.currentSegmentProgressBasisPoints,
    timing: input.timing,
  })
}

export function resolveSentenceProgressSeek(
  core: ReadingProgressCore,
  sentences: Sentence[],
  fraction: number
): Promise<ReadingSeekCursor> {
  const progressBasisPoints = Math.round(
    Math.min(1, Math.max(0, fraction)) * 10_000
  )
  return core.resolveReadingSeek(
    scalarSentenceCharacterCounts(sentences),
    progressBasisPoints
  )
}
