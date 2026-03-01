"use client";

/**
 * Shared content-block types and rendering components.
 *
 * Shared content-block types, sentence splitting, and rendering components.
 * Each format reader (EpubReader, HtmlReader, etc.) reuses these for
 * sentence-level TTS highlighting, word-level progress, and rich rendering.
 */

import { useMemo, memo } from "react";
import { BOUNDARY } from "@/lib/sentences";
import { tokenize, countTokens } from "@/lib/cjk";

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/** A run of text with inline formatting (bold / italic) and optional hyperlink. */
export interface StyledRun {
  text: string;
  bold?: boolean;
  italic?: boolean;
  href?: string;
}

/** A sentence with its global index, plain text (for TTS), and styled runs (for rendering). */
export interface RichSentence {
  index: number;
  text: string;
  runs: StyledRun[];
}

/** Union type representing a single content block. */
export type ContentBlock =
  | { type: "heading"; level: number; sentences: RichSentence[]; anchorId?: string }
  | { type: "paragraph"; sentences: RichSentence[]; anchorId?: string }
  | { type: "image"; src: string; alt?: string }
  | {
      type: "list";
      ordered: boolean;
      items: { sentences: RichSentence[] }[];
      anchorId?: string;
    }
  | { type: "blockquote"; sentences: RichSentence[]; anchorId?: string }
  | { type: "code"; text: string; language?: string }
  | { type: "separator" };

// ---------------------------------------------------------------------------
// Sentence splitting — uses shared BOUNDARY regex from lib/sentences.ts
// ---------------------------------------------------------------------------

/**
 * Split an array of StyledRuns into RichSentences by finding sentence
 * boundaries in the concatenated text, then slicing runs at those offsets.
 */
export function splitRunsIntoSentences(
  runs: StyledRun[],
  startIndex: number
): { sentences: RichSentence[]; nextIndex: number } {
  const fullText = runs.map((r) => r.text).join("");
  if (!fullText.trim()) return { sentences: [], nextIndex: startIndex };

  const parts = fullText.split(BOUNDARY).filter((s) => s.length > 0);

  const sentences: RichSentence[] = [];
  let charCursor = 0;
  let runIdx = 0;
  let runOffset = 0;
  let sentenceIdx = startIndex;

  for (const part of parts) {
    if (!part.trim()) {
      charCursor += part.length;
      let toSkip = part.length;
      while (toSkip > 0 && runIdx < runs.length) {
        const available = runs[runIdx].text.length - runOffset;
        if (toSkip >= available) {
          toSkip -= available;
          runIdx++;
          runOffset = 0;
        } else {
          runOffset += toSkip;
          toSkip = 0;
        }
      }
      continue;
    }

    const sentenceRuns: StyledRun[] = [];
    let remaining = part.length;

    const expectedOffset = fullText.indexOf(part, charCursor);
    if (expectedOffset > charCursor) {
      let gap = expectedOffset - charCursor;
      while (gap > 0 && runIdx < runs.length) {
        const available = runs[runIdx].text.length - runOffset;
        if (gap >= available) {
          gap -= available;
          runIdx++;
          runOffset = 0;
        } else {
          runOffset += gap;
          gap = 0;
        }
      }
      charCursor = expectedOffset;
    }

    while (remaining > 0 && runIdx < runs.length) {
      const run = runs[runIdx];
      const available = run.text.length - runOffset;
      const take = Math.min(remaining, available);
      const slice = run.text.substring(runOffset, runOffset + take);

      sentenceRuns.push({
        text: slice,
        ...(run.bold ? { bold: true } : {}),
        ...(run.italic ? { italic: true } : {}),
        ...(run.href ? { href: run.href } : {}),
      });

      remaining -= take;
      runOffset += take;
      if (runOffset >= run.text.length) {
        runIdx++;
        runOffset = 0;
      }
    }

    charCursor += part.length;

    sentences.push({
      index: sentenceIdx,
      text: part.trim(),
      runs: sentenceRuns,
    });
    sentenceIdx++;
  }

  return { sentences, nextIndex: sentenceIdx };
}

// ---------------------------------------------------------------------------
// SentenceSpan — renders a single sentence with per-word highlighting
// ---------------------------------------------------------------------------

/**
 * SentenceSpan — renders a single sentence with per-word highlighting.
 *
 * Wrapped in React.memo with a custom comparator to avoid re-rendering
 * inactive sentences during TTS playback (ontimeupdate fires 4-10x/sec).
 */
export const SentenceSpan = memo(function SentenceSpan({
  sentence,
  isActive,
  wordProgress,
  onClick,
  onLinkClick,
}: {
  sentence: RichSentence;
  isActive: boolean;
  wordProgress: number;
  onClick: (index: number) => void;
  onLinkClick?: (href: string) => void;
}) {
  const wordCount = useMemo(
    () => countTokens(sentence.text),
    [sentence.text]
  );
  const highlightedWordIdx = isActive
    ? Math.min(Math.floor(wordProgress * wordCount), wordCount - 1)
    : -1;

  let globalWordIdx = 0;

  const isExternal = (href: string) =>
    /^(https?:|mailto:)/i.test(href);

  return (
    <span
      data-sentence-id={sentence.index}
      onClick={() => onClick(sentence.index)}
      className={`cursor-pointer transition-colors duration-200 rounded-sm ${
        isActive ? "bg-highlight-sentence" : "hover:bg-surface-hover"
      }`}
    >
      {sentence.runs.map((run, ri) => {
        const tokens = tokenize(run.text);

        const inner = tokens.map((token, ti) => {
          if (!token.trim()) {
            return <span key={ti}>{token}</span>;
          }
          const thisIdx = globalWordIdx++;
          const isHighlighted = isActive && thisIdx === highlightedWordIdx;
          return (
            <span
              key={ti}
              className={
                isHighlighted
                  ? "bg-highlight-word-bg rounded-sm"
                  : ""
              }
            >
              {token}
            </span>
          );
        });

        const styleClass = `${run.bold ? "font-bold" : ""} ${run.italic ? "italic" : ""}`.trim();

        if (run.href) {
          const external = isExternal(run.href);
          return (
            <a
              key={ri}
              href={run.href}
              className={`${styleClass} text-blue-500 underline decoration-blue-300 hover:text-blue-600 hover:decoration-blue-500`.trim()}
              onClick={(e) => {
                e.stopPropagation();
                if (external) return;
                e.preventDefault();
                onLinkClick?.(run.href!);
              }}
              {...(external
                ? { target: "_blank", rel: "noopener noreferrer" }
                : {})}
            >
              {inner}
            </a>
          );
        }

        return (
          <span key={ri} className={styleClass || undefined}>
            {inner}
          </span>
        );
      })}
    </span>
  );
}, (prevProps, nextProps) => {
  if (!prevProps.isActive && !nextProps.isActive) return true;
  if (prevProps.isActive !== nextProps.isActive) return false;
  return prevProps.wordProgress === nextProps.wordProgress;
});

// ---------------------------------------------------------------------------
// Block renderers
// ---------------------------------------------------------------------------

export function renderSentences(
  sentences: RichSentence[],
  currentSentenceIndex: number,
  currentWordProgress: number,
  onSentenceClick: (index: number) => void,
  onLinkClick?: (href: string) => void
) {
  return sentences.map((s) => (
    <SentenceSpan
      key={s.index}
      sentence={s}
      isActive={s.index === currentSentenceIndex}
      wordProgress={currentWordProgress}
      onClick={onSentenceClick}
      onLinkClick={onLinkClick}
    />
  ));
}

/**
 * Returns the [min, max] sentence index range for a content block,
 * or null if the block contains no sentences (image/separator/code).
 */
export function getBlockSentenceRange(block: ContentBlock): [number, number] | null {
  if (block.type === "image" || block.type === "separator" || block.type === "code") return null;

  let min = Infinity;
  let max = -Infinity;

  const scan = (sentences: RichSentence[]) => {
    for (const s of sentences) {
      if (s.index < min) min = s.index;
      if (s.index > max) max = s.index;
    }
  };

  if (block.type === "list") {
    for (const item of block.items) scan(item.sentences);
  } else {
    scan(block.sentences);
  }

  return min <= max ? [min, max] : null;
}

/**
 * BlockRenderer — renders a single content block (heading, paragraph, list, etc.).
 *
 * Wrapped in React.memo so blocks not containing the active sentence
 * are skipped entirely during TTS playback updates.
 */
export const BlockRenderer = memo(function BlockRenderer({
  block,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
  onLinkClick,
}: {
  block: ContentBlock;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
  onLinkClick?: (href: string) => void;
}) {
  switch (block.type) {
    case "heading": {
      const sizeClass =
        block.level <= 1
          ? "text-2xl font-bold mb-3"
          : block.level === 2
            ? "text-xl font-semibold mb-3"
            : "text-lg font-medium mb-2";
      const className = `${sizeClass} text-text-primary font-serif`;
      const children = renderSentences(
        block.sentences,
        currentSentenceIndex,
        currentWordProgress,
        onSentenceClick,
        onLinkClick
      );
      if (block.level <= 1) return <h1 id={block.anchorId} className={className}>{children}</h1>;
      if (block.level === 2) return <h2 id={block.anchorId} className={className}>{children}</h2>;
      if (block.level === 3) return <h3 id={block.anchorId} className={className}>{children}</h3>;
      if (block.level === 4) return <h4 id={block.anchorId} className={className}>{children}</h4>;
      if (block.level === 5) return <h5 id={block.anchorId} className={className}>{children}</h5>;
      return <h6 id={block.anchorId} className={className}>{children}</h6>;
    }

    case "paragraph":
      return (
        <p id={block.anchorId} className="mb-4 leading-relaxed text-text-primary">
          {renderSentences(
            block.sentences,
            currentSentenceIndex,
            currentWordProgress,
            onSentenceClick,
            onLinkClick
          )}
        </p>
      );

    case "image":
      return (
        <img
          src={block.src}
          alt={block.alt || ""}
          className="max-w-full h-auto rounded-lg my-4"
        />
      );

    case "list": {
      const ListTag = block.ordered ? "ol" : "ul";
      const listClass = block.ordered
        ? "list-decimal pl-6 mb-4"
        : "list-disc pl-6 mb-4";
      return (
        <ListTag id={block.anchorId} className={listClass}>
          {block.items.map((item, i) => (
            <li key={i} className="mb-1 leading-relaxed text-text-primary">
              {renderSentences(
                item.sentences,
                currentSentenceIndex,
                currentWordProgress,
                onSentenceClick,
                onLinkClick
              )}
            </li>
          ))}
        </ListTag>
      );
    }

    case "blockquote":
      return (
        <blockquote id={block.anchorId} className="border-l-4 border-border pl-4 italic text-text-secondary mb-4">
          {renderSentences(
            block.sentences,
            currentSentenceIndex,
            currentWordProgress,
            onSentenceClick,
            onLinkClick
          )}
        </blockquote>
      );

    case "code":
      return (
        <pre className="bg-surface-hover rounded-lg p-4 mb-4 overflow-x-auto text-sm font-mono text-text-primary">
          <code>{block.text}</code>
        </pre>
      );

    case "separator":
      return <hr className="border-border my-4" />;
  }
}, (prevProps, nextProps) => {
  const range = getBlockSentenceRange(nextProps.block);
  if (!range) return true;

  const [min, max] = range;
  const prevIn = prevProps.currentSentenceIndex >= min && prevProps.currentSentenceIndex <= max;
  const nextIn = nextProps.currentSentenceIndex >= min && nextProps.currentSentenceIndex <= max;
  if (!prevIn && !nextIn) return true;

  return false;
});
