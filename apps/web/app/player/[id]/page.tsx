"use client";

import { use, useCallback, useEffect, useState } from "react";
import { useLibraryItem } from "@/lib/hooks";
import { updateProgress } from "@/lib/api";
import { Sentence } from "@/lib/types";
import { useTTSPlayer } from "@/lib/use-tts-player";
import { TopBar } from "@/components/reader/top-bar";
import { PlayerBar } from "@/components/reader/player-bar";
import { ContentRouter } from "@/components/reader/content-router";
import { TOCPanel } from "@/components/reader/toc-panel";
import { SettingsPanel } from "@/components/reader/settings-panel";

export default function PlayerPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const { data: item, isLoading } = useLibraryItem(id);
  const [sentences, setSentences] = useState<Sentence[]>([]);
  const [tocOpen, setTocOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [speed, setSpeed] = useState(1.0);
  const [tocItems, setTocItems] = useState<
    { id: string; title: string }[]
  >([]);
  const [fontSize, setFontSize] = useState("18px");

  const player = useTTSPlayer({
    itemId: id,
    sentences,
    speed,
    prefetchWindow: 3,
  });

  // Load font size from localStorage
  useEffect(() => {
    const saved = localStorage.getItem("readio-font-size");
    const sizeMap: Record<string, string> = {
      small: "16px",
      medium: "18px",
      large: "22px",
    };
    if (saved && saved in sizeMap) {
      setFontSize(sizeMap[saved]);
    }

    // Listen for changes
    const handler = () => {
      const current = localStorage.getItem("readio-font-size");
      if (current && current in sizeMap) {
        setFontSize(sizeMap[current]);
      }
    };
    window.addEventListener("storage", handler);
    // Also poll for same-tab changes
    const interval = setInterval(handler, 500);
    return () => {
      window.removeEventListener("storage", handler);
      clearInterval(interval);
    };
  }, []);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't capture when typing in inputs
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement
      ) {
        return;
      }

      switch (e.key) {
        case " ":
          e.preventDefault();
          if (player.isPlaying) {
            player.pause();
          } else {
            player.play();
          }
          break;
        case "ArrowLeft":
          e.preventDefault();
          player.prevSentence();
          break;
        case "ArrowRight":
          e.preventDefault();
          player.nextSentence();
          break;
        case "[": {
          e.preventDefault();
          const speeds = [0.75, 1.0, 1.25, 1.5, 2.0];
          const idx = speeds.indexOf(speed);
          if (idx > 0) {
            const newSpeed = speeds[idx - 1];
            setSpeed(newSpeed);
            player.setSpeed(newSpeed);
          }
          break;
        }
        case "]": {
          e.preventDefault();
          const speeds = [0.75, 1.0, 1.25, 1.5, 2.0];
          const idx = speeds.indexOf(speed);
          if (idx < speeds.length - 1) {
            const newSpeed = speeds[idx + 1];
            setSpeed(newSpeed);
            player.setSpeed(newSpeed);
          }
          break;
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [player, speed]);

  // Progress persistence
  useEffect(() => {
    if (!item || sentences.length === 0) return;

    const saveProgress = () => {
      const progress = Math.round(
        (player.currentSentenceIndex / sentences.length) * 100
      );
      updateProgress(id, progress).catch(() => {});
    };

    window.addEventListener("beforeunload", saveProgress);
    return () => {
      window.removeEventListener("beforeunload", saveProgress);
      saveProgress();
    };
  }, [id, item, player.currentSentenceIndex, sentences.length]);

  const handleSentencesExtracted = useCallback(
    (extracted: Sentence[]) => {
      setSentences(extracted);
    },
    []
  );

  const handleChaptersExtracted = useCallback(
    (chapters: { id: string; title: string }[]) => {
      setTocItems(chapters);
    },
    []
  );

  const handleOutlineExtracted = useCallback(
    (outline: { title: string; page: number }[]) => {
      setTocItems(
        outline.map((o, i) => ({ id: `outline-${i}`, title: o.title }))
      );
    },
    []
  );

  const handleTocItemClick = useCallback(
    (index: number) => {
      // For now, scroll to the chapter element
      const chapterEl = document.querySelector(
        `[data-chapter="chapter-${index + 1}"]`
      );
      if (chapterEl) {
        chapterEl.scrollIntoView({ behavior: "smooth", block: "start" });
      }
    },
    []
  );

  // Generate TOC from plain text if none available
  useEffect(() => {
    if (
      tocItems.length === 0 &&
      sentences.length > 0 &&
      item &&
      (item.type === "web" || item.type === "txt")
    ) {
      // Generate from paragraphs
      const paragraphs = item.content
        .split(/\n\n+/)
        .filter((p) => p.trim());
      const generated = paragraphs.slice(0, 20).map((p, i) => ({
        id: `para-${i}`,
        title: p.trim().substring(0, 40) + (p.trim().length > 40 ? "..." : ""),
      }));
      setTocItems(generated);
    }
  }, [tocItems.length, sentences.length, item]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center min-h-screen">
        <div className="animate-pulse space-y-4 w-full max-w-prose px-6">
          <div className="h-8 w-64 bg-surface-hover rounded" />
          <div className="h-4 w-48 bg-surface-hover rounded" />
          <div className="space-y-3 mt-8">
            {Array.from({ length: 10 }).map((_, i) => (
              <div
                key={i}
                className="h-4 bg-surface-hover rounded"
                style={{ width: `${60 + Math.random() * 30}%` }}
              />
            ))}
          </div>
        </div>
      </div>
    );
  }

  if (!item) {
    return (
      <div className="flex flex-col items-center justify-center min-h-screen text-center">
        <h2 className="text-xl font-medium text-text-primary">
          Item not found
        </h2>
        <p className="text-sm text-text-secondary mt-2">
          The item you are looking for does not exist.
        </p>
      </div>
    );
  }

  return (
    <>
      <TopBar
        title={item.title}
        onToggleToc={() => setTocOpen(!tocOpen)}
        onToggleSettings={() => setSettingsOpen(!settingsOpen)}
        tocOpen={tocOpen}
      />

      {settingsOpen && (
        <SettingsPanel onClose={() => setSettingsOpen(false)} />
      )}

      <div className="flex pt-12 pb-[72px] min-h-screen">
        {tocOpen && (
          <TOCPanel
            items={tocItems}
            onClose={() => setTocOpen(false)}
            onItemClick={handleTocItemClick}
          />
        )}

        <div className="flex-1 flex justify-center overflow-y-auto">
          <div
            className="max-w-prose w-full px-6 py-8"
            style={{ fontSize }}
          >
            <ContentRouter
              item={item}
              onSentencesExtracted={handleSentencesExtracted}
              currentSentenceIndex={player.currentSentenceIndex}
              currentWordProgress={player.currentWordProgress}
              onSentenceClick={player.playFromSentence}
              onChaptersExtracted={handleChaptersExtracted}
              onOutlineExtracted={handleOutlineExtracted}
            />
          </div>
        </div>
      </div>

      <PlayerBar
        player={player}
        speed={speed}
        onSpeedChange={(s) => {
          setSpeed(s);
          player.setSpeed(s);
        }}
        totalSentences={sentences.length}
      />
    </>
  );
}
