"use client"

import { useCallback, useEffect, useRef, useState } from "react"

import { BrowserTextAdvancePlatform } from "./browser-text-advance-platform"
import {
  projectSentenceProgress,
  resolveSentenceProgressSeek,
} from "./reading-progress-adapter"
import {
  createWebTextAdvanceExecutor,
  webReadioSdk,
} from "./readio-core-runtime"
import type { Sentence } from "./types"
import type { UseTTSPlayerReturn } from "./use-tts-player"
import type { WebTextAdvanceExecutor } from "./web-text-advance-executor"

interface UseTextAdvancePlayerParams {
  sentences: Sentence[]
  charactersPerSecond: number
  enabled: boolean
  onPersistProgress?: (sentence: number) => void | Promise<void>
}

/** React projection for Rust-owned automatic text progression. */
export function useTextAdvancePlayer({
  sentences,
  charactersPerSecond,
  enabled,
  onPersistProgress,
}: UseTextAdvancePlayerParams): UseTTSPlayerReturn {
  const [isPlaying, setIsPlaying] = useState(false)
  const [currentSentenceIndex, setCurrentSentenceIndex] = useState(0)
  const [progressBasisPoints, setProgressBasisPoints] = useState(0)
  const [estimatedRemainingSeconds, setEstimatedRemainingSeconds] = useState(-1)
  const [error, setError] = useState<string | null>(null)
  const executorRef = useRef<WebTextAdvanceExecutor | null>(null)
  const platformRef = useRef<BrowserTextAdvancePlatform | null>(null)
  const readyRef = useRef<Promise<void>>(Promise.resolve())
  const currentSentenceRef = useRef(0)
  const enabledRef = useRef(enabled)
  const sentencesRef = useRef(sentences)
  const persistProgressRef = useRef(onPersistProgress)
  const projectionGenerationRef = useRef(0)

  useEffect(() => {
    enabledRef.current = enabled
  }, [enabled])

  useEffect(() => {
    sentencesRef.current = sentences
  }, [sentences])

  useEffect(() => {
    persistProgressRef.current = onPersistProgress
  }, [onPersistProgress])

  const updateProjection = useCallback(
    (sentence: number, completed = false) => {
      const generation = ++projectionGenerationRef.current
      const timing =
        enabledRef.current && charactersPerSecond > 0
          ? {
              type: "text_rate" as const,
              characters_per_second: charactersPerSecond,
            }
          : { type: "unknown" as const }
      void projectSentenceProgress(webReadioSdk, sentencesRef.current, {
        currentSegment: sentence,
        currentSegmentProgressBasisPoints: completed ? 10_000 : 0,
        timing,
      })
        .then((projection) => {
          if (generation !== projectionGenerationRef.current) return
          setProgressBasisPoints(projection.progress_basis_points)
          setEstimatedRemainingSeconds(
            projection.remaining_time.seconds ?? -1
          )
        })
        .catch((cause: unknown) => {
          if (generation !== projectionGenerationRef.current) return
          setEstimatedRemainingSeconds(-1)
          setError(
            cause instanceof Error
              ? `Reading progress could not be projected: ${cause.message}`
              : "Reading progress could not be projected"
          )
        })
    },
    [charactersPerSecond]
  )

  useEffect(() => {
    updateProjection(currentSentenceRef.current)
  }, [enabled, updateProjection])

  useEffect(() => {
    let disposed = false
    const previousSentence = currentSentenceRef.current
    const platform = new BrowserTextAdvancePlatform({
      onProgress: async (sentence) => {
        try {
          await persistProgressRef.current?.(sentence)
        } catch (cause) {
          setError(
            cause instanceof Error
              ? `Reading position could not be saved: ${cause.message}`
              : "Reading position could not be saved"
          )
        }
      },
      onCompleted: () => setIsPlaying(false),
      onStateChanged: (state) => {
        currentSentenceRef.current = state.current_sentence
        setCurrentSentenceIndex(state.current_sentence)
        setIsPlaying(state.status === "running")
        updateProjection(
          state.current_sentence,
          state.status === "completed"
        )
      },
    })
    const executor = createWebTextAdvanceExecutor(platform)
    platform.connect(executor)
    platformRef.current = platform
    executorRef.current = executor
    const characterCounts = sentences.map((sentence) =>
      Array.from(sentence.text).length
    )
    const ready = executor
      .initialize(characterCounts, charactersPerSecond)
      .then(async () => {
        if (disposed || executorRef.current !== executor) return
        if (previousSentence > 0 && characterCounts.length > 0) {
          await executor.seek(previousSentence, enabledRef.current)
        } else if (enabledRef.current) {
          await executor.start()
        }
      })
      .catch((cause: unknown) => {
        if (!disposed) {
          setError(cause instanceof Error ? cause.message : "Auto reading failed")
        }
      })
    readyRef.current = ready

    return () => {
      disposed = true
      platform.dispose()
      if (platformRef.current === platform) platformRef.current = null
      if (executorRef.current === executor) executorRef.current = null
    }
  }, [charactersPerSecond, sentences, updateProjection])

  const run = useCallback(
    (operation: (executor: WebTextAdvanceExecutor) => Promise<void>) => {
      void readyRef.current
        .then(() => {
          const executor = executorRef.current
          return executor ? operation(executor) : undefined
        })
        .catch((cause: unknown) => {
          setError(cause instanceof Error ? cause.message : "Auto reading failed")
        })
    },
    []
  )

  const play = useCallback(() => {
    if (enabledRef.current) run((executor) => executor.start())
  }, [run])
  const pause = useCallback(() => run((executor) => executor.pause()), [run])
  const playFromSentence = useCallback(
    (index: number) =>
      run((executor) => executor.seek(index, enabledRef.current)),
    [run]
  )
  const seekToSentence = useCallback(
    (index: number) => run((executor) => executor.seek(index, false)),
    [run]
  )
  const nextSentence = useCallback(
    () =>
      run((executor) =>
        executor.seek(
          executor.state().current_sentence + 1,
          executor.state().status === "running"
        )
      ),
    [run]
  )
  const prevSentence = useCallback(
    () =>
      run((executor) =>
        executor.seek(
          executor.state().current_sentence - 1,
          executor.state().status === "running"
        )
      ),
    [run]
  )
  const seekToProgress = useCallback(
    (fraction: number) =>
      run(async (executor) => {
        if (executor.state().sentence_count === 0) return
        const cursor = await resolveSentenceProgressSeek(
          webReadioSdk,
          sentencesRef.current,
          fraction
        )
        await executor.seek(
          cursor.segment,
          executor.state().status === "running"
        )
      }),
    [run]
  )

  return {
    isPlaying,
    isLoading: false,
    currentSentenceIndex,
    progressBasisPoints,
    currentWordProgress: 0,
    currentPositionMilliseconds: 0,
    currentDurationMilliseconds: null,
    playbackRate: 1,
    estimatedRemainingSeconds,
    sleepDeadlineUnixMilliseconds: null,
    ttsError: error,
    play,
    pause,
    playFromSentence,
    seekToSentence,
    nextSentence,
    prevSentence,
    setSpeed: () => {},
    seekToProgress,
    setSleepDeadline: () => {},
    retry: play,
    dismissError: () => setError(null),
  }
}
