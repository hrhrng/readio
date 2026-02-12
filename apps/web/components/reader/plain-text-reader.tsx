"use client";

import { useEffect, useMemo, useRef } from "react";
import { extractSentencesFromText } from "@/lib/sentences";
import { Sentence } from "@/lib/types";

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
          {paraSentences.map((sentence) => {
            const isActive = sentence.index === currentSentenceIndex;
            const words = sentence.text.split(/(\s+)/);
            const wordCount = words.filter((w) => w.trim()).length;
            const highlightedWordIdx = isActive
              ? Math.floor(currentWordProgress * wordCount)
              : -1;

            let wordIdx = 0;
            return (
              <span
                key={sentence.index}
                data-sentence-id={sentence.index}
                onClick={() => onSentenceClick(sentence.index)}
                className={`cursor-pointer transition-colors duration-200 rounded-sm ${
                  isActive
                    ? "bg-highlight-sentence"
                    : "hover:bg-surface-hover"
                }`}
              >
                {words.map((word, wIdx) => {
                  if (!word.trim()) {
                    return (
                      <span key={wIdx}>{word}</span>
                    );
                  }
                  const thisWordIdx = wordIdx++;
                  const isHighlightedWord =
                    isActive && thisWordIdx === highlightedWordIdx;
                  return (
                    <span
                      key={wIdx}
                      className={
                        isHighlightedWord
                          ? "text-highlight-word font-semibold"
                          : ""
                      }
                    >
                      {word}
                    </span>
                  );
                })}
              </span>
            );
          })}
        </p>
      ))}
    </div>
  );
}
