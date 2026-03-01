"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Sentence } from "./types";
import { AudioCache } from "./audio-cache";
import { createTTSJob, pollTTSJob, cancelTTSSession } from "./api";
import { countTokens } from "./cjk";

interface UseTTSPlayerParams {
  itemId: string;
  sentences: Sentence[];
  speed: number;
  voice: string | null;
}

export interface UseTTSPlayerReturn {
  isPlaying: boolean;
  isLoading: boolean;
  currentSentenceIndex: number;
  currentWordProgress: number;
  estimatedRemainingSeconds: number;
  /** Non-null when the current sentence failed after all retries. */
  ttsError: string | null;

  play(): void;
  pause(): void;
  playFromSentence(index: number): void;
  /** Move to a sentence without triggering playback (for progress restoration). */
  seekToSentence(index: number): void;
  nextSentence(): void;
  prevSentence(): void;
  setSpeed(speed: number): void;
  seekToProgress(fraction: number): void;
  /** Retry the failed sentence (clears the error and resumes playback). */
  retry(): void;
  /** Dismiss the error without retrying (stays paused on the same sentence). */
  dismissError(): void;
}

// Maximum time (ms) to poll a single TTS job before giving up
const MAX_POLL_MS = 30_000;

// Maximum number of retry attempts for a failed TTS fetch before surfacing an error
const MAX_FETCH_RETRIES = 3;

// Number of silent auto-retries after the first visible attempt fails.
// These retries run without loading spinner or error UI.
const SILENT_RETRY_COUNT = 2;

// Base delay (ms) between silent retries; actual delay = base * attempt.
const SILENT_RETRY_DELAY_MS = 800;

// Number of sentences to prefetch ahead of the current position.
// Kept small to match the backend's TTS_JOB_PARALLEL_LIMIT (2 workers, ~5s each).
// A window of 5 means the queue never grows beyond ~3 jobs (1 user + 2 prefetch).
const PREFETCH_WINDOW = 5;

// Maximum number of concurrent prefetch requests per wave — matches backend worker
// count so we saturate capacity without piling up a long queue that would timeout.
const MAX_CONCURRENT_PREFETCH = 2;

function base64ToBlob(base64: string, type = "audio/mpeg"): Blob {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return new Blob([bytes], { type });
}

export function useTTSPlayer({
  itemId,
  sentences,
  speed,
  voice,
}: UseTTSPlayerParams): UseTTSPlayerReturn {
  const [isPlaying, setIsPlaying] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [currentSentenceIndex, setCurrentSentenceIndex] = useState(0);
  const [currentWordProgress, setCurrentWordProgress] = useState(0);
  const [estimatedRemainingSeconds, setEstimatedRemainingSeconds] =
    useState(-1);
  const [ttsError, setTtsError] = useState<string | null>(null);

  // Bumped when voice changes to force the Playback Effect to re-run,
  // which triggers re-synthesis of the current sentence with the new voice.
  const [voiceGeneration, setVoiceGeneration] = useState(0);

  const audioRef = useRef<HTMLAudioElement | null>(null);
  const cacheRef = useRef(new AudioCache(50));
  const sessionIdRef = useRef(
    `session-${itemId}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
  );
  const durations = useRef<number[]>([]);
  const pendingJobsRef = useRef<Set<string>>(new Set());
  const cancelledRef = useRef(false);

  // Debounce guard — prevents concurrent retry loops when the user rapidly
  // clicks the "retry" button multiple times.
  const retryingRef = useRef(false);

  // In-flight fetch dedup — prevents duplicate TTS jobs for the same sentence.
  // Key = sentence index, value = the pending promise.
  // Result types: { url, duration } = success, { error } = permanent failure, null = transient/retry-able.
  const fetchPromisesRef = useRef(
    new Map<number, Promise<{ url: string; duration: number } | { error: string } | null>>()
  );

  // Refs for mutable values — callbacks read from `.current` so their
  // references stay stable (deps = []) and memo comparators don't need
  // to check onClick identity.
  const sentencesRef = useRef<Sentence[]>(sentences);
  const speedRef = useRef(speed);
  const voiceRef = useRef(voice);
  const itemIdRef = useRef(itemId);

  // Sync refs with props
  useEffect(() => { sentencesRef.current = sentences; }, [sentences]);
  useEffect(() => { speedRef.current = speed; }, [speed]);
  useEffect(() => { voiceRef.current = voice; }, [voice]);
  useEffect(() => { itemIdRef.current = itemId; }, [itemId]);

  // Cleanup on unmount — cancel pending jobs, release audio & cache
  useEffect(() => {
    // Reset on (re-)mount: React Strict Mode double-invokes effects, so the
    // previous cleanup sets cancelledRef=true; we must clear it here or all
    // subsequent polling loops bail immediately.
    cancelledRef.current = false;
    const sid = sessionIdRef.current;
    return () => {
      cancelledRef.current = true;
      if (audioRef.current) {
        audioRef.current.pause();
        audioRef.current.onloadedmetadata = null;
        audioRef.current.ontimeupdate = null;
        audioRef.current.onended = null;
        audioRef.current.onerror = null;
        audioRef.current = null;
      }
      cancelTTSSession(sid).catch(() => {});
      // clear() properly revokes all cached object URLs
      cacheRef.current.clear();
    };
  }, []);

  // Invalidate cached audio when speed changes — TTS generates different audio
  // per speed, so stale entries would play at the wrong speed.
  const prevSpeedRef = useRef(speed);
  useEffect(() => {
    if (prevSpeedRef.current !== speed) {
      cacheRef.current.clear();
      // In-flight promises will still resolve, but their results (generated at
      // the old speed) will be stale. Clearing the map lets new fetches start
      // fresh with the updated speed value.
      fetchPromisesRef.current.clear();
      prevSpeedRef.current = speed;
    }
  }, [speed]);

  // Invalidate cached audio when voice changes — different voice = different audio.
  // Also bump voiceGeneration so the Playback Effect re-runs and replays the
  // current sentence with the new voice (if currently playing).
  const prevVoiceRef = useRef(voice);
  useEffect(() => {
    if (prevVoiceRef.current !== voice) {
      cacheRef.current.clear();
      fetchPromisesRef.current.clear();
      prevVoiceRef.current = voice;
      setVoiceGeneration((prev) => prev + 1);
    }
  }, [voice]);

  /**
   * Recompute estimated remaining time.
   * Accounts for both future sentences (avg duration * count) and the
   * unplayed portion of the current sentence for a smoothly ticking display.
   * @param idx            current sentence index
   * @param elapsedSec     seconds already played in the current sentence (0 at sentence start)
   */
  const updateEstimate = useCallback(
    (idx: number, elapsedSec = 0) => {
      if (durations.current.length === 0 || sentencesRef.current.length === 0) {
        setEstimatedRemainingSeconds(-1);
        return;
      }
      const recent = durations.current.slice(-10);
      const avgMs = recent.reduce((a, b) => a + b, 0) / recent.length;

      // Time for all sentences after the current one
      const futureSentences = sentencesRef.current.length - idx - 1;
      const futureSec = (avgMs * futureSentences) / 1000;

      // Remaining portion of the current sentence (use actual duration if known, else avg)
      const currentDurationMs = durations.current[durations.current.length - 1] ?? avgMs;
      const currentRemainingSec = Math.max(0, currentDurationMs / 1000 - elapsedSec);

      setEstimatedRemainingSeconds(Math.round(futureSec + currentRemainingSec));
    },
    []
  );

  // Single-attempt TTS fetch — creates one job, polls until done.
  // Returns { url, duration } on success, { error } on permanent backend failure
  // (don't retry), or null on transient failure / cancellation (retry-able).
  const fetchAudioOnce = useCallback(
    async (
      sentenceIdx: number,
      priority: "user" | "prefetch"
    ): Promise<{ url: string; duration: number } | { error: string } | null> => {
      const sentence = sentencesRef.current[sentenceIdx];
      try {
        const job = await createTTSJob({
          session_id: sessionIdRef.current,
          item_id: itemIdRef.current,
          chapter_id: "main",
          priority,
          request: {
            text: sentence.text,
            speed: speedRef.current,
            ...(voiceRef.current ? { voice: voiceRef.current } : {}),
          },
        });

        const jobId = job.job_id;
        pendingJobsRef.current.add(jobId);

        // Poll until done, with a hard timeout to avoid infinite loops on stuck jobs
        let delay = 200;
        let elapsed = 0;
        let status = job;
        while (
          status.status === "queued" ||
          status.status === "running"
        ) {
          if (cancelledRef.current) return null;
          if (elapsed >= MAX_POLL_MS) {
            console.warn(`TTS job ${jobId} timed out after ${MAX_POLL_MS}ms`);
            pendingJobsRef.current.delete(jobId);
            return null;
          }
          await new Promise((r) => setTimeout(r, delay));
          elapsed += delay;
          delay = Math.min(delay * 1.5, 2000);
          status = await pollTTSJob(jobId, true);
        }

        pendingJobsRef.current.delete(jobId);

        // Permanent backend failure (e.g. API quota exceeded, invalid text) —
        // return the error immediately so callers can skip retries.
        if (status.status === "failed") {
          return { error: status.error || "TTS synthesis failed" };
        }

        // Cache hit: createTTSJob returns completed immediately but without
        // audio_base64. Do one explicit poll with include_audio=true to fetch it.
        if (status.status === "completed" && !status.audio_base64) {
          status = await pollTTSJob(jobId, true);
        }

        if (status.status === "completed" && status.audio_base64) {
          const blob = base64ToBlob(status.audio_base64);
          const url = URL.createObjectURL(blob);
          // Prefer backend-reported duration; fall back to 3s if missing.
          // The real duration from the browser will override this later
          // via el.onloadedmetadata in the Playback Effect.
          const duration = status.duration_ms || 3000;
          const entry = { url, duration };
          // Cache owns the object URL — it will be revoked on eviction or clear()
          cacheRef.current.set(sentenceIdx, entry);
          return entry;
        }
        return null;
      } catch (err) {
        console.error(`[TTS] fetchAudioOnce failed for sentence ${sentenceIdx}:`, err);
        return null;
      }
    },
    []
  );

  // Ref-stable audio fetcher — shared by Playback Effect and Prefetch Effect.
  // Returns a cached or freshly-fetched { url, duration } for the given sentence,
  // or null on failure / cancellation.
  //
  // Dedup: if a fetch for the same sentenceIdx is already in-flight, the existing
  // promise is returned instead of creating a duplicate TTS job.
  //
  // Single-attempt only — retry logic is managed by the caller (Playback Effect
  // for silent retries, retry() for user-initiated retries). This keeps the
  // fetcher simple and gives callers full control over retry UX (visible vs silent).
  const fetchAudio = useCallback(
    async (
      sentenceIdx: number,
      priority: "user" | "prefetch"
    ): Promise<{ url: string; duration: number } | { error: string } | null> => {
      if (cancelledRef.current) return null;
      const sentence = sentencesRef.current[sentenceIdx];
      if (!sentence || !sentence.text.trim()) return null;

      // 1) Already cached → instant return
      const cached = cacheRef.current.get(sentenceIdx);
      if (cached) return cached;

      // 2) Already in-flight → reuse the same promise (dedup core)
      const inflight = fetchPromisesRef.current.get(sentenceIdx);
      if (inflight) return inflight;

      // 3) New request → single attempt, no internal retry
      const maxAttempts = 1;

      const promise = (async (): Promise<{ url: string; duration: number } | { error: string } | null> => {
        try {
          for (let attempt = 1; attempt <= maxAttempts; attempt++) {
            if (cancelledRef.current) return null;

            const result = await fetchAudioOnce(sentenceIdx, priority);
            if (result) return result;
          }
          return null;
        } finally {
          // Remove from in-flight map once settled (success or failure)
          fetchPromisesRef.current.delete(sentenceIdx);
        }
      })();

      fetchPromisesRef.current.set(sentenceIdx, promise);
      return promise;
    },
    [fetchAudioOnce]
  );

  // ---------------------------------------------------------------------------
  // Core Playback Effect — the single engine that drives all audio playback.
  //
  // Reacts to changes in (currentSentenceIndex, isPlaying):
  //   1. Fetches audio for the current sentence (cache hit = instant)
  //   2. Creates an Audio element and starts playback
  //   3. On `ended`, advances index → effect re-triggers for next sentence
  //
  // The cleanup function sets `cancelled = true` and tears down the Audio
  // element, replacing the old isPlayingRef guard pattern.
  // ---------------------------------------------------------------------------
  useEffect(() => {
    if (!isPlaying) return;

    const idx = currentSentenceIndex;
    if (idx < 0 || idx >= sentencesRef.current.length) {
      setIsPlaying(false);
      return;
    }

    let cancelled = false;

    (async () => {
      setCurrentWordProgress(0);
      setIsLoading(true);
      setTtsError(null);

      // Phase 1: check cache / reuse in-flight prefetch / first attempt (visible loading).
      // The user sees a spinner only during this initial request.
      let audio: { url: string; duration: number } | { error: string } | null | undefined =
        cacheRef.current.get(idx);
      if (!audio) {
        const inflight = fetchPromisesRef.current.get(idx);
        audio = inflight ? await inflight : await fetchAudioOnce(idx, "user");
      }
      if (cancelled) return;
      setIsLoading(false);

      // Permanent backend failure — skip retries and surface the specific error
      if (audio && "error" in audio) {
        setTtsError(`语音合成失败：${audio.error}`);
        setIsPlaying(false);
        return;
      }

      // Phase 2: silent auto-retries (no loading spinner, no error yet).
      // Up to SILENT_RETRY_COUNT additional attempts with linear backoff.
      // Only runs for transient failures (null), not permanent ones ({ error }).
      if (!audio) {
        for (let i = 1; i <= SILENT_RETRY_COUNT; i++) {
          if (cancelled) return;
          await new Promise(r => setTimeout(r, SILENT_RETRY_DELAY_MS * i));
          if (cancelled) return;
          const retryResult = await fetchAudioOnce(idx, "user");
          // Permanent failure during retry — stop immediately
          if (retryResult && "error" in retryResult) {
            setTtsError(`语音合成失败：${retryResult.error}`);
            setIsPlaying(false);
            return;
          }
          audio = retryResult;
          if (audio) break;
        }
      }

      if (cancelled) return;
      if (!audio) {
        // All retries exhausted — pause and surface the error to the user
        // instead of silently skipping to the next sentence.
        setTtsError(`语音合成失败（第 ${idx + 1} 句），请重试`);
        setIsPlaying(false);
        return;
      }

      durations.current.push(audio.duration);
      updateEstimate(idx);

      // Tear down the previous Audio element to prevent stale callbacks
      if (audioRef.current) {
        audioRef.current.pause();
        audioRef.current.onloadedmetadata = null;
        audioRef.current.ontimeupdate = null;
        audioRef.current.onended = null;
        audioRef.current.onerror = null;
      }

      const el = new Audio(audio.url);
      el.playbackRate = speedRef.current;
      audioRef.current = el;

      // Override the backend-reported duration with the browser's real decoded
      // duration once metadata is available. This ensures accurate remaining-time
      // estimates even when the backend returns duration_ms = null.
      el.onloadedmetadata = () => {
        const realDurationMs = el.duration * 1000;
        if (realDurationMs > 0 && isFinite(realDurationMs)) {
          durations.current[durations.current.length - 1] = realDurationMs;
          updateEstimate(idx);
        }
      };

      // Word-level progress tracking — countTokens handles CJK (per-char) and Latin (per-word)
      const wordCount = countTokens(sentencesRef.current[idx].text);
      el.ontimeupdate = () => {
        if (el.duration > 0) {
          const progress = el.currentTime / el.duration;
          setCurrentWordProgress(Math.floor(progress * wordCount) / wordCount);
          // Tick the remaining-time estimate using real playback position
          updateEstimate(idx, el.currentTime);
        }
      };

      // Auto-advance: onended just bumps the index → effect re-triggers
      el.onended = () => {
        if (cancelled) return;
        if (idx + 1 < sentencesRef.current.length) {
          setCurrentSentenceIndex(idx + 1);
        } else {
          setIsPlaying(false);
        }
      };

      el.onerror = () => {
        if (cancelled) return;
        // Audio element failed to decode/play — surface error instead of skipping
        setTtsError(`音频播放失败（第 ${idx + 1} 句），请重试`);
        setIsPlaying(false);
      };

      try {
        await el.play();
      } catch {
        if (!cancelled) setIsPlaying(false);
      }
    })();

    // Cleanup: cancel in-flight work and release the Audio element
    return () => {
      cancelled = true;
      setIsLoading(false);
      if (audioRef.current) {
        audioRef.current.pause();
        audioRef.current.onloadedmetadata = null;
        audioRef.current.ontimeupdate = null;
        audioRef.current.onended = null;
        audioRef.current.onerror = null;
        audioRef.current = null;
      }
    };
    // voiceGeneration: when voice changes mid-playback, re-run to synthesize
    // the current sentence with the new voice (cleanup stops the old audio).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentSentenceIndex, isPlaying, voiceGeneration]);

  // ---------------------------------------------------------------------------
  // Prefetch Effect — eagerly fetches the next PREFETCH_WINDOW sentences
  // whenever the playback position changes, so subsequent plays are instant.
  //
  // Waves of MAX_CONCURRENT_PREFETCH requests are dispatched sequentially to
  // avoid flooding the backend. Already-cached and in-flight sentences are
  // skipped (via cacheRef + fetchPromisesRef dedup). On cleanup (user jumps
  // to a new position), `cancelled` is set so no further waves are issued.
  // ---------------------------------------------------------------------------
  useEffect(() => {
    const len = sentencesRef.current.length;
    let cancelled = false;

    (async () => {
      // Collect sentences that actually need fetching (skip cached & in-flight)
      const toFetch: number[] = [];
      for (
        let i = currentSentenceIndex + 1;
        i <= currentSentenceIndex + PREFETCH_WINDOW && i < len;
        i++
      ) {
        if (!cacheRef.current.has(i) && !fetchPromisesRef.current.has(i)) {
          toFetch.push(i);
        }
      }

      // Dispatch in waves of MAX_CONCURRENT_PREFETCH, waiting for each wave
      // to settle before sending the next one.
      for (let start = 0; start < toFetch.length; start += MAX_CONCURRENT_PREFETCH) {
        if (cancelled) break;
        const wave = toFetch.slice(start, start + MAX_CONCURRENT_PREFETCH);
        await Promise.allSettled(wave.map((i) => fetchAudio(i, "prefetch")));
      }
    })();

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentSentenceIndex]);

  // Update playback rate when speed prop changes (live adjustment)
  useEffect(() => {
    if (audioRef.current) {
      audioRef.current.playbackRate = speed;
    }
  }, [speed]);

  // ---------------------------------------------------------------------------
  // Public API — all callbacks are pure state setters, no imperative chains.
  // React batches the state updates, so the Playback Effect fires exactly once.
  // ---------------------------------------------------------------------------

  const play = useCallback(() => {
    setIsPlaying(true);
  }, []);

  const pause = useCallback(() => {
    setIsPlaying(false);
    // Pause audio immediately — don't wait for effect cleanup (avoids perceived latency)
    audioRef.current?.pause();
  }, []);

  const playFromSentence = useCallback((index: number) => {
    setCurrentSentenceIndex(index);
    setIsPlaying(true);
    // React batches → Playback Effect triggers once with both new values
  }, []);

  // Seek to a sentence without triggering playback — used for restoring
  // reading position on page load. Only updates index + resets word progress.
  const seekToSentence = useCallback((index: number) => {
    setCurrentSentenceIndex(index);
    setCurrentWordProgress(0);
  }, []);

  const nextSentence = useCallback(() => {
    setCurrentSentenceIndex(prev => {
      const next = prev + 1;
      return next < sentencesRef.current.length ? next : prev;
    });
  }, []);

  const prevSentence = useCallback(() => {
    // If current audio is >2s in, restart current sentence (no state change, direct audio op)
    if (audioRef.current && audioRef.current.currentTime > 2) {
      audioRef.current.currentTime = 0;
      return;
    }
    setCurrentSentenceIndex(prev => Math.max(0, prev - 1));
  }, []);

  const setSpeedFn = useCallback((newSpeed: number) => {
    if (audioRef.current) {
      audioRef.current.playbackRate = newSpeed;
    }
  }, []);

  const seekToProgress = useCallback((fraction: number) => {
    const len = sentencesRef.current.length;
    const targetIdx = Math.round(fraction * (len - 1));
    if (targetIdx >= 0 && targetIdx < len) {
      setCurrentSentenceIndex(targetIdx);
    }
  }, []);

  // Retry the current sentence after a TTS error — silently fetches audio in
  // the background (no loading spinner). On success, triggers the Playback
  // Effect which will cache-hit instantly. Debounced via retryingRef to prevent
  // concurrent retry loops from rapid clicks.
  const retry = useCallback(() => {
    if (retryingRef.current) return; // debounce concurrent clicks

    const idx = currentSentenceIndex;
    cacheRef.current.delete(idx);
    fetchPromisesRef.current.delete(idx);
    setTtsError(null);

    // Clear prefetch dedup entries to free backend workers for current sentence.
    // Backend priority queue already ranks "user" > "prefetch", but clearing
    // stale dedup entries prevents the frontend from waiting on slow prefetches.
    for (const key of fetchPromisesRef.current.keys()) {
      if (key !== idx) fetchPromisesRef.current.delete(key);
    }

    retryingRef.current = true;

    (async () => {
      try {
        for (let attempt = 0; attempt < MAX_FETCH_RETRIES; attempt++) {
          if (cancelledRef.current) return;
          if (attempt > 0) {
            await new Promise(r => setTimeout(r, SILENT_RETRY_DELAY_MS * attempt));
          }
          if (cancelledRef.current) return;
          const result = await fetchAudioOnce(idx, "user");

          // Permanent backend failure — surface specific error, no more retries
          if (result && "error" in result) {
            setTtsError(`语音合成失败：${result.error}`);
            return;
          }

          if (result) {
            // Audio now in cache; trigger Playback Effect — cache hit = instant, no loading
            setIsPlaying(true);
            return;
          }
        }
        // All retries failed — re-show error
        setTtsError(`语音合成失败（第 ${idx + 1} 句），请重试`);
      } finally {
        retryingRef.current = false;
      }
    })();
  }, [currentSentenceIndex, fetchAudioOnce]);

  const dismissError = useCallback(() => {
    setTtsError(null);
  }, []);

  return {
    isPlaying,
    isLoading,
    currentSentenceIndex,
    currentWordProgress,
    estimatedRemainingSeconds,
    ttsError,
    play,
    pause,
    playFromSentence,
    seekToSentence,
    nextSentence,
    prevSentence,
    setSpeed: setSpeedFn,
    seekToProgress,
    retry,
    dismissError,
  };
}
