"use client";

import { useEffect, useRef, useState } from "react";
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  Loader2,
  RotateCw,
  X,
  Mic2,
  Volume2,
} from "lucide-react";
import { UseTTSPlayerReturn } from "@/lib/use-tts-player";
import { useVoices } from "@/lib/hooks";

interface PlayerBarProps {
  player: UseTTSPlayerReturn;
  speed: number;
  onSpeedChange: (speed: number) => void;
  totalSentences: number;
  voice: string | null;
  onVoiceChange: (voiceId: string) => void;
  /** Display label for the currently selected voice */
  voiceLabel: string | null;
}

type LanguageFilter = "all" | "en" | "zh";

const SPEED_OPTIONS = [0.75, 1.0, 1.25, 1.5, 2.0];

function formatTime(seconds: number): string {
  if (seconds < 0) return "";
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

export function PlayerBar({
  player,
  speed,
  onSpeedChange,
  totalSentences,
  voice,
  onVoiceChange,
  voiceLabel,
}: PlayerBarProps) {
  const [voicePanelOpen, setVoicePanelOpen] = useState(false);
  const [langFilter, setLangFilter] = useState<LanguageFilter>("all");
  const voicePanelRef = useRef<HTMLDivElement>(null);

  const { data: voiceData } = useVoices();

  // Close voice panel when clicking outside
  useEffect(() => {
    if (!voicePanelOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (
        voicePanelRef.current &&
        !voicePanelRef.current.contains(e.target as Node)
      ) {
        setVoicePanelOpen(false);
      }
    };
    // Delay listener attachment to avoid the opening click immediately closing
    const timer = setTimeout(
      () => document.addEventListener("mousedown", handleClickOutside),
      0
    );
    return () => {
      clearTimeout(timer);
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [voicePanelOpen]);

  const progress =
    totalSentences > 0
      ? player.currentSentenceIndex / totalSentences
      : 0;
  const progressPercent = Math.round(progress * 100);

  const cycleSpeed = () => {
    const currentIdx = SPEED_OPTIONS.indexOf(speed);
    const nextIdx = (currentIdx + 1) % SPEED_OPTIONS.length;
    const newSpeed = SPEED_OPTIONS[nextIdx];
    onSpeedChange(newSpeed);
    player.setSpeed(newSpeed);
  };

  const remaining = player.estimatedRemainingSeconds;

  // Resolve effective voice for highlighting in the panel
  const effectiveVoiceId = voice ?? voiceData?.default_voice_id ?? null;

  // Filter voices by selected language tab
  const filteredVoices =
    voiceData?.voices.filter(
      (v) => langFilter === "all" || v.language === langFilter
    ) ?? [];

  return (
    <>
      {/* TTS error banner — slides in above the player bar */}
      {player.ttsError && (
        <div className="fixed bottom-[76px] left-1/2 -translate-x-1/2 w-[min(480px,calc(100%-32px))] z-40 bg-red-50 dark:bg-red-950 border border-red-200 dark:border-red-800 rounded-xl px-4 py-2 flex items-center gap-3">
          <span className="text-sm text-red-700 dark:text-red-300 flex-1 min-w-0 truncate">
            {player.ttsError}
          </span>
          <button
            onClick={player.retry}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-white bg-red-600 hover:bg-red-700 rounded-md transition-colors duration-200 cursor-pointer shrink-0"
          >
            <RotateCw size={14} />
            重试
          </button>
          <button
            onClick={player.dismissError}
            className="p-1 text-red-400 hover:text-red-600 dark:text-red-500 dark:hover:text-red-300 transition-colors duration-200 cursor-pointer shrink-0"
            aria-label="Dismiss error"
          >
            <X size={16} />
          </button>
        </div>
      )}
    <div className="fixed bottom-4 left-1/2 -translate-x-1/2 w-[min(480px,calc(100%-32px))] h-[56px] bg-surface-card/80 backdrop-blur-xl border border-border rounded-2xl shadow-lg z-40 flex items-center px-4 gap-2">
      {/* Left — progress & remaining time */}
      <div className="flex-1 flex items-center gap-2 text-xs text-text-secondary tabular-nums">
        <span>{progressPercent}%</span>
        {remaining >= 0 && (
          <>
            <span className="text-border">·</span>
            <span>−{formatTime(remaining)}</span>
          </>
        )}
      </div>

      {/* Center — transport controls */}
      <div className="flex items-center gap-1">
        <button
          onClick={player.prevSentence}
          className="w-8 h-8 flex items-center justify-center rounded-full text-text-secondary hover:text-text-primary transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
          aria-label="Previous sentence"
        >
          <SkipBack size={16} />
        </button>

        <button
          onClick={player.isPlaying ? player.pause : player.play}
          disabled={player.isLoading && !player.isPlaying}
          className="w-10 h-10 flex items-center justify-center rounded-full bg-accent text-white hover:opacity-90 transition-opacity duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 disabled:opacity-50"
          aria-label={player.isPlaying ? "Pause" : "Play"}
        >
          {player.isLoading && !player.isPlaying ? (
            <Loader2 size={18} className="animate-spin" />
          ) : player.isPlaying ? (
            <Pause size={18} />
          ) : (
            <Play size={18} className="ml-0.5" />
          )}
        </button>

        <button
          onClick={player.nextSentence}
          className="w-8 h-8 flex items-center justify-center rounded-full text-text-secondary hover:text-text-primary transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
          aria-label="Next sentence"
        >
          <SkipForward size={16} />
        </button>
      </div>

      {/* Right — speed & voice */}
      <div className="flex-1 flex items-center justify-end gap-1">
        <button
          onClick={cycleSpeed}
          className="h-8 px-2 flex items-center justify-center rounded-lg text-xs font-medium text-text-secondary hover:text-text-primary hover:bg-surface-hover transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent tabular-nums"
          aria-label={`Playback speed: ${speed}x`}
        >
          {speed}x
        </button>

        {/* Voice selector button + floating panel */}
        <div className="relative" ref={voicePanelRef}>
          <button
            onClick={() => setVoicePanelOpen(!voicePanelOpen)}
            className="h-8 flex items-center gap-1 px-2 rounded-lg text-text-secondary hover:text-text-primary hover:bg-surface-hover transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            aria-label="Select voice"
          >
            <Mic2 size={14} />
            <span className="max-w-[72px] truncate text-xs hidden sm:inline">
              {voiceLabel ?? "Voice"}
            </span>
          </button>

          {/* Voice selection floating panel — opens above the player bar */}
          {voicePanelOpen && (
            <div className="absolute bottom-full right-0 mb-2 w-72 bg-surface-card border border-border rounded-xl shadow-lg z-50 overflow-hidden">
              <div className="flex items-center justify-between px-4 py-3 border-b border-border">
                <h3 className="text-sm font-semibold text-text-primary">Voice</h3>
                <button
                  onClick={() => setVoicePanelOpen(false)}
                  className="w-7 h-7 flex items-center justify-center rounded-md text-text-secondary hover:text-text-primary transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                  aria-label="Close voice panel"
                >
                  <X size={14} />
                </button>
              </div>

              <div className="p-3 space-y-2">
                {/* Language filter tabs */}
                <div className="flex items-center gap-1 rounded-lg bg-surface-hover p-1">
                  {([
                    { key: "all", label: "All" },
                    { key: "en", label: "English" },
                    { key: "zh", label: "中文" },
                  ] as const).map(({ key, label }) => (
                    <button
                      key={key}
                      onClick={() => setLangFilter(key)}
                      className={`flex-1 rounded-md px-2 py-1.5 text-xs transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${
                        langFilter === key
                          ? "bg-surface-card text-text-primary shadow-sm"
                          : "text-text-secondary hover:text-text-primary"
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                </div>

                {/* Scrollable voice list */}
                <div className="max-h-64 overflow-y-auto rounded-lg border border-border">
                  {filteredVoices.length === 0 ? (
                    <div className="flex items-center justify-center py-6 text-text-secondary">
                      <Loader2 size={16} className="animate-spin mr-2" />
                      <span className="text-xs">Loading voices...</span>
                    </div>
                  ) : (
                    filteredVoices.map((v) => {
                      const isSelected = effectiveVoiceId === v.voice_id;
                      return (
                        <button
                          key={v.voice_id}
                          onClick={() => {
                            onVoiceChange(v.voice_id);
                            setVoicePanelOpen(false);
                          }}
                          className={`w-full flex items-center gap-2 px-3 py-2 text-left text-xs transition-colors duration-150 cursor-pointer border-b border-border last:border-b-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-inset ${
                            isSelected
                              ? "bg-accent/10 text-accent"
                              : "text-text-primary hover:bg-surface-hover"
                          }`}
                        >
                          <Volume2
                            size={14}
                            className={
                              isSelected
                                ? "text-accent shrink-0"
                                : "text-text-secondary shrink-0 opacity-0"
                            }
                          />
                          <div className="min-w-0 flex-1">
                            <div className="font-medium truncate">{v.label}</div>
                            {v.description && (
                              <div className="text-[10px] text-text-secondary truncate mt-0.5">
                                {v.description}
                              </div>
                            )}
                          </div>
                          {v.gender && (
                            <span className="text-[10px] text-text-secondary shrink-0">
                              {v.gender === "male" ? "♂" : "♀"}
                            </span>
                          )}
                        </button>
                      );
                    })
                  )}
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
    </>
  );
}
