"use client";

import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  Loader2,
} from "lucide-react";
import { UseTTSPlayerReturn } from "@/lib/use-tts-player";

interface PlayerBarProps {
  player: UseTTSPlayerReturn;
  speed: number;
  onSpeedChange: (speed: number) => void;
  totalSentences: number;
}

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
}: PlayerBarProps) {
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

  const handleProgressClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const fraction = Math.max(
      0,
      Math.min(1, (e.clientX - rect.left) / rect.width)
    );
    player.seekToProgress(fraction);
  };

  const remaining = player.estimatedRemainingSeconds;

  return (
    <div className="fixed bottom-0 left-0 right-0 h-[72px] bg-surface-card border-t border-border z-40 flex items-center px-4 gap-3">
      {/* Prev */}
      <button
        onClick={player.prevSentence}
        className="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-full text-text-secondary hover:text-text-primary transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
        aria-label="Previous sentence"
      >
        <SkipBack size={20} />
      </button>

      {/* Play/Pause */}
      <button
        onClick={player.isPlaying ? player.pause : player.play}
        disabled={player.isLoading && !player.isPlaying}
        className="min-w-[48px] min-h-[48px] flex items-center justify-center rounded-full bg-accent text-white hover:opacity-90 transition-opacity duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 disabled:opacity-50"
        aria-label={player.isPlaying ? "Pause" : "Play"}
      >
        {player.isLoading && !player.isPlaying ? (
          <Loader2 size={22} className="animate-spin" />
        ) : player.isPlaying ? (
          <Pause size={22} />
        ) : (
          <Play size={22} className="ml-0.5" />
        )}
      </button>

      {/* Next */}
      <button
        onClick={player.nextSentence}
        className="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-full text-text-secondary hover:text-text-primary transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
        aria-label="Next sentence"
      >
        <SkipForward size={20} />
      </button>

      {/* Progress Bar */}
      <div className="flex-1 flex items-center gap-3 min-w-0">
        <div
          className="flex-1 h-1.5 bg-surface-hover rounded-full cursor-pointer relative group"
          onClick={handleProgressClick}
          role="slider"
          aria-valuenow={progressPercent}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label="Reading progress"
          tabIndex={0}
        >
          <div
            className="h-full bg-accent rounded-full transition-[width] duration-150 relative"
            style={{ width: `${progressPercent}%` }}
          >
            <div className="absolute right-0 top-1/2 -translate-y-1/2 w-3 h-3 bg-accent rounded-full opacity-0 group-hover:opacity-100 transition-opacity duration-200 shadow-sm" />
          </div>
        </div>
        <span className="text-xs text-text-secondary tabular-nums whitespace-nowrap min-w-[32px]">
          {progressPercent}%
        </span>
      </div>

      {/* Speed */}
      <button
        onClick={cycleSpeed}
        className="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-sm font-medium text-text-secondary hover:text-text-primary hover:bg-surface-hover transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent tabular-nums"
        aria-label={`Playback speed: ${speed}x`}
      >
        {speed}x
      </button>

      {/* Estimated time */}
      <span className="text-xs text-text-tertiary whitespace-nowrap hidden sm:inline">
        {remaining >= 0
          ? `Est. ~${formatTime(remaining)} remaining`
          : "Estimating..."}
      </span>
    </div>
  );
}
