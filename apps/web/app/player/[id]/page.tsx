"use client";

import { use, useCallback, useEffect, useRef, useState } from "react";
import { useLibraryItem, useVoices } from "@/lib/hooks";
import { updateProgress, updateVoice } from "@/lib/api";
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
    { id: string; title: string; depth?: number }[]
  >([]);
  const [fontSize, setFontSize] = useState("18px");

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

  // Load font size from localStorage and listen for changes
  useEffect(() => {
    const sizeMap: Record<string, string> = {
      small: "16px",
      medium: "18px",
      large: "22px",
    };

    const applyFontSize = () => {
      const current = localStorage.getItem("readio-font-size");
      if (current && current in sizeMap) {
        setFontSize(sizeMap[current]);
      }
    };

    // Apply saved value on mount
    applyFontSize();

    // Cross-tab changes fire "storage"; same-tab changes fire "readio-font-change"
    window.addEventListener("storage", applyFontSize);
    window.addEventListener("readio-font-change", applyFontSize);
    return () => {
      window.removeEventListener("storage", applyFontSize);
      window.removeEventListener("readio-font-change", applyFontSize);
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

  // Track latest progress values in a ref so the save callback is always fresh
  // without re-running the effect (which was causing the PATCH storm).
  const progressRef = useRef({ currentSentenceIndex: 0, sentencesLength: 0 });
  useEffect(() => {
    progressRef.current = {
      currentSentenceIndex: player.currentSentenceIndex,
      sentencesLength: sentences.length,
    };
  }, [player.currentSentenceIndex, sentences.length]);

  // Progress persistence — only fires on page unload / component unmount
  useEffect(() => {
    const saveProgress = () => {
      const { currentSentenceIndex, sentencesLength } = progressRef.current;
      if (sentencesLength === 0) return;
      const progress = Math.round(
        (currentSentenceIndex / sentencesLength) * 100
      );
      updateProgress(id, progress).catch(() => {});
    };

    window.addEventListener("beforeunload", saveProgress);
    return () => {
      window.removeEventListener("beforeunload", saveProgress);
      saveProgress(); // save once on unmount
    };
  }, [id]);

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

      {settingsOpen && (
        <SettingsPanel onClose={() => setSettingsOpen(false)} />
      )}

      <div className="flex pt-12 h-screen overflow-hidden">
        {/* TOC as fixed overlay — avoids reflowing the content area */}
        {tocOpen && (
          <div className="fixed left-0 top-12 bottom-[80px] z-30">
            <TOCPanel
              items={tocItems}
              onClose={() => setTocOpen(false)}
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
        voice={voice}
        onVoiceChange={handleVoiceChange}
        voiceLabel={currentVoiceLabel}
      />
    </>
  );
}
