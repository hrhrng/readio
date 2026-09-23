"use client"

import { useEffect, useRef, useState, type PropsWithChildren } from "react"
import {
  Check,
  Loader2,
  Mic2,
  Pause,
  Play,
  RotateCw,
  SkipBack,
  SkipForward,
  X,
} from "lucide-react"
import { UseTTSPlayerReturn } from "@/lib/use-tts-player"
import type { ReadingMode } from "@/lib/rust-core"
import { useVoices } from "@/lib/hooks"
import { StatusBanner } from "../feedback"
import { SleepTimerControl } from "./sleep-timer-control"
import { useLocale } from "../locale-provider"
import type { MessageKey } from "@/lib/localization"

interface PlayerBarProps {
  player: UseTTSPlayerReturn
  speed: number
  onSpeedChange: (speed: number) => void
  voice: string | null
  onVoiceChange: (voiceId: string) => void
  voiceLabel: string | null
  mode?: ReadingMode
}

type LanguageFilter = "all" | "en" | "zh"

const SPEED_OPTIONS = [0.75, 1.0, 1.25, 1.5, 2.0]

export function ReaderPlayerSlot({ children }: PropsWithChildren) {
  return <div className="reader-player-slot">{children}</div>
}

function formatTime(seconds: number): string {
  if (seconds < 0) return ""
  const minutes = Math.floor(seconds / 60)
  const remainder = Math.floor(seconds % 60)
  return `${minutes}:${remainder.toString().padStart(2, "0")}`
}

export function PlayerBar({
  player,
  speed,
  onSpeedChange,
  voice,
  onVoiceChange,
  voiceLabel,
  mode = "read_aloud",
}: PlayerBarProps) {
  const { t } = useLocale()
  const [voicePanelOpen, setVoicePanelOpen] = useState(false)
  const [language, setLanguage] = useState<LanguageFilter>("all")
  const voicePanelRef = useRef<HTMLDivElement>(null)
  const { data: voiceData } = useVoices()

  useEffect(() => {
    if (!voicePanelOpen) return

    const handlePointerDown = (event: MouseEvent) => {
      if (!voicePanelRef.current?.contains(event.target as Node)) {
        setVoicePanelOpen(false)
      }
    }
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setVoicePanelOpen(false)
    }
    const timer = window.setTimeout(
      () => document.addEventListener("mousedown", handlePointerDown),
      0
    )
    document.addEventListener("keydown", handleKeyDown)

    return () => {
      window.clearTimeout(timer)
      document.removeEventListener("mousedown", handlePointerDown)
      document.removeEventListener("keydown", handleKeyDown)
    }
  }, [voicePanelOpen])

  const progressPercent = Math.min(
    100,
    Math.max(0, Math.round(player.progressBasisPoints / 100))
  )
  const effectiveVoiceId = voice ?? voiceData?.default_voice_id ?? null
  const filteredVoices =
    voiceData?.voices.filter(
      (candidate) => language === "all" || candidate.language === language
    ) ?? []

  const cycleSpeed = () => {
    const currentIndex = SPEED_OPTIONS.indexOf(speed)
    const nextSpeed = SPEED_OPTIONS[(currentIndex + 1) % SPEED_OPTIONS.length]
    onSpeedChange(nextSpeed)
    player.setSpeed(nextSpeed)
  }

  return (
    <ReaderPlayerSlot>
      {player.ttsError && (
        <StatusBanner
          tone="danger"
          className="reader-error-banner"
          action={
            <div className="feedback-actions">
              <button
                type="button"
                onClick={player.retry}
                className="ui-button ui-button--danger feedback-action"
              >
                <RotateCw className="ui-icon-sm" aria-hidden="true" />
                {t("common.retry")}
              </button>
              <button
                type="button"
                onClick={() => setVoicePanelOpen(true)}
                className="ui-button ui-button--quiet feedback-action"
              >
                {t("reader.changeVoice")}
              </button>
            </div>
          }
          dismiss={
            <button
              type="button"
              onClick={player.dismissError}
              className="ui-icon-button"
              aria-label={t("reader.dismissPlaybackError")}
            >
              <X className="ui-icon-sm" aria-hidden="true" />
            </button>
          }
        >
          <span className="block truncate">{player.ttsError}</span>
        </StatusBanner>
      )}

      <div
        className="reader-player"
        aria-label={
          mode === "read_aloud"
            ? t("reader.readAloudControls")
            : t("reader.readingControls")
        }
      >
        <div className="reader-player-side">
          <span>{progressPercent}%</span>
          {player.estimatedRemainingSeconds >= 0 && (
            <span>−{formatTime(player.estimatedRemainingSeconds)}</span>
          )}
        </div>

        <div className="reader-player-controls">
          <button
            type="button"
            onClick={player.prevSentence}
            className="ui-icon-button reader-control-button"
            aria-label={t("reader.previousSentence")}
          >
            <SkipBack className="ui-icon-sm" aria-hidden="true" />
          </button>
          <button
            type="button"
            onClick={player.isPlaying ? player.pause : player.play}
            disabled={mode === "manual" || (player.isLoading && !player.isPlaying)}
            className="reader-play-button"
            aria-label={
              mode === "manual"
                ? t("reader.manualReading")
                : player.isPlaying
                  ? t("reader.pause")
                  : t("reader.play")
            }
          >
            {player.isLoading && !player.isPlaying ? (
              <Loader2 className="ui-icon animate-spin" aria-hidden="true" />
            ) : player.isPlaying ? (
              <Pause className="ui-icon" aria-hidden="true" />
            ) : (
              <Play className="ui-icon reader-play-icon" aria-hidden="true" />
            )}
          </button>
          <button
            type="button"
            onClick={player.nextSentence}
            className="ui-icon-button reader-control-button"
            aria-label={t("reader.nextSentence")}
          >
            <SkipForward className="ui-icon-sm" aria-hidden="true" />
          </button>
        </div>

        <div className="reader-player-side reader-player-side--end">
          <button
            type="button"
            onClick={cycleSpeed}
            className="reader-text-control"
            aria-label={t("reader.playbackSpeed", { speed })}
          >
            {speed}x
          </button>

          {mode === "read_aloud" && <div className="voice-picker" ref={voicePanelRef}>
            <button
              type="button"
              onClick={() => setVoicePanelOpen((open) => !open)}
              className="reader-text-control"
              aria-label={t("reader.selectVoice")}
              aria-expanded={voicePanelOpen}
            >
              <Mic2 className="ui-icon-sm" aria-hidden="true" />
              <span className="voice-label">{voiceLabel ?? t("reader.voice")}</span>
            </button>

            {voicePanelOpen && (
              <section
                className="surface-popover voice-panel"
                aria-label={t("reader.voiceSelection")}
              >
                <header className="voice-panel-header">
                  <h2>{t("reader.voice")}</h2>
                  <button
                    type="button"
                    className="ui-icon-button"
                    onClick={() => setVoicePanelOpen(false)}
                    aria-label={t("reader.closeVoiceSelection")}
                  >
                    <X className="ui-icon-sm" aria-hidden="true" />
                  </button>
                </header>

                <div className="voice-panel-body">
                  <div className="segmented-control" aria-label={t("reader.voiceLanguage")}>
                    {(
                      [
                        ["all", "reader.allLanguages"],
                        ["en", "reader.english"],
                        ["zh", "reader.chinese"],
                      ] as const
                    ).map(([value, labelKey]) => (
                      <button
                        type="button"
                        key={value}
                        onClick={() => setLanguage(value)}
                        aria-pressed={language === value}
                      >
                        {t(labelKey as MessageKey)}
                      </button>
                    ))}
                  </div>

                  <div className="voice-list">
                    {!voiceData ? (
                      <div className="voice-list-state">
                        <Loader2
                          className="ui-icon-sm animate-spin"
                          aria-hidden="true"
                        />
                        {t("reader.loadingVoices")}
                      </div>
                    ) : filteredVoices.length === 0 ? (
                      <div className="voice-list-state">{t("reader.noVoices")}</div>
                    ) : (
                      filteredVoices.map((candidate) => {
                        const selected = effectiveVoiceId === candidate.voice_id
                        return (
                          <button
                            type="button"
                            key={candidate.voice_id}
                            onClick={() => {
                              onVoiceChange(candidate.voice_id)
                              setVoicePanelOpen(false)
                            }}
                            className="voice-option"
                            aria-pressed={selected}
                          >
                            <span className="min-w-0 flex-1">
                              <strong>{candidate.label}</strong>
                              {candidate.description && (
                                <small>{candidate.description}</small>
                              )}
                            </span>
                            {selected && (
                              <Check className="ui-icon-sm" aria-hidden="true" />
                            )}
                          </button>
                        )
                      })
                    )}
                  </div>
                </div>
              </section>
            )}
          </div>}

          {mode === "read_aloud" && (
            <SleepTimerControl
              deadlineUnixMilliseconds={player.sleepDeadlineUnixMilliseconds}
              onChange={player.setSleepDeadline}
            />
          )}
        </div>
      </div>
    </ReaderPlayerSlot>
  )
}
