"use client"

import { useEffect, useRef } from "react"

import { BrowserMediaSessionController } from "./browser-media-session"
import type { UseTTSPlayerReturn } from "./use-tts-player"

interface UseWebMediaSessionParams {
  enabled: boolean
  title: string
  artwork: string | null
  player: UseTTSPlayerReturn
}

/** Connects Web Media Session only while Read-aloud owns effects. */
export function useWebMediaSession({
  enabled,
  title,
  artwork,
  player,
}: UseWebMediaSessionParams): void {
  const controllerRef = useRef<BrowserMediaSessionController | null>(null)

  useEffect(() => {
    if (
      !enabled ||
      typeof navigator === "undefined" ||
      !("mediaSession" in navigator) ||
      typeof MediaMetadata === "undefined"
    ) {
      return
    }

    const controller = new BrowserMediaSessionController(navigator.mediaSession)
    controller.connect({
      play: player.play,
      pause: player.pause,
      nextSentence: player.nextSentence,
      prevSentence: player.prevSentence,
    })
    controllerRef.current = controller
    return () => {
      controller.disconnect()
      if (controllerRef.current === controller) controllerRef.current = null
    }
  }, [
    enabled,
    player.nextSentence,
    player.pause,
    player.play,
    player.prevSentence,
  ])

  useEffect(() => {
    if (!enabled) return
    controllerRef.current?.update({
      title,
      artist: "Read-aloud",
      artwork,
      isPlaying: player.isPlaying,
      durationSeconds:
        player.currentDurationMilliseconds === null
          ? null
          : player.currentDurationMilliseconds / 1_000,
      positionSeconds: player.currentPositionMilliseconds / 1_000,
      playbackRate: player.playbackRate,
    })
  }, [
    artwork,
    enabled,
    player.currentDurationMilliseconds,
    player.currentPositionMilliseconds,
    player.isPlaying,
    player.playbackRate,
    title,
  ])
}
