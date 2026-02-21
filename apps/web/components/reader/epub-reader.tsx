"use client";

import { useEffect, useRef, useState, useCallback, useMemo, memo } from "react";
import { fetchItemFile } from "@/lib/api";
import { Sentence } from "@/lib/types";
import { BOUNDARY } from "@/lib/sentences";
import { tokenize, countTokens } from "@/lib/cjk";

// ---------------------------------------------------------------------------
// Data structures for structured EPUB content
// ---------------------------------------------------------------------------

/** A run of text with inline formatting (bold / italic). */
interface StyledRun {
  text: string;
  bold?: boolean;
  italic?: boolean;
}

/** A sentence with its global index, plain text (for TTS), and styled runs (for rendering). */
interface RichSentence {
  index: number;
  text: string;
  runs: StyledRun[];
}

/** Union type representing a single content block extracted from EPUB HTML. */
type ContentBlock =
  | { type: "heading"; level: number; sentences: RichSentence[] }
  | { type: "paragraph"; sentences: RichSentence[] }
  | { type: "image"; src: string; alt?: string }
  | {
      type: "list";
      ordered: boolean;
      items: { sentences: RichSentence[] }[];
    }
  | { type: "blockquote"; sentences: RichSentence[] }
  | { type: "separator" };

interface EpubChapter {
  id: string;
  title: string;
  blocks: ContentBlock[];
}

// ---------------------------------------------------------------------------
// Props — identical to the old EpubReader, so ContentRouter needs no changes
// ---------------------------------------------------------------------------

interface EpubReaderProps {
  itemId: string;
  fallbackContent?: string;
  onSentencesExtracted: (sentences: Sentence[]) => void;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
  onChaptersExtracted?: (chapters: { id: string; title: string }[]) => void;
}

// ---------------------------------------------------------------------------
// Sentence splitting — uses shared BOUNDARY regex from lib/sentences.ts
// ---------------------------------------------------------------------------

/**
 * Split an array of StyledRuns into RichSentences by finding sentence
 * boundaries in the concatenated text, then slicing runs at those offsets.
 *
 * @param runs  – flat list of styled text runs from a single block element
 * @param startIndex – the global sentence index to begin numbering from
 * @returns { sentences, nextIndex } – the rich sentences and the next free index
 */
function splitRunsIntoSentences(
  runs: StyledRun[],
  startIndex: number
): { sentences: RichSentence[]; nextIndex: number } {
  // Concatenate all run texts into one string
  const fullText = runs.map((r) => r.text).join("");
  if (!fullText.trim()) return { sentences: [], nextIndex: startIndex };

  // Find sentence split points as character offsets
  const parts = fullText.split(BOUNDARY).filter((s) => s.length > 0);

  // Build a mapping: for each character offset in fullText, which run index
  // and local offset within that run?
  const sentences: RichSentence[] = [];
  let charCursor = 0; // tracks how far we've consumed in fullText
  let runIdx = 0; // current run
  let runOffset = 0; // offset within current run
  let sentenceIdx = startIndex;

  for (const part of parts) {
    if (!part.trim()) {
      // Advance cursor past whitespace-only "parts"
      charCursor += part.length;
      // Also advance run pointers
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

    // Slice runs for this sentence
    const sentenceRuns: StyledRun[] = [];
    let remaining = part.length;

    // Also skip any leading whitespace between sentences within runs
    // (the BOUNDARY regex splits *after* punctuation on whitespace)
    // We need to skip whitespace that was between the boundary match and
    // the start of `part` in the original fullText.
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
// DOM → StyledRun[] extraction  (preserves bold/italic from inline elements)
// ---------------------------------------------------------------------------

/**
 * Recursively extract styled text runs from a DOM element, inheriting
 * bold/italic state from ancestor elements like <b>, <strong>, <em>, <i>.
 */
function extractStyledRuns(
  node: Node,
  inheritBold: boolean,
  inheritItalic: boolean
): StyledRun[] {
  if (node.nodeType === Node.TEXT_NODE) {
    const text = node.textContent || "";
    if (!text) return [];
    return [
      {
        text,
        ...(inheritBold ? { bold: true } : {}),
        ...(inheritItalic ? { italic: true } : {}),
      },
    ];
  }

  if (node.nodeType !== Node.ELEMENT_NODE) return [];

  const el = node as Element;
  const tag = el.tagName.toLowerCase();

  // Determine if this element adds bold/italic styling
  const isBold =
    inheritBold || tag === "b" || tag === "strong";
  const isItalic =
    inheritItalic || tag === "i" || tag === "em";

  const runs: StyledRun[] = [];
  for (const child of Array.from(el.childNodes)) {
    runs.push(...extractStyledRuns(child, isBold, isItalic));
  }
  return runs;
}

// ---------------------------------------------------------------------------
// DOM → ContentBlock[] extraction
// ---------------------------------------------------------------------------

/**
 * Extract structured ContentBlocks from a chapter's <body> DOM element.
 *
 * @param body – the <body> element of the chapter's XHTML document
 * @param sentenceStartIndex – global sentence index to start numbering from
 * @param resolveImageSrc – async callback to convert a relative image href
 *                          to a blob URL via epub.js's archive
 * @returns { blocks, nextSentenceIndex }
 */
async function extractBlocks(
  body: Element,
  sentenceStartIndex: number,
  resolveImageSrc: (href: string) => Promise<string | null>
): Promise<{ blocks: ContentBlock[]; nextSentenceIndex: number }> {
  const blocks: ContentBlock[] = [];
  let sentenceIdx = sentenceStartIndex;

  /**
   * Process a single DOM element into zero or more ContentBlocks.
   * Handles recursive descent for wrapper elements like <div>, <section>.
   */
  async function processElement(el: Element): Promise<void> {
    const tag = el.tagName.toLowerCase();

    // --- Headings ---
    if (/^h[1-6]$/.test(tag)) {
      const level = parseInt(tag[1]);
      const runs = extractStyledRuns(el, false, false);
      const { sentences, nextIndex } = splitRunsIntoSentences(runs, sentenceIdx);
      if (sentences.length > 0) {
        blocks.push({ type: "heading", level, sentences });
        sentenceIdx = nextIndex;
      }
      return;
    }

    // --- Images ---
    if (tag === "img") {
      const src = el.getAttribute("src");
      if (src) {
        const resolved = await resolveImageSrc(src);
        if (resolved) {
          blocks.push({
            type: "image",
            src: resolved,
            alt: el.getAttribute("alt") || undefined,
          });
        }
      }
      return;
    }

    // --- SVG (may contain embedded <image> with xlink:href) ---
    if (tag === "svg") {
      const imageEl =
        el.querySelector("image") || el.querySelector("[href]");
      if (imageEl) {
        const href =
          imageEl.getAttribute("xlink:href") ||
          imageEl.getAttribute("href");
        if (href) {
          const resolved = await resolveImageSrc(href);
          if (resolved) {
            blocks.push({ type: "image", src: resolved });
          }
        }
      }
      return;
    }

    // --- Horizontal rule ---
    if (tag === "hr") {
      blocks.push({ type: "separator" });
      return;
    }

    // --- Lists ---
    if (tag === "ul" || tag === "ol") {
      const ordered = tag === "ol";
      const items: { sentences: RichSentence[] }[] = [];
      for (const li of Array.from(el.querySelectorAll(":scope > li"))) {
        const runs = extractStyledRuns(li, false, false);
        const { sentences, nextIndex } = splitRunsIntoSentences(
          runs,
          sentenceIdx
        );
        if (sentences.length > 0) {
          items.push({ sentences });
          sentenceIdx = nextIndex;
        }
      }
      if (items.length > 0) {
        blocks.push({ type: "list", ordered, items });
      }
      return;
    }

    // --- Blockquotes ---
    if (tag === "blockquote") {
      const runs = extractStyledRuns(el, false, false);
      const { sentences, nextIndex } = splitRunsIntoSentences(
        runs,
        sentenceIdx
      );
      if (sentences.length > 0) {
        blocks.push({ type: "blockquote", sentences });
        sentenceIdx = nextIndex;
      }
      return;
    }

    // --- Paragraphs ---
    if (tag === "p") {
      // Check if it contains only an image (common EPUB pattern)
      const imgs = el.querySelectorAll("img");
      if (imgs.length > 0 && !el.textContent?.trim()) {
        for (const img of Array.from(imgs)) {
          const src = img.getAttribute("src");
          if (src) {
            const resolved = await resolveImageSrc(src);
            if (resolved) {
              blocks.push({
                type: "image",
                src: resolved,
                alt: img.getAttribute("alt") || undefined,
              });
            }
          }
        }
        return;
      }

      const runs = extractStyledRuns(el, false, false);
      const { sentences, nextIndex } = splitRunsIntoSentences(
        runs,
        sentenceIdx
      );
      if (sentences.length > 0) {
        blocks.push({ type: "paragraph", sentences });
        sentenceIdx = nextIndex;
      }
      return;
    }

    // --- Wrapper elements (div, section, article, etc.) → recurse children ---
    // Also handle <figure> by looking for <img> and <figcaption>
    if (tag === "figure") {
      const img = el.querySelector("img");
      if (img) {
        const src = img.getAttribute("src");
        if (src) {
          const resolved = await resolveImageSrc(src);
          if (resolved) {
            blocks.push({
              type: "image",
              src: resolved,
              alt: img.getAttribute("alt") || undefined,
            });
          }
        }
      }
      // Also extract figcaption text as a paragraph
      const caption = el.querySelector("figcaption");
      if (caption) {
        const runs = extractStyledRuns(caption, false, true);
        const { sentences, nextIndex } = splitRunsIntoSentences(
          runs,
          sentenceIdx
        );
        if (sentences.length > 0) {
          blocks.push({ type: "paragraph", sentences });
          sentenceIdx = nextIndex;
        }
      }
      return;
    }

    // Generic container — check if it has direct text content
    const hasDirectText = Array.from(el.childNodes).some(
      (n) => n.nodeType === Node.TEXT_NODE && n.textContent?.trim()
    );

    if (hasDirectText) {
      // Treat as a paragraph-like element
      const runs = extractStyledRuns(el, false, false);
      const { sentences, nextIndex } = splitRunsIntoSentences(
        runs,
        sentenceIdx
      );
      if (sentences.length > 0) {
        blocks.push({ type: "paragraph", sentences });
        sentenceIdx = nextIndex;
      }
      return;
    }

    // No direct text — recurse into child elements
    for (const child of Array.from(el.children)) {
      await processElement(child);
    }
  }

  // Process all direct children of the body
  for (const child of Array.from(body.children)) {
    await processElement(child);
  }

  return { blocks, nextSentenceIndex: sentenceIdx };
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
const SentenceSpan = memo(function SentenceSpan({
  sentence,
  isActive,
  wordProgress,
  onClick,
}: {
  sentence: RichSentence;
  isActive: boolean;
  wordProgress: number;
  onClick: (index: number) => void;
}) {
  // Cache wordCount — sentence.text is stable across renders.
  // countTokens handles CJK (per-character) and Latin (per-word) correctly.
  const wordCount = useMemo(
    () => countTokens(sentence.text),
    [sentence.text]
  );
  const highlightedWordIdx = isActive
    ? Math.min(Math.floor(wordProgress * wordCount), wordCount - 1)
    : -1;

  let globalWordIdx = 0;

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
        return (
          <span
            key={ri}
            className={`${run.bold ? "font-bold" : ""} ${run.italic ? "italic" : ""}`}
          >
            {tokens.map((token, ti) => {
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
                      ? "text-highlight-word font-semibold"
                      : ""
                  }
                >
                  {token}
                </span>
              );
            })}
          </span>
        );
      })}
    </span>
  );
}, (prevProps, nextProps) => {
  // Return true = skip re-render, false = re-render
  // onClick is ref-stable (useTTSPlayer stores mutable values in refs),
  // so no identity check needed here.
  if (!prevProps.isActive && !nextProps.isActive) return true;   // both inactive → skip
  if (prevProps.isActive !== nextProps.isActive) return false;    // active state changed → re-render
  return prevProps.wordProgress === nextProps.wordProgress;       // both active → compare progress
});

// ---------------------------------------------------------------------------
// Block renderers
// ---------------------------------------------------------------------------

function renderSentences(
  sentences: RichSentence[],
  currentSentenceIndex: number,
  currentWordProgress: number,
  onSentenceClick: (index: number) => void
) {
  return sentences.map((s) => (
    <SentenceSpan
      key={s.index}
      sentence={s}
      isActive={s.index === currentSentenceIndex}
      wordProgress={currentWordProgress}
      onClick={onSentenceClick}
    />
  ));
}

/**
 * Returns the [min, max] sentence index range for a content block,
 * or null if the block contains no sentences (image/separator).
 * Used by BlockRenderer's memo comparator to skip re-renders for
 * blocks that don't contain the active sentence.
 */
function getBlockSentenceRange(block: ContentBlock): [number, number] | null {
  if (block.type === "image" || block.type === "separator") return null;

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
 * Wrapped in React.memo so that blocks not containing the active sentence
 * are skipped entirely during TTS playback updates.
 */
const BlockRenderer = memo(function BlockRenderer({
  block,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
}: {
  block: ContentBlock;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
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
        onSentenceClick
      );
      if (block.level <= 1) return <h1 className={className}>{children}</h1>;
      if (block.level === 2) return <h2 className={className}>{children}</h2>;
      if (block.level === 3) return <h3 className={className}>{children}</h3>;
      if (block.level === 4) return <h4 className={className}>{children}</h4>;
      if (block.level === 5) return <h5 className={className}>{children}</h5>;
      return <h6 className={className}>{children}</h6>;
    }

    case "paragraph":
      return (
        <p className="mb-4 text-lg leading-relaxed text-text-primary">
          {renderSentences(
            block.sentences,
            currentSentenceIndex,
            currentWordProgress,
            onSentenceClick
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
        <ListTag className={listClass}>
          {block.items.map((item, i) => (
            <li key={i} className="mb-1 text-lg leading-relaxed text-text-primary">
              {renderSentences(
                item.sentences,
                currentSentenceIndex,
                currentWordProgress,
                onSentenceClick
              )}
            </li>
          ))}
        </ListTag>
      );
    }

    case "blockquote":
      return (
        <blockquote className="border-l-4 border-border pl-4 italic text-text-secondary mb-4">
          {renderSentences(
            block.sentences,
            currentSentenceIndex,
            currentWordProgress,
            onSentenceClick
          )}
        </blockquote>
      );

    case "separator":
      return <hr className="border-border my-4" />;
  }
}, (prevProps, nextProps) => {
  // Return true = skip re-render
  // onSentenceClick is ref-stable — no identity check needed.
  const range = getBlockSentenceRange(nextProps.block);
  if (!range) return true;                              // no sentences (image/separator) → skip

  const [min, max] = range;
  const prevIn = prevProps.currentSentenceIndex >= min && prevProps.currentSentenceIndex <= max;
  const nextIn = nextProps.currentSentenceIndex >= min && nextProps.currentSentenceIndex <= max;
  if (!prevIn && !nextIn) return true;                  // active sentence outside this block → skip

  // Otherwise re-render (SentenceSpan's own memo handles the rest)
  return false;
});

// ---------------------------------------------------------------------------
// EPUB loading via epub.js
// ---------------------------------------------------------------------------

/**
 * Load and parse an EPUB file into structured chapters using epub.js.
 *
 * This replaces the old JSZip + manual OPF parsing approach with epub.js
 * for reliable EPUB parsing, while extracting structured content blocks
 * for pure-React rendering (no dangerouslySetInnerHTML).
 */
interface EpubLoadResult {
  chapters: EpubChapter[];
}

async function loadEpubChapters(
  buffer: ArrayBuffer
): Promise<EpubLoadResult> {
  const ePub = (await import("epubjs")).default;
  const book = ePub(buffer);

  // Wait for book metadata and navigation to load
  await book.ready;
  await book.loaded.navigation;

  // Build TOC label lookup: href (without fragment) → label
  const tocLabels = new Map<string, string>();
  const navToc = (book.navigation as any)?.toc;
  if (navToc && Array.isArray(navToc)) {
    for (const entry of navToc) {
      const href = (entry.href || "").split("#")[0];
      const label = (entry.label || "").trim();
      if (href && label) tocLabels.set(href, label);
    }
  }

  const chapters: EpubChapter[] = [];
  let sentenceIdx = 0;

  // Access the spine — epub.js stores spine items under `book.spine`
  const spine = book.spine as any;
  // Use `spineItems` (Section instances with .load()), NOT `items` (raw packaging objects)
  const spineItems: any[] = spine?.spineItems || [];

  for (const section of spineItems) {
    // Load the section's document via epub.js
    await section.load(book.load.bind(book));
    const doc: Document = section.document;
    if (!doc?.body) continue;

    /**
     * Resolve image src paths to blob URLs using epub.js's archive.
     * Handles both relative paths and absolute-within-EPUB paths.
     */
    const resolveImageSrc = async (href: string): Promise<string | null> => {
      try {
        // Skip data URIs and blob URLs
        if (href.startsWith("data:") || href.startsWith("blob:")) return href;

        // Resolve relative path against section's canonical directory
        const sectionHref = section.canonical || section.href || "";
        const sectionDir = sectionHref.includes("/")
          ? sectionHref.substring(0, sectionHref.lastIndexOf("/") + 1)
          : "";

        let resolvedPath: string;
        if (href.startsWith("/")) {
          resolvedPath = href.substring(1);
        } else if (href.startsWith("../") || href.startsWith("./")) {
          // Resolve relative path segments
          const parts = (sectionDir + href).split("/");
          const resolved: string[] = [];
          for (const part of parts) {
            if (part === "." || part === "") continue;
            if (part === "..") resolved.pop();
            else resolved.push(part);
          }
          resolvedPath = resolved.join("/");
        } else {
          resolvedPath = sectionDir + href;
        }

        // Use epub.js archive to get the resource as a blob URL
        // epub.js archive internally does `url.substr(1)` — path must start with "/"
        const url = await (book.archive as any).createUrl("/" + resolvedPath, {
          base64: false,
        });
        return url || null;
      } catch {
        return null;
      }
    };

    // Extract structured content blocks from the chapter body
    const { blocks, nextSentenceIndex } = await extractBlocks(
      doc.body,
      sentenceIdx,
      resolveImageSrc
    );
    sentenceIdx = nextSentenceIndex;

    // Determine chapter title: prefer TOC label, then first heading, then generic
    const sectionHref = (section.href || "").split("#")[0];
    let title = tocLabels.get(sectionHref) || "";
    if (!title) {
      // Look for first heading block
      const headingBlock = blocks.find((b) => b.type === "heading");
      if (headingBlock && headingBlock.type === "heading") {
        title = headingBlock.sentences.map((s) => s.text).join(" ");
      }
    }
    if (!title) {
      title = `Chapter ${chapters.length + 1}`;
    }

    chapters.push({
      id: `chapter-${chapters.length + 1}`,
      title,
      blocks,
    });
  }

  // Clean up epub.js resources (blob URLs survive destroy)
  book.destroy();

  return { chapters };
}

// ---------------------------------------------------------------------------
// EpubReader component
// ---------------------------------------------------------------------------

export function EpubReader({
  itemId,
  fallbackContent,
  onSentencesExtracted,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
  onChaptersExtracted,
}: EpubReaderProps) {
  const [chapters, setChapters] = useState<EpubChapter[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const sentencesExtractedRef = useRef(false);

  // Collect all sentences across all chapters into a flat array (for TTS)
  const allSentences: Sentence[] = useMemo(() => {
    const result: Sentence[] = [];
    for (const chapter of chapters) {
      for (const block of chapter.blocks) {
        if (block.type === "image" || block.type === "separator") continue;
        if (block.type === "list") {
          for (const item of block.items) {
            for (const s of item.sentences) {
              result.push({ index: s.index, text: s.text });
            }
          }
        } else {
          for (const s of block.sentences) {
            result.push({ index: s.index, text: s.text });
          }
        }
      }
    }
    return result;
  }, [chapters]);

  // Load the EPUB file on mount
  const loadEpub = useCallback(async () => {
    try {
      setLoading(true);
      const buffer = await fetchItemFile(itemId);
      const result = await loadEpubChapters(buffer);

      if (result.chapters.length === 0) {
        throw new Error("No chapters found in EPUB");
      }

      setChapters(result.chapters);
      onChaptersExtracted?.(
        result.chapters.map((c) => ({ id: c.id, title: c.title }))
      );
    } catch (err) {
      console.error("EPUB load error:", err);
      setError(err instanceof Error ? err.message : "Failed to load EPUB");
    } finally {
      setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [itemId]);

  useEffect(() => {
    loadEpub();
  }, [loadEpub]);

  // Emit extracted sentences to the parent (once per load)
  useEffect(() => {
    if (allSentences.length > 0 && !sentencesExtractedRef.current) {
      sentencesExtractedRef.current = true;
      onSentencesExtracted(allSentences);
    }
  }, [allSentences, onSentencesExtracted]);

  // Track blob URLs from EPUB images and revoke them on unmount to prevent memory leaks
  const blobUrlsRef = useRef<Set<string>>(new Set());
  useEffect(() => {
    const urls = blobUrlsRef.current;
    for (const chapter of chapters) {
      for (const block of chapter.blocks) {
        if (block.type === "image" && block.src.startsWith("blob:")) {
          urls.add(block.src);
        }
      }
    }
    return () => {
      for (const url of urls) URL.revokeObjectURL(url);
      urls.clear();
    };
  }, [chapters]);

  // Auto-scroll to the currently active sentence
  useEffect(() => {
    if (currentSentenceIndex < 0) return;
    const el = document.querySelector(
      `[data-sentence-id="${currentSentenceIndex}"]`
    );
    if (el) {
      el.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, [currentSentenceIndex]);

  // --- Loading state ---
  if (loading) {
    return (
      <div className="space-y-4 animate-pulse">
        {Array.from({ length: 8 }).map((_, i) => (
          <div
            key={i}
            className="h-4 bg-surface-hover rounded"
            style={{ width: `${60 + Math.random() * 30}%` }}
          />
        ))}
      </div>
    );
  }

  // --- Error state with optional fallback ---
  if (error) {
    if (fallbackContent) {
      const PlainTextReaderFallback =
        require("./plain-text-reader").PlainTextReader;
      return (
        <PlainTextReaderFallback
          content={fallbackContent}
          onSentencesExtracted={onSentencesExtracted}
          currentSentenceIndex={currentSentenceIndex}
          currentWordProgress={currentWordProgress}
          onSentenceClick={onSentenceClick}
        />
      );
    }
    return (
      <div className="text-center py-12 text-text-secondary">
        <p>Failed to load EPUB: {error}</p>
      </div>
    );
  }

  // --- Main render: pure React, no dangerouslySetInnerHTML ---
  return (
    <div className="space-y-8">
      {chapters.map((chapter, idx) => (
        <div key={chapter.id} data-chapter={chapter.id}>
          {idx > 0 && <hr className="border-border my-8" />}
          <div className="space-y-0">
            {chapter.blocks.map((block, bi) => (
              <BlockRenderer
                key={bi}
                block={block}
                currentSentenceIndex={currentSentenceIndex}
                currentWordProgress={currentWordProgress}
                onSentenceClick={onSentenceClick}
              />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
