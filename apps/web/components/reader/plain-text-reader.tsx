"use client";

import { useEffect, useMemo, useRef, memo } from "react";
import { extractSentencesFromText } from "@/lib/sentences";
import { Sentence } from "@/lib/types";
import { tokenize } from "@/lib/cjk";

// ---------------------------------------------------------------------------
// PlainTextSentenceSpan — per-sentence rendering with word-level highlighting.
//
// Wrapped in React.memo so inactive sentences skip re-renders during TTS
// playback (ontimeupdate fires 4-10x/sec, but only 1 sentence is active).
// ---------------------------------------------------------------------------

const PlainTextSentenceSpan = memo(function PlainTextSentenceSpan({
  sentence,
  isActive,
  wordProgress,
  onClick,
}: {
  sentence: Sentence;
  isActive: boolean;
  wordProgress: number;
  onClick: (index: number) => void;
}) {
  const words = useMemo(() => tokenize(sentence.text), [sentence.text]);
  const wordCount = useMemo(
    () => words.filter((w) => w.trim()).length,
    [words]
  );
  const highlightedWordIdx = isActive
    ? Math.floor(wordProgress * wordCount)
    : -1;

  let wordIdx = 0;
  return (
    <span
      data-sentence-id={sentence.index}
      onClick={() => onClick(sentence.index)}
      className={`cursor-pointer transition-colors duration-200 rounded-sm ${
        isActive ? "bg-highlight-sentence" : "hover:bg-surface-hover"
      }`}
    >
      {words.map((word, wIdx) => {
        if (!word.trim()) {
          return <span key={wIdx}>{word}</span>;
        }
        const thisWordIdx = wordIdx++;
        const isHighlightedWord = isActive && thisWordIdx === highlightedWordIdx;
        return (
          <span
            key={wIdx}
            className={
              isHighlightedWord ? "text-highlight-word font-semibold" : ""
            }
          >
            {word}
          </span>
        );
      })}
    </span>
  );
}, (prevProps, nextProps) => {
  // onClick is ref-stable (useTTSPlayer stores mutable values in refs),
  // so no identity check needed here.
  if (!prevProps.isActive && !nextProps.isActive) return true;   // both inactive → skip
  if (prevProps.isActive !== nextProps.isActive) return false;    // active state changed → re-render
  return prevProps.wordProgress === nextProps.wordProgress;       // both active → compare progress
});

interface PlainTextReaderProps {
  content: string;
  onSentencesExtracted: (sentences: Sentence[]) => void;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
}

export function PlainTextReader({
  content,
  onSentencesExtracted,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
}: PlainTextReaderProps) {
  const extractedRef = useRef(false);

  const { paragraphs, sentences } = useMemo(() => {
    const allSentences: Sentence[] = [];
    const rawParagraphs = content.split(/\n\n+/).filter((p) => p.trim());
    const parags = rawParagraphs.map((para) => {
      const paraText = para.trim();
      const paraSentences = extractSentencesFromText(paraText);
      const offsetSentences = paraSentences.map((s) => ({
        ...s,
        index: allSentences.length + s.index,
      }));
      allSentences.push(...offsetSentences);
      return offsetSentences;
    });
    return { paragraphs: parags, sentences: allSentences };
  }, [content]);

  useEffect(() => {
    if (!extractedRef.current && sentences.length > 0) {
      extractedRef.current = true;
      onSentencesExtracted(sentences);
    }
  }, [sentences, onSentencesExtracted]);

  // Auto-scroll to current sentence
  useEffect(() => {
    const el = document.querySelector(
      `[data-sentence-id="${currentSentenceIndex}"]`
    );
    if (el) {
      el.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, [currentSentenceIndex]);

  return (
    <div className="space-y-6">
      {paragraphs.map((paraSentences, pIdx) => (
        <p key={pIdx} className="text-lg leading-relaxed text-text-primary">
          {paraSentences.map((sentence) => (
            <PlainTextSentenceSpan
              key={sentence.index}
              sentence={sentence}
              isActive={sentence.index === currentSentenceIndex}
              wordProgress={currentWordProgress}
              onClick={onSentenceClick}
            />
          ))}
        </p>
      ))}
    </div>
  );
}
