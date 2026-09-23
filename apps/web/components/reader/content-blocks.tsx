"use client";

/**
 * Shared content-block types and rendering components.
 *
 * Shared content-block types, sentence splitting, and rendering components.
 * Each format reader (EpubReader, HtmlReader, etc.) reuses these for
 * sentence-level TTS highlighting, word-level progress, and rich rendering.
 */

import {
  Fragment,
  createContext,
  memo,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { visualSentenceSeparator } from "@/lib/sentences";
import type { HighlightTarget } from "@/lib/rust-core";

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
  | {
      type: "image";
      src: string | null;
      alt?: string;
      assetId?: string;
      mediaType?: string;
    }
  | {
      type: "list";
      ordered: boolean;
      items: { sentences: RichSentence[] }[];
      anchorId?: string;
    }
  | { type: "blockquote"; sentences: RichSentence[]; anchorId?: string }
  | { type: "code"; text: string; language?: string }
  | { type: "separator" };

export type ImageAssetResolver = (assetId: string) => Promise<string>;

const ReaderHighlightContext = createContext<HighlightTarget | null>(null);

export function ReaderHighlightProvider({
  target,
  children,
}: {
  target: HighlightTarget | null;
  children: ReactNode;
}) {
  return (
    <ReaderHighlightContext.Provider value={target}>
      {children}
    </ReaderHighlightContext.Provider>
  );
}

export function useReaderHighlightTarget(): HighlightTarget | null {
  return useContext(ReaderHighlightContext);
}

/** Convert Rust's block-local range into the active sentence's scalar space. */
export function sentenceLocalWordRange(
  target: HighlightTarget | null,
  sentenceIndex: number
): [number, number] | null {
  if (target?.sentence_index !== sentenceIndex || !target.word_range) return null;
  const [sentenceStart, sentenceEnd] = target.sentence_range;
  const [wordStart, wordEnd] = target.word_range;
  if (
    wordStart < sentenceStart
    || wordEnd > sentenceEnd
    || wordStart >= wordEnd
  ) {
    return null;
  }
  return [wordStart - sentenceStart, wordEnd - sentenceStart];
}

export interface ScalarTextSegment {
  text: string;
  range: [number, number];
  highlighted: boolean;
}

export interface ScalarTextRun {
  run: StyledRun;
  segments: ScalarTextSegment[];
}

/** Split styled visual runs exactly at Rust's scalar highlight boundaries. */
export function scalarTextRuns(
  runs: StyledRun[],
  highlightRange: [number, number] | null
): ScalarTextRun[] {
  let scalarOffset = 0;
  return runs.map((run) => {
    const scalars = Array.from(run.text);
    const runStart = scalarOffset;
    const runEnd = runStart + scalars.length;
    scalarOffset = runEnd;
    if (
      !highlightRange
      || highlightRange[1] <= runStart
      || highlightRange[0] >= runEnd
    ) {
      return {
        run,
        segments: [{
          text: run.text,
          range: [runStart, runEnd],
          highlighted: false,
        }],
      };
    }

    const highlightStart = Math.max(runStart, highlightRange[0]);
    const highlightEnd = Math.min(runEnd, highlightRange[1]);
    const boundaries = [runStart, highlightStart, highlightEnd, runEnd]
      .filter((value, index, values) => index === 0 || value !== values[index - 1]);
    const segments: ScalarTextSegment[] = [];
    for (let index = 0; index < boundaries.length - 1; index += 1) {
      const start = boundaries[index];
      const end = boundaries[index + 1];
      segments.push({
        text: scalars.slice(start - runStart, end - runStart).join(""),
        range: [start, end],
        highlighted: start >= highlightStart && end <= highlightEnd,
      });
    }
    return { run, segments };
  });
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
  onClick,
  onLinkClick,
}: {
  sentence: RichSentence;
  isActive: boolean;
  wordProgress: number;
  onClick: (index: number) => void;
  onLinkClick?: (href: string) => void;
}) {
  const target = useReaderHighlightTarget();
  const wordRange = isActive
    ? sentenceLocalWordRange(target, sentence.index)
    : null;
  const projectedRuns = scalarTextRuns(sentence.runs, wordRange);

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
      {projectedRuns.map(({ run, segments }, ri) => {
        const inner = segments.map((segment, segmentIndex) => {
          return (
            <span
              key={segmentIndex}
              className={
                segment.highlighted
                  ? "bg-highlight-word-bg rounded-sm"
                  : ""
              }
            >
              {segment.text}
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
  if (
    prevProps.sentence !== nextProps.sentence
    || prevProps.onClick !== nextProps.onClick
    || prevProps.onLinkClick !== nextProps.onLinkClick
  ) {
    return false;
  }
  if (!prevProps.isActive && !nextProps.isActive) return true;
  if (prevProps.isActive !== nextProps.isActive) return false;
  return true; // Rust target arrives through context and bypasses prop memoization.
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
  return sentences.map((s, sentenceIndex) => (
    <Fragment key={s.index}>
      {visualSentenceSeparator(
        sentences[sentenceIndex - 1]?.text ?? null,
        s.text
      )}
      <SentenceSpan
        sentence={s}
        isActive={s.index === currentSentenceIndex}
        wordProgress={currentWordProgress}
        onClick={onSentenceClick}
        onLinkClick={onLinkClick}
      />
    </Fragment>
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
  resolveImageAsset,
}: {
  block: ContentBlock;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
  onLinkClick?: (href: string) => void;
  resolveImageAsset?: ImageAssetResolver;
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
        <ReaderImage block={block} resolveImageAsset={resolveImageAsset} />
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
        <blockquote
          id={block.anchorId}
          className="rounded-md bg-surface-hover px-4 py-3 italic text-text-secondary mb-4"
        >
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
  if (
    prevProps.block !== nextProps.block
    || prevProps.onSentenceClick !== nextProps.onSentenceClick
    || prevProps.onLinkClick !== nextProps.onLinkClick
    || prevProps.resolveImageAsset !== nextProps.resolveImageAsset
  ) {
    return false;
  }
  const range = getBlockSentenceRange(nextProps.block);
  if (!range) return true;

  const [min, max] = range;
  const prevIn = prevProps.currentSentenceIndex >= min && prevProps.currentSentenceIndex <= max;
  const nextIn = nextProps.currentSentenceIndex >= min && nextProps.currentSentenceIndex <= max;
  if (!prevIn && !nextIn) return true;

  return false;
});

function ReaderImage({
  block,
  resolveImageAsset,
}: {
  block: Extract<ContentBlock, { type: "image" }>;
  resolveImageAsset?: ImageAssetResolver;
}) {
  const [source, setSource] = useState(block.src);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    setSource(block.src);
    setFailed(false);
    if (block.src || !block.assetId || !resolveImageAsset) {
      if (!block.src) setFailed(true);
      return;
    }

    let active = true;
    void resolveImageAsset(block.assetId)
      .then((resolved) => {
        if (active) setSource(resolved);
      })
      .catch(() => {
        if (active) setFailed(true);
      });
    return () => {
      active = false;
    };
  }, [block.assetId, block.src, resolveImageAsset]);

  if (!source) {
    return (
      <div
        role="img"
        aria-label={block.alt || "Book illustration"}
        aria-busy={failed ? undefined : true}
        className="my-4 flex min-h-32 items-center justify-center rounded-lg border border-border bg-surface-subtle text-sm text-text-secondary"
      >
        {failed ? "Image unavailable" : "Loading image…"}
      </div>
    );
  }

  return (
    // EPUB/PDF images can be blob URLs, so next/image cannot optimize them.
    // eslint-disable-next-line @next/next/no-img-element
    <img
      src={source}
      alt={block.alt || ""}
      className="max-w-full h-auto rounded-lg my-4"
    />
  );
}
