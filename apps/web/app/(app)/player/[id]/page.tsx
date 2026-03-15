"use client";

import { use, useCallback, useEffect, useRef, useState } from "react";
import { useLibraryItem, useSettings, useVoices } from "@/lib/hooks";
import { updateProgress, updateVoice, updateSpeed, updateChapter } from "@/lib/api";
import { Sentence } from "@/lib/types";
import { useTTSPlayer } from "@/lib/use-tts-player";
import { TopBar } from "@/components/reader/top-bar";
import { PlayerBar } from "@/components/reader/player-bar";
import { ContentRouter } from "@/components/reader/content-router";
import { TOCPanel } from "@/components/reader/toc-panel";
import { SettingsDialog } from "@/components/settings-dialog";

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
  const tocPanelRef = useRef<HTMLDivElement>(null);
  const { settings } = useSettings();
  const [speed, setSpeed] = useState(1.0);
  const [tocItems, setTocItems] = useState<
    { id: string; title: string; depth?: number }[]
  >([]);

  // Per-item voice preference — initialized from backend item data
  const [voice, setVoice] = useState<string | null>(null);
  const voiceInitialized = useRef(false);

  // Sync voice state from backend item data on first load
  useEffect(() => {
    if (item && !voiceInitialized.current) {
      setVoice(item.voice);
      voiceInitialized.current = true;
    }
  }, [item]);

  const handleVoiceChange = useCallback(
    (voiceId: string | null) => {
      setVoice(voiceId);
      // Persist to backend (fire-and-forget, same pattern as updateProgress)
      updateVoice(id, voiceId).catch(() => {});
    },
    [id]
  );

  // Derive font size from backend-persisted settings (SettingsProvider applies
  // the CSS var globally; this local value drives the inline style for the reader)
  const fontSize = `${settings.font_size ?? "18"}px`;

  // Initialize TTS speed: per-book speed takes priority over global setting.
  // The per-book value comes from item.speed (backend), the global fallback
  // comes from settings.tts_speed. Only runs once on first load.
  const speedInitialized = useRef(false);
  useEffect(() => {
    if (speedInitialized.current) return;
    if (!item) return;

    // Per-book speed has the highest priority
    if (item.speed != null) {
      setSpeed(item.speed);
      speedInitialized.current = true;
      return;
    }

    // Fall back to global TTS speed setting
    const persisted = settings.tts_speed;
    if (persisted) {
      const parsed = parseFloat(persisted);
      if (!isNaN(parsed)) {
        setSpeed(parsed);
        speedInitialized.current = true;
      }
    }
  }, [item, settings.tts_speed]);

  const player = useTTSPlayer({
    itemId: id,
    sentences,
    speed,
    voice,
  });

  // Resolve voice label for display in the PlayerBar
  const { data: voiceData } = useVoices();
  const currentVoiceLabel = (() => {
    const effectiveId = voice ?? voiceData?.default_voice_id ?? null;
    if (!effectiveId || !voiceData) return null;
    const match = voiceData.voices.find((v) => v.voice_id === effectiveId);
    return match?.label ?? null;
  })();

  // Close TOC panel on click-outside
  useEffect(() => {
    if (!tocOpen) return;

    const handleClickOutside = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      // Ignore clicks on the TOC toggle button — let its onClick handle the toggle
      if (target.closest("[data-toc-toggle]")) return;
      if (
        tocPanelRef.current &&
        !tocPanelRef.current.contains(target)
      ) {
        setTocOpen(false);
      }
    };

    // Use mousedown so the panel closes before any other click handlers fire
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [tocOpen]);

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

  // Track latest progress values in a ref so the save callback is always fresh
  // without re-running the effect (which was causing the PATCH storm).
  const progressRef = useRef({ currentSentenceIndex: 0, sentencesLength: 0 });
  useEffect(() => {
    progressRef.current = {
      currentSentenceIndex: player.currentSentenceIndex,
      sentencesLength: sentences.length,
    };
  }, [player.currentSentenceIndex, sentences.length]);

  // Track current chapter for EPUB/PDF chapter position persistence.
  // Updated when the active sentence changes — finds the chapter element
  // closest to the current scroll position.
  const currentChapterRef = useRef<string | null>(null);
  useEffect(() => {
    if (!item || (item.type !== "epub" && item.type !== "pdf") || tocItems.length === 0) return;

    // Find which chapter element contains the active sentence
    const sentenceEl = document.querySelector(
      `[data-sentence-id="${player.currentSentenceIndex}"]`
    );
    if (!sentenceEl) return;

    const chapterEl = sentenceEl.closest("[data-chapter]");
    if (chapterEl) {
      const chapterId = chapterEl.getAttribute("data-chapter");
      if (chapterId) currentChapterRef.current = chapterId;
    }
  }, [player.currentSentenceIndex, item, tocItems.length]);

  // Progress + chapter persistence — only fires on page unload / component unmount
  useEffect(() => {
    const saveProgress = () => {
      const { currentSentenceIndex, sentencesLength } = progressRef.current;
      if (sentencesLength === 0) return;
      const progress = Math.round(
        (currentSentenceIndex / sentencesLength) * 100
      );
      updateProgress(id, progress).catch(() => {});

      // Persist current EPUB chapter position alongside progress
      if (currentChapterRef.current) {
        updateChapter(id, currentChapterRef.current).catch(() => {});
      }
    };

    window.addEventListener("beforeunload", saveProgress);
    return () => {
      window.removeEventListener("beforeunload", saveProgress);
      saveProgress(); // save once on unmount
    };
  }, [id]);

  // Restore reading position when sentences are extracted and item has progress.
  // Calculates the target sentence index from progress percentage and scrolls
  // to that position without auto-playing. Uses a ref to prevent re-triggering.
  const progressRestored = useRef(false);
  useEffect(() => {
    if (progressRestored.current) return;
    if (!item || sentences.length === 0) return;

    // For EPUB/PDF with a saved chapter, scroll to the chapter element first
    if ((item.type === "epub" || item.type === "pdf") && item.current_chapter) {
      const chapterEl = document.querySelector(
        `[data-chapter="${item.current_chapter}"]`
      );
      if (chapterEl) {
        chapterEl.scrollIntoView({ behavior: "instant", block: "start" });
      }
    }

    // Restore sentence-level position from progress percentage
    if (item.progress > 0 && item.progress < 100) {
      const restoredIndex = Math.round(
        (item.progress / 100) * (sentences.length - 1)
      );
      if (restoredIndex > 0) {
        player.seekToSentence(restoredIndex);

        // Scroll to the restored sentence after a short delay to let the DOM settle
        // (especially for EPUBs where chapter scroll may still be animating)
        requestAnimationFrame(() => {
          const el = document.querySelector(
            `[data-sentence-id="${restoredIndex}"]`
          );
          if (el) {
            el.scrollIntoView({ behavior: "instant", block: "center" });
          }
        });
      }
    }

    progressRestored.current = true;
  }, [item, sentences, player]);

  const handleSentencesExtracted = useCallback(
    (extracted: Sentence[]) => {
      setSentences(extracted);
    },
    []
  );

  const handleChaptersExtracted = useCallback(
    (chapters: { id: string; title: string; depth?: number }[]) => {
      setTocItems(chapters);
    },
    []
  );

  const handleTocItemClick = useCallback(
    (index: number) => {
      const item = tocItems[index];
      if (!item) return;

      // Support "chapterId#anchor" format from navToc entries
      const [chapterId, anchor] = item.id.split("#");
      const el = anchor
        ? document.querySelector(`[data-chapter="${chapterId}"] [id="${anchor}"]`)
        : document.querySelector(`[data-chapter="${chapterId}"]`);
      if (el) {
        el.scrollIntoView({ behavior: "smooth", block: "start" });
      }
    },
    [tocItems]
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

      <SettingsDialog
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />

      <div className="flex pt-12 h-screen overflow-hidden">
        {/* TOC as floating popover — click outside to dismiss */}
        {tocOpen && (
          <div
            ref={tocPanelRef}
            className="fixed left-3 top-14 z-30"
          >
            <TOCPanel
              items={tocItems}
              onItemClick={handleTocItemClick}
            />
          </div>
        )}

        <div className="flex-1 flex justify-center overflow-y-auto">
          <div
            className="max-w-prose w-full px-6 pt-8 pb-24"
            style={{ fontSize }}
          >
            <ContentRouter
              item={item}
              onSentencesExtracted={handleSentencesExtracted}
              currentSentenceIndex={player.currentSentenceIndex}
              currentWordProgress={player.currentWordProgress}
              onSentenceClick={player.playFromSentence}
              onChaptersExtracted={handleChaptersExtracted}
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
          // Persist per-book speed (fire-and-forget)
          updateSpeed(id, s).catch(() => {});
        }}
        totalSentences={sentences.length}
        voice={voice}
        onVoiceChange={handleVoiceChange}
        voiceLabel={currentVoiceLabel}
      />
    </>
  );
}
