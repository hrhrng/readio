"use client";

import {
  use,
  useCallback,
  useEffect,
  useReducer,
  useRef,
  useState,
} from "react";
import { useLibraryItem, useSettings, useVoices } from "@/lib/hooks";
import { updateProgress, updateVoice, updateSpeed, updateChapter } from "@/lib/api";
import { Sentence } from "@/lib/types";
import { useTTSPlayer } from "@/lib/use-tts-player";
import { useTextAdvancePlayer } from "@/lib/use-text-advance-player";
import { TopBar } from "@/components/reader/top-bar";
import { PlayerBar } from "@/components/reader/player-bar";
import { ContentRouter } from "@/components/reader/content-router";
import { ReaderHighlightProvider } from "@/components/reader/content-blocks";
import {
  TOCPanel,
  type TOCItem,
} from "@/components/reader/toc-panel";
import { SettingsDialog } from "@/components/settings-dialog";
import { Sheet } from "@/components/sheet";
import { ReaderToolsInspector } from "@/components/reader/reader-tools-inspector";
import { resolveReaderHighlight } from "@/lib/reader-highlight";
import { webRustCore } from "@/lib/readio-core-runtime";
import {
  reduceReaderOverlay,
  shouldOpenReaderSearch,
} from "@/lib/reader-tools-navigation";
import type {
  HighlightTarget,
  ReaderIR,
  ReadingPace,
} from "@/lib/rust-core";
import {
  resolveInitialPlaybackRate,
  resolveStoredReadingPace,
} from "@/lib/settings-utils";
import { transitionReadingMode } from "@/lib/reading-mode-transition";
import { ReaderProgressCommitter } from "@/lib/reader-progress";
import { ReaderProgressRestoreGate } from "@/lib/reader-progress-restore";
import { resolveSentenceProgressSeek } from "@/lib/reading-progress-adapter";
import { useWebMediaSession } from "@/lib/use-web-media-session";
import { resolvePlayerItemId } from "@/lib/player-route";
import { useLocale } from "@/components/locale-provider";

export default function PlayerPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { t } = useLocale();
  const { id: routeId } = use(params);
  const id = resolvePlayerItemId(routeId);
  const { data: item, isLoading } = useLibraryItem(id);
  const [sentences, setSentences] = useState<Sentence[]>([]);
  const [readerSource, setReaderSource] = useState<{
    itemId: string;
    reader: ReaderIR;
  } | null>(null);
  const readerIR = readerSource?.itemId === id ? readerSource.reader : null;
  const readerDocument = readerIR?.document ?? null;
  const readerTextIndex = readerIR?.text_index ?? null;
  const [highlightTarget, setHighlightTarget] = useState<HighlightTarget | null>(null);
  const readerDocumentError = null;
  const [readerOverlay, dispatchReaderOverlay] = useReducer(
    reduceReaderOverlay,
    { tocOpen: false, activeTool: null }
  );
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { settings, isLoading: settingsLoading } = useSettings();
  const [speed, setSpeed] = useState(1.0);
  const [readingPace, setReadingPace] = useState<ReadingPace | null>(null);
  const [positionReady, setPositionReady] = useState(false);
  const progressCommitterRef = useRef<ReaderProgressCommitter | null>(null);
  const pendingProgressSentenceRef = useRef<number | null>(null);
  const [tocItems, setTocItems] = useState<TOCItem[]>([]);

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

  // A per-book override wins; otherwise the shared Rust effort ladder supplies
  // the audio rate. Waiting for local settings avoids briefly locking in 1×.
  const speedInitialized = useRef(false);
  useEffect(() => {
    if (speedInitialized.current) return;
    if (!item) return;
    if (item.speed == null && settingsLoading) return;
    let active = true;
    void resolveInitialPlaybackRate(item.speed, settings).then((rate) => {
      if (!active || speedInitialized.current) return;
      setSpeed(rate);
      speedInitialized.current = true;
    });
    return () => {
      active = false;
    };
  }, [item, settings, settingsLoading]);

  useEffect(() => {
    if (settingsLoading) return;
    let active = true;
    void resolveStoredReadingPace(settings).then((pace) => {
      if (active) setReadingPace(pace);
    });
    return () => {
      active = false;
    };
  }, [settings, settingsLoading]);

  const persistSentenceProgress = useCallback(async (sentence: number) => {
    const committer = progressCommitterRef.current;
    if (!committer) {
      pendingProgressSentenceRef.current = sentence;
      return;
    }
    await committer.commitSentence(sentence);
  }, []);
  const audioPlayer = useTTSPlayer({
    itemId: id,
    sentences,
    speed,
    voice,
    language: item?.locale ?? "en",
    onPersistProgress: persistSentenceProgress,
  });
  const readingMode = readingPace?.mode ?? "manual";
  const activeReadingMode = positionReady ? readingMode : "manual";
  const effortRate = readingPace?.audio_playback_rate ?? 1;
  const textCharactersPerSecond =
    ((readingPace?.text_characters_per_second ?? 46) / effortRate) * speed;
  const textPlayer = useTextAdvancePlayer({
    sentences,
    charactersPerSecond: textCharactersPerSecond,
    enabled: activeReadingMode === "auto",
    onPersistProgress: persistSentenceProgress,
  });
  const player = activeReadingMode === "read_aloud" ? audioPlayer : textPlayer;
  const seekTextToSentence = textPlayer.seekToSentence;
  useWebMediaSession({
    enabled: positionReady && readingMode === "read_aloud",
    title: item?.title ?? "Readio",
    artwork: item?.cover_image ?? null,
    player: audioPlayer,
  });

  const previousReadingMode = useRef(activeReadingMode);
  useEffect(() => {
    const previous = previousReadingMode.current;
    transitionReadingMode(
      previous,
      activeReadingMode,
      {
        currentSentenceIndex: audioPlayer.currentSentenceIndex,
        pause: audioPlayer.pause,
        playFromSentence: audioPlayer.playFromSentence,
        seekToSentence: audioPlayer.seekToSentence,
      },
      {
        currentSentenceIndex: textPlayer.currentSentenceIndex,
        pause: textPlayer.pause,
        playFromSentence: textPlayer.playFromSentence,
        seekToSentence: seekTextToSentence,
      }
    );
    previousReadingMode.current = activeReadingMode;
  }, [
    activeReadingMode,
    audioPlayer.currentSentenceIndex,
    audioPlayer.pause,
    audioPlayer.playFromSentence,
    audioPlayer.seekToSentence,
    seekTextToSentence,
    textPlayer.currentSentenceIndex,
    textPlayer.pause,
    textPlayer.playFromSentence,
  ]);

  useEffect(() => {
    let cancelled = false;
    if (!readerDocument || !readerTextIndex) {
      setHighlightTarget(null);
      return () => {
        cancelled = true;
      };
    }
    void resolveReaderHighlight(
      webRustCore,
      readerDocument,
      readerTextIndex,
      player.currentSentenceIndex,
      player.currentWordProgress
    )
      .then((target) => {
        if (!cancelled) setHighlightTarget(target);
      })
      .catch(() => {
        if (!cancelled) setHighlightTarget(null);
      });
    return () => {
      cancelled = true;
    };
  }, [
    player.currentSentenceIndex,
    player.currentWordProgress,
    readerDocument,
    readerTextIndex,
  ]);

  useEffect(() => {
    if (!readerDocument) {
      progressCommitterRef.current = null;
      return;
    }
    const committer = new ReaderProgressCommitter({
      itemId: id,
      document: readerDocument,
      core: webRustCore,
    });
    progressCommitterRef.current = committer;
    const pendingSentence = pendingProgressSentenceRef.current;
    pendingProgressSentenceRef.current = null;
    if (pendingSentence !== null) {
      void committer.commitSentence(pendingSentence).catch(() => {
        // The player hook reports future persistence failures without stopping
        // its Rust playback/text deadline loop.
      });
    }
    return () => {
      if (progressCommitterRef.current === committer) {
        progressCommitterRef.current = null;
      }
    };
  }, [id, readerDocument]);

  // Resolve voice label for display in the PlayerBar
  const { data: voiceData } = useVoices();
  const currentVoiceLabel = (() => {
    const effectiveId = voice ?? voiceData?.default_voice_id ?? null;
    if (!effectiveId || !voiceData) return null;
    const match = voiceData.voices.find((v) => v.voice_id === effectiveId);
    return match?.label ?? null;
  })();

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const isEditable =
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        (e.target instanceof HTMLElement && e.target.isContentEditable);
      if (
        shouldOpenReaderSearch({
          key: e.key,
          metaKey: e.metaKey,
          ctrlKey: e.ctrlKey,
          isEditable,
        })
      ) {
        e.preventDefault();
        dispatchReaderOverlay({ type: "open-tool", tool: "search" });
        return;
      }
      if (isEditable) return;

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

  const jumpToSentence = useCallback(
    (index: number) => {
      player.seekToSentence(index);
      requestAnimationFrame(() => {
        document
          .querySelector(`[data-sentence-id="${index}"]`)
          ?.scrollIntoView({ behavior: "smooth", block: "center" });
      });
    },
    [player]
  );

  // Track latest progress values in a ref so the save callback is always fresh
  // without re-running the effect (which was causing the PATCH storm).
  const progressRef = useRef({ progressBasisPoints: 0 });
  useEffect(() => {
    progressRef.current = {
      progressBasisPoints: player.progressBasisPoints,
    };
  }, [player.progressBasisPoints]);

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
      const { progressBasisPoints } = progressRef.current;
      const progress = Math.round(progressBasisPoints / 100);
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

  // Restore the exact Rust scalar cursor before activating Auto/read-aloud. The
  // legacy percentage remains a one-way fallback for pre-Replica libraries.
  const progressRestoreGate = useRef(new ReaderProgressRestoreGate());
  useEffect(() => {
    if (!item || !readerDocument || sentences.length === 0) return;
    const gate = progressRestoreGate.current;
    const attempt = gate.begin(
      `${id}:${readerDocument.revision?.revision_digest ?? "unversioned"}`
    );
    if (attempt === null) return;
    let cancelled = false;

    void (async () => {
      // For EPUB/PDF with a saved chapter, scroll to the chapter element first.
      if ((item.type === "epub" || item.type === "pdf") && item.current_chapter) {
        document
          .querySelector(`[data-chapter="${item.current_chapter}"]`)
          ?.scrollIntoView({ behavior: "instant", block: "start" });
      }

      let restoredIndex: number | null = null;
      try {
        restoredIndex =
          (await progressCommitterRef.current?.restoreSentence()) ?? null;
      } catch {
        // A damaged/missing local cursor falls through to the compatibility
        // percentage; it must never prevent the reader from becoming usable.
      }
      if (restoredIndex == null && item.progress > 0 && item.progress < 100) {
        restoredIndex = (
          await resolveSentenceProgressSeek(
            webRustCore,
            sentences,
            item.progress / 100
          )
        ).segment;
      }
      if (cancelled) return;
      if (restoredIndex !== null && restoredIndex > 0) {
        // Text is the neutral/manual cursor owner while positionReady is false.
        // Activating the selected mode then transfers this cursor exactly once.
        seekTextToSentence(restoredIndex);

        requestAnimationFrame(() => {
          const el = document.querySelector(
            `[data-sentence-id="${restoredIndex}"]`
          );
          if (el) {
            el.scrollIntoView({ behavior: "instant", block: "center" });
          }
        });
      }
      setPositionReady(true);
      gate.complete(attempt);
    })();

    return () => {
      cancelled = true;
      gate.cancel(attempt);
    };
  }, [id, item, readerDocument, seekTextToSentence, sentences]);

  const handleSentencesExtracted = useCallback(
    (extracted: Sentence[]) => {
      setSentences(extracted);
    },
    []
  );

  const handleReaderIRExtracted = useCallback((reader: ReaderIR) => {
    setReaderSource({ itemId: id, reader });
  }, [id]);

  const handleChaptersExtracted = useCallback(
    (chapters: TOCItem[]) => {
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
      <main className="reader-shell">
        <div className="reader-scroll">
          <div className="reader-content reader-loading">
            <div className="skeleton reader-loading-title" />
            <div className="skeleton reader-loading-meta" />
            <div className="reader-loading-copy">
              {Array.from({ length: 10 }, (_, index) => (
                <div
                  key={index}
                  className={`skeleton skeleton-line ${
                    index % 4 === 3 ? "skeleton-line--short" : ""
                  }`}
                />
              ))}
            </div>
          </div>
        </div>
      </main>
    );
  }

  if (!item) {
    return (
      <main className="reader-shell ui-empty-state">
        <h1 className="ui-state-title">{t("reader.itemNotFound")}</h1>
        <p className="ui-state-description">
          {t("reader.itemNotFoundDescription")}
        </p>
      </main>
    );
  }

  return (
    <main className="reader-shell">
      <TopBar
        title={item.title}
        onToggleToc={() => dispatchReaderOverlay({ type: "toggle-toc" })}
        onToggleSettings={() => {
          dispatchReaderOverlay({ type: "close-all" });
          setSettingsOpen(true);
        }}
        onOpenTool={(tool) =>
          dispatchReaderOverlay({ type: "open-tool", tool })
        }
        activeTool={readerOverlay.activeTool}
        tocOpen={readerOverlay.tocOpen}
      />

      <Sheet
        open={readerOverlay.tocOpen}
        title={t("reader.toc")}
        onClose={() => dispatchReaderOverlay({ type: "toggle-toc" })}
      >
        <TOCPanel
          items={tocItems}
          currentSentenceIndex={player.currentSentenceIndex}
          onNavigate={jumpToSentence}
          onItemClick={handleTocItemClick}
        />
      </Sheet>

      <Sheet
        open={readerOverlay.activeTool !== null}
        title={t("reader.tools")}
        onClose={() => dispatchReaderOverlay({ type: "close-tools" })}
      >
        <ReaderToolsInspector
          activeTool={readerOverlay.activeTool ?? "search"}
          onToolChange={(tool) =>
            dispatchReaderOverlay({ type: "open-tool", tool })
          }
          itemId={id}
          document={readerDocument}
          textIndex={readerTextIndex}
          documentError={readerDocumentError}
          currentSentenceIndex={player.currentSentenceIndex}
          onJumpToSentence={jumpToSentence}
        />
      </Sheet>

      <SettingsDialog
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />

      <div className="reader-scroll">
          <article className="reader-content" style={{ fontSize }}>
            <ReaderHighlightProvider target={highlightTarget}>
              <ContentRouter
                item={item}
                onSentencesExtracted={handleSentencesExtracted}
                onReaderIRExtracted={handleReaderIRExtracted}
                currentSentenceIndex={player.currentSentenceIndex}
                currentWordProgress={player.currentWordProgress}
                onSentenceClick={player.playFromSentence}
                onChaptersExtracted={handleChaptersExtracted}
              />
            </ReaderHighlightProvider>
          </article>
      </div>

      <PlayerBar
        player={player}
        mode={readingMode}
        speed={speed}
        onSpeedChange={(s) => {
          setSpeed(s);
          player.setSpeed(s);
          // Persist per-book speed (fire-and-forget)
          updateSpeed(id, s).catch(() => {});
        }}
        voice={voice}
        onVoiceChange={handleVoiceChange}
        voiceLabel={currentVoiceLabel}
      />
    </main>
  );
}
