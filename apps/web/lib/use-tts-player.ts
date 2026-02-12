"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Sentence } from "./types";
import { AudioCache } from "./audio-cache";
import { createTTSJob, pollTTSJob, cancelTTSSession } from "./api";

interface UseTTSPlayerParams {
  itemId: string;
  sentences: Sentence[];
  speed: number;
  prefetchWindow?: number;
}

export interface UseTTSPlayerReturn {
  isPlaying: boolean;
  isLoading: boolean;
  currentSentenceIndex: number;
  currentWordProgress: number;
  estimatedRemainingSeconds: number;

  play(): void;
  pause(): void;
  playFromSentence(index: number): void;
  nextSentence(): void;
  prevSentence(): void;
  setSpeed(speed: number): void;
  seekToProgress(fraction: number): void;
}

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
  prefetchWindow = 3,
}: UseTTSPlayerParams): UseTTSPlayerReturn {
  const [isPlaying, setIsPlaying] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [currentSentenceIndex, setCurrentSentenceIndex] = useState(0);
  const [currentWordProgress, setCurrentWordProgress] = useState(0);
  const [estimatedRemainingSeconds, setEstimatedRemainingSeconds] =
    useState(-1);

  const audioRef = useRef<HTMLAudioElement | null>(null);
  const cacheRef = useRef(new AudioCache(50));
  const sessionIdRef = useRef(
    `session-${itemId}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
  );
  const durations = useRef<number[]>([]);
  const isPlayingRef = useRef(false);
  const currentIndexRef = useRef(0);
  const pendingJobsRef = useRef<Set<string>>(new Set());
  const cancelledRef = useRef(false);

  // Sync refs
  useEffect(() => {
    isPlayingRef.current = isPlaying;
  }, [isPlaying]);

  useEffect(() => {
    currentIndexRef.current = currentSentenceIndex;
  }, [currentSentenceIndex]);

  // Cleanup on unmount
  useEffect(() => {
    const sid = sessionIdRef.current;
    return () => {
      cancelledRef.current = true;
      if (audioRef.current) {
        audioRef.current.pause();
        audioRef.current = null;
      }
      cancelTTSSession(sid).catch(() => {});
      cacheRef.current.clear();
    };
  }, []);

  const updateEstimate = useCallback(
    (idx: number) => {
      if (durations.current.length === 0 || sentences.length === 0) {
        setEstimatedRemainingSeconds(-1);
        return;
      }
      const recent = durations.current.slice(-10);
      const avg = recent.reduce((a, b) => a + b, 0) / recent.length;
      const remaining = sentences.length - idx - 1;
      setEstimatedRemainingSeconds(Math.round((avg * remaining) / 1000));
    },
    [sentences.length]
  );

  const fetchAudio = useCallback(
    async (
      sentenceIdx: number,
      priority: "user" | "prefetch"
    ): Promise<{ blob: Blob; duration: number } | null> => {
      if (cancelledRef.current) return null;
      const sentence = sentences[sentenceIdx];
      if (!sentence || !sentence.text.trim()) return null;

      const cached = cacheRef.current.get(sentenceIdx);
      if (cached) return cached;

      try {
        const job = await createTTSJob({
          session_id: sessionIdRef.current,
          item_id: itemId,
          chapter_id: "main",
          priority,
          request: { text: sentence.text, speed },
        });

        const jobId = job.job_id;
        pendingJobsRef.current.add(jobId);

        // Poll until done
        let delay = 200;
        let status = job;
        while (
          status.status === "queued" ||
          status.status === "running"
        ) {
          if (cancelledRef.current) return null;
          await new Promise((r) => setTimeout(r, delay));
          delay = Math.min(delay * 1.5, 2000);
          status = await pollTTSJob(jobId, true);
        }

        pendingJobsRef.current.delete(jobId);

        if (status.status === "completed" && status.audio_base64) {
          const blob = base64ToBlob(status.audio_base64);
          const duration = status.duration_ms || 3000;
          const entry = { blob, duration };
          cacheRef.current.set(sentenceIdx, entry);
          return entry;
        }
        return null;
      } catch {
        return null;
      }
    },
    [itemId, sentences, speed]
  );

  const prefetch = useCallback(
    (fromIdx: number) => {
      for (
        let i = fromIdx + 1;
        i <= fromIdx + prefetchWindow && i < sentences.length;
        i++
      ) {
        if (!cacheRef.current.has(i)) {
          fetchAudio(i, "prefetch");
        }
      }
    },
    [fetchAudio, prefetchWindow, sentences.length]
  );

  const playSentence = useCallback(
    async (idx: number) => {
      if (idx < 0 || idx >= sentences.length) {
        setIsPlaying(false);
        return;
      }

      setCurrentSentenceIndex(idx);
      setCurrentWordProgress(0);
      setIsLoading(true);

      const audio = await fetchAudio(idx, "user");
      setIsLoading(false);

      if (!audio) {
        // Skip failed sentence, try next
        if (isPlayingRef.current && idx + 1 < sentences.length) {
          playSentence(idx + 1);
        } else {
          setIsPlaying(false);
        }
        return;
      }

      durations.current.push(audio.duration);
      updateEstimate(idx);

      // Start prefetching
      prefetch(idx);

      if (audioRef.current) {
        audioRef.current.pause();
        const oldSrc = audioRef.current.src;
        audioRef.current.src = "";
        URL.revokeObjectURL(oldSrc);
      }

      const el = new Audio();
      const url = URL.createObjectURL(audio.blob);
      el.src = url;
      el.playbackRate = speed;
      audioRef.current = el;

      // Word progress tracking
      const wordCount = sentences[idx].text.split(/\s+/).length;
      el.ontimeupdate = () => {
        if (el.duration > 0) {
          const progress = el.currentTime / el.duration;
          setCurrentWordProgress(
            Math.floor(progress * wordCount) / wordCount
          );
        }
      };

      el.onended = () => {
        URL.revokeObjectURL(url);
        if (isPlayingRef.current && idx + 1 < sentences.length) {
          playSentence(idx + 1);
        } else {
          setIsPlaying(false);
        }
      };

      el.onerror = () => {
        URL.revokeObjectURL(url);
        if (isPlayingRef.current && idx + 1 < sentences.length) {
          playSentence(idx + 1);
        } else {
          setIsPlaying(false);
        }
      };

      if (isPlayingRef.current) {
        try {
          await el.play();
        } catch {
          setIsPlaying(false);
        }
      }
    },
    [sentences, fetchAudio, speed, prefetch, updateEstimate]
  );

  const play = useCallback(() => {
    setIsPlaying(true);
    isPlayingRef.current = true;
    playSentence(currentIndexRef.current);
  }, [playSentence]);

  const pause = useCallback(() => {
    setIsPlaying(false);
    isPlayingRef.current = false;
    if (audioRef.current) {
      audioRef.current.pause();
    }
  }, []);

  const playFromSentence = useCallback(
    (index: number) => {
      setIsPlaying(true);
      isPlayingRef.current = true;
      playSentence(index);
    },
    [playSentence]
  );

  const nextSentence = useCallback(() => {
    const next = currentIndexRef.current + 1;
    if (next < sentences.length) {
      if (isPlayingRef.current) {
        playSentence(next);
      } else {
        setCurrentSentenceIndex(next);
      }
    }
  }, [sentences.length, playSentence]);

  const prevSentence = useCallback(() => {
    // If current audio is >2s in, restart; otherwise go to prev
    if (audioRef.current && audioRef.current.currentTime > 2) {
      audioRef.current.currentTime = 0;
      return;
    }
    const prev = currentIndexRef.current - 1;
    if (prev >= 0) {
      if (isPlayingRef.current) {
        playSentence(prev);
      } else {
        setCurrentSentenceIndex(prev);
      }
    }
  }, [playSentence]);

  const setSpeedFn = useCallback((newSpeed: number) => {
    if (audioRef.current) {
      audioRef.current.playbackRate = newSpeed;
    }
  }, []);

  // Update playback rate when speed prop changes
  useEffect(() => {
    if (audioRef.current) {
      audioRef.current.playbackRate = speed;
    }
  }, [speed]);

  const seekToProgress = useCallback(
    (fraction: number) => {
      const targetIdx = Math.round(fraction * (sentences.length - 1));
      if (targetIdx >= 0 && targetIdx < sentences.length) {
        if (isPlayingRef.current) {
          playSentence(targetIdx);
        } else {
          setCurrentSentenceIndex(targetIdx);
        }
      }
    },
    [sentences.length, playSentence]
  );

  return {
    isPlaying,
    isLoading,
    currentSentenceIndex,
    currentWordProgress,
    estimatedRemainingSeconds,
    play,
    pause,
    playFromSentence,
    nextSentence,
    prevSentence,
    setSpeed: setSpeedFn,
    seekToProgress,
  };
}
