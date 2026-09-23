"use client"

import { useCallback, useEffect, useRef, useState } from "react"

import { AudioCache } from "./audio-cache"
import {
  BrowserPlaybackPlatform,
  type BrowserAudioElement,
} from "./browser-playback-platform"
import { synthesizeCloudSpeech } from "./cloud-tts-client"
import { cloudTTSModelResolver } from "./cloud-tts-model-resolver"
import {
  projectSentenceProgress,
  resolveSentenceProgressSeek,
} from "./reading-progress-adapter"
import { createWebPlaybackExecutor, webReadioSdk } from "./readio-core-runtime"
import type { PlaybackState, ReadingTimingEvidence } from "./rust-core"
import type { Sentence } from "./types"
import type { WebPlaybackExecutor } from "./web-playback-executor"
import { closeWebPlayback } from "./web-playback-lifecycle"
import { presentCloudTTSFailure } from "./tts-error-presentation"

interface UseTTSPlayerParams {
  itemId: string
  sentences: Sentence[]
  speed: number
  voice: string | null
  language: string
  onPersistProgress?: (
    sentence: number,
    positionMilliseconds: number
  ) => void | Promise<void>
}

export interface UseTTSPlayerReturn {
  isPlaying: boolean
  isLoading: boolean
  currentSentenceIndex: number
  progressBasisPoints: number
  currentWordProgress: number
  currentPositionMilliseconds: number
  currentDurationMilliseconds: number | null
  playbackRate: number
  estimatedRemainingSeconds: number
  sleepDeadlineUnixMilliseconds: number | null
  ttsError: string | null

  play(): void
  pause(): void
  playFromSentence(index: number): void
  seekToSentence(index: number): void
  nextSentence(): void
  prevSentence(): void
  setSpeed(speed: number): void
  seekToProgress(fraction: number): void
  setSleepDeadline(deadlineUnixMilliseconds: number | null): void
  retry(): void
  dismissError(): void
}

type AudioFetchResult =
  | { url: string }
  | { error: string }
  | null

const SILENT_RETRY_COUNT = 2
const SILENT_RETRY_DELAY_MS = 800
const MAX_CONCURRENT_PREFETCH = 2

export function useTTSPlayer({
  itemId,
  sentences,
  speed,
  voice,
  language,
  onPersistProgress,
}: UseTTSPlayerParams): UseTTSPlayerReturn {
  const [isPlaying, setIsPlaying] = useState(false)
  const [isLoading, setIsLoading] = useState(false)
  const [currentSentenceIndex, setCurrentSentenceIndex] = useState(0)
  const [progressBasisPoints, setProgressBasisPoints] = useState(0)
  const [currentWordProgress, setCurrentWordProgress] = useState(0)
  const [currentPositionMilliseconds, setCurrentPositionMilliseconds] =
    useState(0)
  const [currentDurationMilliseconds, setCurrentDurationMilliseconds] =
    useState<number | null>(null)
  const [estimatedRemainingSeconds, setEstimatedRemainingSeconds] = useState(-1)
  const [sleepDeadlineUnixMilliseconds, setSleepDeadlineUnixMilliseconds] =
    useState<number | null>(null)
  const [ttsError, setTtsError] = useState<string | null>(null)

  const cacheRef = useRef(new AudioCache(50))
  const cancelledRef = useRef(false)
  const sentencesRef = useRef(sentences)
  const speedRef = useRef(speed)
  const voiceRef = useRef(voice)
  const languageRef = useRef(language)
  const sourceVersionRef = useRef(0)
  const durationsRef = useRef(new Map<number, number>())
  const fetchPromisesRef = useRef(new Map<string, Promise<AudioFetchResult>>())
  const executorRef = useRef<WebPlaybackExecutor | null>(null)
  const platformRef = useRef<BrowserPlaybackPlatform | null>(null)
  const readyRef = useRef<Promise<void>>(Promise.resolve())
  const playbackStateRef = useRef<PlaybackState | null>(null)
  const persistProgressRef = useRef(onPersistProgress)
  const projectionGenerationRef = useRef(0)

  useEffect(() => {
    sentencesRef.current = sentences
  }, [sentences])

  useEffect(() => {
    speedRef.current = speed
    platformRef.current?.refreshPlaybackRate()
  }, [speed])

  useEffect(() => {
    voiceRef.current = voice
  }, [voice])

  useEffect(() => {
    languageRef.current = language
  }, [language])

  useEffect(() => {
    persistProgressRef.current = onPersistProgress
  }, [onPersistProgress])

  useEffect(() => {
    cancelledRef.current = false
    const cache = cacheRef.current
    return () => {
      cancelledRef.current = true
      cache.clear()
    }
  }, [])

  const updateProjection = useCallback(
    (
      sentence: number,
      positionMilliseconds = 0,
      decodedDurationMilliseconds?: number
    ) => {
      if (
        decodedDurationMilliseconds !== undefined &&
        Number.isFinite(decodedDurationMilliseconds) &&
        decodedDurationMilliseconds > 0
      ) {
        durationsRef.current.set(
          sentence,
          Math.round(decodedDurationMilliseconds)
        )
      }
      const durations = [...durationsRef.current.entries()].map(
        ([segment, duration_ms]) => ({ segment, duration_ms })
      )
      const timing: ReadingTimingEvidence =
        durations.length === 0
          ? { type: "unknown" }
          : {
              type: "observed_audio",
              playback_rate: speedRef.current,
              current_position_ms: Math.max(0, Math.round(positionMilliseconds)),
              durations,
            }
      const currentDuration = durationsRef.current.get(sentence)
      const segmentProgressBasisPoints =
        currentDuration === undefined || currentDuration <= 0
          ? 0
          : Math.round(
              (Math.min(positionMilliseconds, currentDuration) /
                currentDuration) *
                10_000
            )
      const generation = ++projectionGenerationRef.current
      void projectSentenceProgress(webReadioSdk, sentencesRef.current, {
        currentSegment: sentence,
        currentSegmentProgressBasisPoints: segmentProgressBasisPoints,
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
          setTtsError(
            cause instanceof Error
              ? `Reading progress could not be projected: ${cause.message}`
              : "Reading progress could not be projected"
          )
        })
    },
    []
  )

  useEffect(() => {
    durationsRef.current.clear()
    setEstimatedRemainingSeconds(-1)
    const state = playbackStateRef.current
    updateProjection(state?.current_sentence ?? 0, state?.current_position_ms ?? 0)
  }, [itemId, sentences, updateProjection])

  const fetchAudioOnce = useCallback(
    async (
      sentenceIndex: number,
      _priority: "user" | "prefetch",
      sourceVersion = sourceVersionRef.current
    ): Promise<AudioFetchResult> => {
      const sentence = sentencesRef.current[sentenceIndex]
      if (!sentence?.text.trim() || cancelledRef.current) return null
      const requestedVoice = voiceRef.current

      try {
        const selection = await cloudTTSModelResolver.resolve({
          language: languageRef.current,
          voiceId: requestedVoice,
        })
        const speech = await synthesizeCloudSpeech({
          modelId: selection.modelId,
          language: selection.language,
          text: sentence.text,
          voiceId: selection.voiceId,
          // Audio is synthesized canonically; the Rust effort multiplier is
          // applied once by the browser media element.
          speed: 1,
        })
        if (sourceVersion !== sourceVersionRef.current || cancelledRef.current) return null
        const url = URL.createObjectURL(speech.audio)
        const entry = { url }
        if (sourceVersion !== sourceVersionRef.current) {
          URL.revokeObjectURL(url)
          return null
        }
        cacheRef.current.set(sentenceIndex, entry)
        return entry
      } catch (error) {
        return {
          error: error instanceof Error ? error.message : "Cloud TTS failed",
        }
      }
    },
    []
  )

  const fetchAudio = useCallback(
    (
      sentenceIndex: number,
      priority: "user" | "prefetch"
    ): Promise<AudioFetchResult> => {
      if (cancelledRef.current) return Promise.resolve(null)
      const cached = cacheRef.current.get(sentenceIndex)
      if (cached) return Promise.resolve(cached)

      const sourceVersion = sourceVersionRef.current
      const key = `${sourceVersion}:${sentenceIndex}`
      const inFlight = fetchPromisesRef.current.get(key)
      if (inFlight) return inFlight

      const request = fetchAudioOnce(sentenceIndex, priority, sourceVersion)
      fetchPromisesRef.current.set(key, request)
      void request.finally(() => {
        if (fetchPromisesRef.current.get(key) === request) {
          fetchPromisesRef.current.delete(key)
        }
      })
      return request
    },
    [fetchAudioOnce]
  )

  const loadCurrentAudio = useCallback(
    async (sentenceIndex: number) => {
      let result = await fetchAudio(sentenceIndex, "user")
      if (result && "error" in result) throw new Error(result.error)

      for (
        let attempt = 1;
        !result && attempt <= SILENT_RETRY_COUNT;
        attempt += 1
      ) {
        await new Promise((resolve) =>
          setTimeout(resolve, SILENT_RETRY_DELAY_MS * attempt)
        )
        if (cancelledRef.current) return null
        result = await fetchAudioOnce(sentenceIndex, "user")
        if (result && "error" in result) throw new Error(result.error)
      }

      if (!result) {
        throw new Error("the provider returned no audio")
      }
      return {
        url: result.url,
      }
    },
    [fetchAudio, fetchAudioOnce]
  )

  const prefetchSentences = useCallback(
    async (sentenceIndexes: number[]) => {
      const pending = sentenceIndexes.filter(
        (sentence) =>
          !cacheRef.current.has(sentence) &&
          !fetchPromisesRef.current.has(`${sourceVersionRef.current}:${sentence}`)
      )
      for (
        let start = 0;
        start < pending.length;
        start += MAX_CONCURRENT_PREFETCH
      ) {
        if (cancelledRef.current) return
        const wave = pending.slice(start, start + MAX_CONCURRENT_PREFETCH)
        await Promise.allSettled(
          wave.map((sentence) => fetchAudio(sentence, "prefetch"))
        )
      }
    },
    [fetchAudio]
  )

  const applyPlaybackState = useCallback(
    (state: PlaybackState) => {
      playbackStateRef.current = state
      setIsPlaying(state.status === "playing")
      setIsLoading(state.status === "loading")
      setCurrentSentenceIndex(state.current_sentence)
      setCurrentPositionMilliseconds(state.current_position_ms)
      setCurrentDurationMilliseconds(state.current_duration_ms)
      setSleepDeadlineUnixMilliseconds(state.sleep_deadline_unix_ms)
      if (state.status !== "failed") setTtsError(null)
      if (state.status === "loading" || state.current_position_ms === 0) {
        setCurrentWordProgress(0)
      }
      updateProjection(state.current_sentence, state.current_position_ms)
    },
    [updateProjection]
  )

  useEffect(() => {
    let disposed = false
    const platform = new BrowserPlaybackPlatform({
      loadAudio: loadCurrentAudio,
      createAudio: (url) => new Audio(url) as BrowserAudioElement,
      playbackRate: () => speedRef.current,
      prefetchAudio: prefetchSentences,
      persistProgress: async (sentence, positionMilliseconds) => {
        try {
          await persistProgressRef.current?.(sentence, positionMilliseconds)
        } catch (error) {
          setTtsError(
            error instanceof Error
              ? `Reading position could not be saved: ${error.message}`
              : "Reading position could not be saved"
          )
        }
      },
      onPlaybackFailed: (sentence, message) => {
        setTtsError(presentCloudTTSFailure(sentence, message))
      },
      onPlaybackCompleted: () => {
        setCurrentWordProgress(1)
      },
      onStateChanged: applyPlaybackState,
      onTimeUpdate: (sentence, positionMilliseconds, durationMilliseconds) => {
        const fraction =
          durationMilliseconds > 0
            ? Math.min(1, positionMilliseconds / durationMilliseconds)
            : 0
        setCurrentWordProgress(fraction)
        updateProjection(
          sentence,
          positionMilliseconds,
          durationMilliseconds
        )
      },
    })
    const executor = createWebPlaybackExecutor(platform)
    platform.connect(executor)
    platformRef.current = platform
    executorRef.current = executor
    const ready = executor.initialize(sentences.length).then(() => {
      if (!disposed && executorRef.current === executor) {
        applyPlaybackState(executor.state())
      }
    })
    readyRef.current = ready

    return () => {
      disposed = true
      if (platformRef.current === platform) platformRef.current = null
      if (executorRef.current === executor) executorRef.current = null
      // React cleanup cannot await, but the close protocol retains the old
      // executor/platform pair until Rust has sampled and persisted its cursor.
      void closeWebPlayback(executor, platform).catch(() => undefined)
    }
  }, [
    applyPlaybackState,
    itemId,
    loadCurrentAudio,
    prefetchSentences,
    sentences.length,
    updateProjection,
  ])

  const runWithExecutor = useCallback(
    (operation: (executor: WebPlaybackExecutor) => Promise<void>) => {
      void readyRef.current
        .then(() => {
          const executor = executorRef.current
          return executor ? operation(executor) : undefined
        })
        .catch((error: unknown) => {
          setTtsError(error instanceof Error ? error.message : "Playback failed")
        })
    },
    []
  )

  const previousVoiceRef = useRef(voice)
  useEffect(() => {
    if (previousVoiceRef.current === voice) return
    previousVoiceRef.current = voice
    sourceVersionRef.current += 1
    cacheRef.current.clear()
    durationsRef.current.clear()
    setEstimatedRemainingSeconds(-1)
    fetchPromisesRef.current.clear()
    runWithExecutor((executor) => executor.audioSourceChanged())
  }, [runWithExecutor, voice])

  const previousLanguageRef = useRef(language)
  useEffect(() => {
    if (previousLanguageRef.current === language) return
    previousLanguageRef.current = language
    sourceVersionRef.current += 1
    cacheRef.current.clear()
    durationsRef.current.clear()
    setEstimatedRemainingSeconds(-1)
    fetchPromisesRef.current.clear()
    runWithExecutor((executor) => executor.audioSourceChanged())
  }, [language, runWithExecutor])

  const play = useCallback(() => {
    runWithExecutor((executor) => executor.play())
  }, [runWithExecutor])

  const pause = useCallback(() => {
    runWithExecutor((executor) => executor.pause())
  }, [runWithExecutor])

  const playFromSentence = useCallback(
    (index: number) => {
      runWithExecutor((executor) => executor.seek(index, 0, true))
    },
    [runWithExecutor]
  )

  const seekToSentence = useCallback(
    (index: number) => {
      runWithExecutor((executor) => executor.seek(index, 0, false))
    },
    [runWithExecutor]
  )

  const nextSentence = useCallback(() => {
    runWithExecutor((executor) => {
      const state = executor.state()
      const target = Math.min(state.current_sentence + 1, state.sentence_count - 1)
      const autoplay = state.status === "playing" || state.status === "loading"
      return target < 0
        ? Promise.resolve()
        : executor.seek(target, 0, autoplay)
    })
  }, [runWithExecutor])

  const prevSentence = useCallback(() => {
    runWithExecutor((executor) => {
      const state = executor.state()
      const exactPosition = platformRef.current?.audioPositionMilliseconds() ?? 0
      const target = exactPosition > 2_000
        ? state.current_sentence
        : Math.max(0, state.current_sentence - 1)
      const autoplay = state.status === "playing" || state.status === "loading"
      return executor.seek(target, 0, autoplay)
    })
  }, [runWithExecutor])

  const setSpeed = useCallback((nextSpeed: number) => {
    speedRef.current = nextSpeed
    platformRef.current?.refreshPlaybackRate()
    const state = playbackStateRef.current
    updateProjection(state?.current_sentence ?? 0, state?.current_position_ms ?? 0)
  }, [updateProjection])

  const seekToProgress = useCallback(
    (fraction: number) => {
      runWithExecutor(async (executor) => {
        const state = executor.state()
        if (state.sentence_count === 0) return
        const cursor = await resolveSentenceProgressSeek(
          webReadioSdk,
          sentencesRef.current,
          fraction
        )
        const autoplay = state.status === "playing" || state.status === "loading"
        await executor.seek(cursor.segment, 0, autoplay)
      })
    },
    [runWithExecutor]
  )

  const setSleepDeadline = useCallback(
    (deadlineUnixMilliseconds: number | null) => {
      runWithExecutor((executor) =>
        deadlineUnixMilliseconds === null
          ? executor.clearSleepDeadline()
          : executor.setSleepDeadline(deadlineUnixMilliseconds)
      )
    },
    [runWithExecutor]
  )

  const retry = useCallback(() => {
    const sentence = playbackStateRef.current?.current_sentence ?? 0
    cacheRef.current.delete(sentence)
    fetchPromisesRef.current.delete(`${sourceVersionRef.current}:${sentence}`)
    setTtsError(null)
    runWithExecutor((executor) => executor.play())
  }, [runWithExecutor])

  const dismissError = useCallback(() => {
    setTtsError(null)
  }, [])

  return {
    isPlaying,
    isLoading,
    currentSentenceIndex,
    progressBasisPoints,
    currentWordProgress,
    currentPositionMilliseconds,
    currentDurationMilliseconds,
    playbackRate: speed,
    estimatedRemainingSeconds,
    sleepDeadlineUnixMilliseconds,
    ttsError,
    play,
    pause,
    playFromSentence,
    seekToSentence,
    nextSentence,
    prevSentence,
    setSpeed,
    seekToProgress,
    setSleepDeadline,
    retry,
    dismissError,
  }
}
