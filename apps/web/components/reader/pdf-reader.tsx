"use client";

/**
 * PDF text reflow reader — extracts structured text from PDFs and renders
 * via the shared ContentBlock / BlockRenderer system for TTS highlighting.
 *
 * Pipeline: PDF binary → pdfjs-dist → TextItem[]
 *   → line grouping by Y-coordinate → column reorder → header/footer filter
 *   → hyphen merge → paragraph/heading/list detection
 *   → StyledRun[] → splitRunsIntoSentences() → ContentBlock[]
 *
 * When extraction quality is poor (scanned/image-only PDFs with < 50 chars
 * per page on average), automatically falls back to PlainTextReader using
 * the backend pymupdf-extracted fallbackContent.
 */

import { useEffect, useRef, useState, useCallback, useMemo } from "react";
import { fetchItemFile } from "@/lib/api";
import { Sentence } from "@/lib/types";
import {
  type StyledRun,
  type RichSentence,
  type ContentBlock,
  splitRunsIntoSentences,
  BlockRenderer,
} from "./content-blocks";

// ---------------------------------------------------------------------------
// Types for the PDF extraction pipeline
// ---------------------------------------------------------------------------

/** A single text item extracted from PDF with position and style metadata. */
interface PdfTextItem {
  str: string;
  x: number;
  y: number;
  width: number;
  fontSize: number;
  fontName: string;
}

/** A horizontal line of text, grouped by Y-coordinate proximity. */
interface PdfLine {
  items: PdfTextItem[];
  y: number; // average Y position of items
  fontSize: number; // max font size in the line
  fontName: string; // dominant font (by character count)
}

/** A chapter in the reflowed PDF content. */
interface PdfChapter {
  id: string;
  title: string;
  blocks: ContentBlock[];
}

/** A flattened outline entry with resolved 0-based page index. */
interface ResolvedOutlineEntry {
  title: string;
  pageIndex: number;
  depth: number; // 0 = top-level
}

// ---------------------------------------------------------------------------
// Props — aligned with EpubReader, uses onChaptersExtracted for TOC
// ---------------------------------------------------------------------------

interface PdfReaderProps {
  itemId: string;
  fallbackContent?: string;
  onSentencesExtracted: (sentences: Sentence[]) => void;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
  onChaptersExtracted?: (
    chapters: { id: string; title: string; depth?: number }[]
  ) => void;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Regex to detect list item prefixes: bullets, dashes, numbered, lettered. */
const LIST_PREFIX = /^(?:[-•·▪▸►–—]\s+|\d+[.)]\s+|[a-zA-Z][.)]\s+)/;

// ---------------------------------------------------------------------------
// Font style parsing — extract bold/italic from PDF font names
// e.g. "TimesNewRoman,Bold", "AAAAAB+Arial-BoldItalicMT"
// ---------------------------------------------------------------------------

function parseFontStyle(fontName: string): { bold: boolean; italic: boolean } {
  const lower = fontName.toLowerCase();
  return {
    bold: /bold|heavy|black/i.test(lower),
    italic: /italic|oblique/i.test(lower),
  };
}

// ---------------------------------------------------------------------------
// Text extraction: pdfjs TextContent → PdfTextItem[]
// ---------------------------------------------------------------------------

function extractTextItems(textContent: any): PdfTextItem[] {
  return textContent.items
    .filter((item: any) => item.str?.trim())
    .map((item: any) => ({
      str: item.str as string,
      x: item.transform[4] as number,
      y: item.transform[5] as number,
      width: (item.width as number) || 0,
      // Font size from the transformation matrix (scaleX or scaleY)
      fontSize: Math.round(
        Math.abs(item.transform[0]) || Math.abs(item.transform[3]) || 12
      ),
      fontName: (item.fontName as string) || "",
    }));
}

// ---------------------------------------------------------------------------
// Group items into lines by Y-coordinate proximity
// Tolerance = max(fontSize) * 0.3 to handle slight baseline shifts
// ---------------------------------------------------------------------------

function groupIntoLines(items: PdfTextItem[]): PdfLine[] {
  if (items.length === 0) return [];

  // PDF Y-axis points upward: sort descending Y for top-to-bottom reading order
  const sorted = [...items].sort((a, b) => b.y - a.y || a.x - b.x);

  const lines: PdfLine[] = [];
  let currentItems: PdfTextItem[] = [sorted[0]];
  let currentY = sorted[0].y;

  for (let i = 1; i < sorted.length; i++) {
    const item = sorted[i];
    const tolerance =
      Math.max(item.fontSize, currentItems[0]?.fontSize || 12) * 0.3;

    if (Math.abs(item.y - currentY) <= tolerance) {
      currentItems.push(item);
    } else {
      // Finalize the current line (sort left-to-right by X)
      currentItems.sort((a, b) => a.x - b.x);
      lines.push(buildLine(currentItems));
      currentItems = [item];
      currentY = item.y;
    }
  }

  if (currentItems.length > 0) {
    currentItems.sort((a, b) => a.x - b.x);
    lines.push(buildLine(currentItems));
  }

  return lines;
}

/** Build a PdfLine from pre-sorted items, computing aggregate properties. */
function buildLine(items: PdfTextItem[]): PdfLine {
  const maxFontSize = Math.max(...items.map((i) => i.fontSize));

  // Dominant font: the one contributing the most characters
  const fontCharCounts = new Map<string, number>();
  for (const item of items) {
    fontCharCounts.set(
      item.fontName,
      (fontCharCounts.get(item.fontName) || 0) + item.str.length
    );
  }
  const dominantFont =
    [...fontCharCounts.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] || "";

  return {
    items,
    y: items.reduce((sum, i) => sum + i.y, 0) / items.length,
    fontSize: maxFontSize,
    fontName: dominantFont,
  };
}

// ---------------------------------------------------------------------------
// Multi-column detection — reorder lines so left column reads before right
// Uses X-coordinate gap analysis against page center
// ---------------------------------------------------------------------------

function reorderColumns(lines: PdfLine[], pageWidth: number): PdfLine[] {
  if (lines.length < 4) return lines;

  const midPoint = pageWidth / 2;
  const leftLines: PdfLine[] = [];
  const rightLines: PdfLine[] = [];

  for (const line of lines) {
    const lineStart = line.items[0]?.x || 0;
    const lineEnd = Math.max(...line.items.map((i) => i.x + i.width));
    const lineCenter = (lineStart + lineEnd) / 2;

    // Right column: starts well past center and center of mass is in right half
    if (lineStart > midPoint * 0.6 && lineCenter > midPoint) {
      rightLines.push(line);
    } else {
      leftLines.push(line);
    }
  }

  // Only treat as two columns if both sides carry significant content
  if (
    leftLines.length > lines.length * 0.25 &&
    rightLines.length > lines.length * 0.25
  ) {
    return [...leftLines, ...rightLines];
  }

  return lines;
}

// ---------------------------------------------------------------------------
// Header/footer detection — identify text that repeats on >= 50% of pages
// Digits are stripped before comparison (page numbers vary per page)
// ---------------------------------------------------------------------------

function detectRepeatedText(allPageLines: PdfLine[][]): {
  headers: Set<string>;
  footers: Set<string>;
} {
  if (allPageLines.length < 3)
    return { headers: new Set(), footers: new Set() };

  const topTexts = new Map<string, number>();
  const bottomTexts = new Map<string, number>();

  for (const lines of allPageLines) {
    if (lines.length === 0) continue;

    const first = lines[0].items
      .map((i) => i.str)
      .join(" ")
      .replace(/\d+/g, "")
      .trim();
    if (first.length > 0 && first.length < 80) {
      topTexts.set(first, (topTexts.get(first) || 0) + 1);
    }

    const last = lines[lines.length - 1].items
      .map((i) => i.str)
      .join(" ")
      .replace(/\d+/g, "")
      .trim();
    if (last.length > 0 && last.length < 80) {
      bottomTexts.set(last, (bottomTexts.get(last) || 0) + 1);
    }
  }

  const threshold = allPageLines.length * 0.5;
  return {
    headers: new Set(
      [...topTexts.entries()]
        .filter(([, count]) => count >= threshold)
        .map(([text]) => text)
    ),
    footers: new Set(
      [...bottomTexts.entries()]
        .filter(([, count]) => count >= threshold)
        .map(([text]) => text)
    ),
  };
}

/** Remove header/footer lines from a single page's line array. */
function filterHeaderFooter(
  lines: PdfLine[],
  headers: Set<string>,
  footers: Set<string>
): PdfLine[] {
  let result = lines;

  if (result.length > 0 && headers.size > 0) {
    const first = result[0].items
      .map((i) => i.str)
      .join(" ")
      .replace(/\d+/g, "")
      .trim();
    if (headers.has(first)) result = result.slice(1);
  }

  if (result.length > 0 && footers.size > 0) {
    const last = result[result.length - 1].items
      .map((i) => i.str)
      .join(" ")
      .replace(/\d+/g, "")
      .trim();
    if (footers.has(last)) result = result.slice(0, -1);
  }

  return result;
}

// ---------------------------------------------------------------------------
// Hyphen merging — reconnect words split across line breaks
// "docu-" + "ment" → "document"
// ---------------------------------------------------------------------------

function mergeHyphens(lines: PdfLine[]): PdfLine[] {
  for (let i = 0; i < lines.length - 1; i++) {
    const lastItem = lines[i].items[lines[i].items.length - 1];
    if (!lastItem || lastItem.str.length <= 1) continue;

    // Only merge when line ends with hyphen and next starts with lowercase
    if (lastItem.str.endsWith("-")) {
      const nextLine = lines[i + 1];
      const firstNext = nextLine?.items[0];
      if (firstNext && /^[a-z]/.test(firstNext.str)) {
        lastItem.str = lastItem.str.slice(0, -1) + firstNext.str;
        nextLine.items = nextLine.items.slice(1);
      }
    }
  }

  // Drop lines that became empty after merging
  return lines.filter((l) => l.items.length > 0);
}

// ---------------------------------------------------------------------------
// Statistics: body font size (most common) and median line spacing
// ---------------------------------------------------------------------------

/** Detect the body font size — the size used for the most characters overall. */
function detectBodyFontSize(allLines: PdfLine[]): number {
  const sizeCounts = new Map<number, number>();
  for (const line of allLines) {
    for (const item of line.items) {
      sizeCounts.set(
        item.fontSize,
        (sizeCounts.get(item.fontSize) || 0) + item.str.length
      );
    }
  }
  if (sizeCounts.size === 0) return 12;
  return [...sizeCounts.entries()].sort((a, b) => b[1] - a[1])[0][0];
}

/** Median vertical spacing between consecutive lines (for paragraph break threshold). */
function computeMedianLineSpacing(lines: PdfLine[]): number {
  if (lines.length < 2) return 0;

  const spacings: number[] = [];
  for (let i = 1; i < lines.length; i++) {
    const s = Math.abs(lines[i - 1].y - lines[i].y);
    if (s > 0 && s < 200) spacings.push(s);
  }
  if (spacings.length === 0) return 0;
  spacings.sort((a, b) => a - b);
  return spacings[Math.floor(spacings.length / 2)];
}

// ---------------------------------------------------------------------------
// Line → StyledRun[] conversion, preserving bold/italic from fontName
// ---------------------------------------------------------------------------

/** Convert one or more lines into a flat array of StyledRuns.
 *  Adjacent items with same style are merged. Spaces are inserted
 *  between items when a positional gap exceeds fontSize * 0.15. */
function linesToRuns(lines: PdfLine[]): StyledRun[] {
  const runs: StyledRun[] = [];

  for (let li = 0; li < lines.length; li++) {
    const line = lines[li];

    for (let i = 0; i < line.items.length; i++) {
      const item = line.items[i];
      const style = parseFontStyle(item.fontName);
      let text = item.str;

      // Insert space between adjacent items on same line when gap is wide enough
      if (i < line.items.length - 1) {
        const next = line.items[i + 1];
        const gap = next.x - (item.x + item.width);
        if (gap > item.fontSize * 0.15 && !text.endsWith(" ")) {
          text += " ";
        }
      }

      // Merge consecutive runs sharing the same bold/italic style
      if (runs.length > 0) {
        const prev = runs[runs.length - 1];
        if (
          (prev.bold ?? false) === style.bold &&
          (prev.italic ?? false) === style.italic
        ) {
          prev.text += text;
          continue;
        }
      }

      runs.push({
        text,
        ...(style.bold ? { bold: true } : {}),
        ...(style.italic ? { italic: true } : {}),
      });
    }

    // Space between lines within a paragraph
    if (li < lines.length - 1 && runs.length > 0) {
      const last = runs[runs.length - 1];
      if (!last.text.endsWith(" ")) {
        last.text += " ";
      }
    }
  }

  return runs;
}

/** Strip the list bullet/number prefix from the beginning of runs. */
function stripListPrefix(runs: StyledRun[]): StyledRun[] {
  if (runs.length === 0) return runs;
  const match = runs[0].text.match(LIST_PREFIX);
  if (!match) return runs;
  const stripped = runs[0].text.slice(match[0].length);
  if (stripped) return [{ ...runs[0], text: stripped }, ...runs.slice(1)];
  return runs.slice(1);
}

// ---------------------------------------------------------------------------
// Lines → ContentBlock[] with paragraph / heading / list detection
//
// Paragraph break: line spacing > median * 1.8
// Heading: fontSize >= body * 1.2, or all-uppercase, or short bold line
// List: starts with "- ", "• ", "1. " etc.
// ---------------------------------------------------------------------------

function linesToBlocks(
  lines: PdfLine[],
  bodyFontSize: number,
  medianSpacing: number,
  sentenceStartIndex: number
): { blocks: ContentBlock[]; nextIndex: number } {
  const blocks: ContentBlock[] = [];
  let sentenceIdx = sentenceStartIndex;
  const headingThreshold = bodyFontSize * 1.2;

  // Group consecutive lines into typed segments
  type Segment = { lines: PdfLine[]; type: "heading" | "paragraph" | "list" };
  const segments: Segment[] = [];
  let currentLines: PdfLine[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const lineText = line.items.map((it) => it.str).join(" ").trim();
    if (!lineText) continue;

    // Detect paragraph break by spacing gap
    const prevLine = i > 0 ? lines[i - 1] : null;
    const isParagraphBreak =
      currentLines.length > 0 &&
      prevLine &&
      medianSpacing > 0 &&
      Math.abs(prevLine.y - line.y) > medianSpacing * 1.8;

    // Detect heading: significantly larger font, all-uppercase, or bold short line
    const isUpperCase =
      lineText.length < 100 &&
      lineText === lineText.toUpperCase() &&
      /[A-Z]{2,}/.test(lineText);
    const isHeading =
      (line.fontSize >= headingThreshold && lineText.length < 200) ||
      isUpperCase ||
      (lineText.length < 100 &&
        line.fontSize > bodyFontSize &&
        parseFontStyle(line.fontName).bold);

    // Detect list item by prefix pattern
    const isList = LIST_PREFIX.test(lineText);

    // Flush accumulated paragraph lines on any structural break
    if (isParagraphBreak || isHeading || isList) {
      if (currentLines.length > 0) {
        segments.push({ lines: currentLines, type: "paragraph" });
        currentLines = [];
      }
    }

    if (isHeading) {
      segments.push({ lines: [line], type: "heading" });
    } else if (isList) {
      // Merge consecutive list items into a single list segment
      const prev = segments[segments.length - 1];
      if (prev?.type === "list") {
        prev.lines.push(line);
      } else {
        segments.push({ lines: [line], type: "list" });
      }
    } else {
      currentLines.push(line);
    }
  }

  // Flush any remaining paragraph lines
  if (currentLines.length > 0) {
    segments.push({ lines: currentLines, type: "paragraph" });
  }

  // Convert segments to ContentBlocks
  for (const seg of segments) {
    if (seg.type === "heading") {
      const runs = linesToRuns(seg.lines);
      if (!runs.some((r) => r.text.trim())) continue;

      // Heading level from font size relative to body
      const headingSize = seg.lines[0].fontSize;
      const level =
        headingSize >= bodyFontSize * 1.6
          ? 1
          : headingSize >= bodyFontSize * 1.3
            ? 2
            : 3;

      const { sentences, nextIndex } = splitRunsIntoSentences(
        runs,
        sentenceIdx
      );
      if (sentences.length > 0) {
        blocks.push({ type: "heading", level, sentences });
        sentenceIdx = nextIndex;
      }
    } else if (seg.type === "list") {
      const ordered = /^\d+[.)]/.test(
        seg.lines[0].items.map((it) => it.str).join(" ").trim()
      );
      const items: { sentences: RichSentence[] }[] = [];

      for (const line of seg.lines) {
        const rawRuns = linesToRuns([line]);
        const runs = stripListPrefix(rawRuns);
        if (!runs.some((r) => r.text.trim())) continue;

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
    } else {
      // Paragraph: all lines merged into one block
      const runs = linesToRuns(seg.lines);
      if (!runs.some((r) => r.text.trim())) continue;

      const { sentences, nextIndex } = splitRunsIntoSentences(
        runs,
        sentenceIdx
      );
      if (sentences.length > 0) {
        blocks.push({ type: "paragraph", sentences });
        sentenceIdx = nextIndex;
      }
    }
  }

  return { blocks, nextIndex: sentenceIdx };
}

// ---------------------------------------------------------------------------
// Outline resolution — PDF bookmarks → real page indices
// Uses doc.getDestination() + doc.getPageIndex() instead of hardcoding page 1
// ---------------------------------------------------------------------------

async function resolveOutline(doc: any): Promise<ResolvedOutlineEntry[]> {
  const outline = await doc.getOutline();
  if (!outline?.length) return [];

  const entries: ResolvedOutlineEntry[] = [];

  async function walk(item: any, depth: number) {
    let pageIndex = 0;
    try {
      if (item.dest) {
        let dest = item.dest;
        // Named destination (string) → resolve to explicit destination array
        if (typeof dest === "string") dest = await doc.getDestination(dest);
        // Explicit destination array — first element is the page ref object
        if (Array.isArray(dest) && dest[0]) {
          pageIndex = await doc.getPageIndex(dest[0]);
        }
      }
    } catch {
      // Failed to resolve — keep default page 0
    }

    entries.push({ title: item.title || "Untitled", pageIndex, depth });

    // Recurse into nested sub-items
    if (item.items?.length) {
      for (const child of item.items) await walk(child, depth + 1);
    }
  }

  for (const item of outline) await walk(item, 0);
  return entries;
}

// ---------------------------------------------------------------------------
// Quality assessment — detect scanned / image-only PDFs
// Threshold: average < 50 chars per page → "poor" → trigger fallback
// ---------------------------------------------------------------------------

function assessExtractionQuality(
  blocks: ContentBlock[],
  pageCount: number
): "good" | "poor" {
  let totalChars = 0;
  for (const block of blocks) {
    if (
      block.type === "heading" ||
      block.type === "paragraph" ||
      block.type === "blockquote"
    ) {
      for (const s of block.sentences) totalChars += s.text.length;
    } else if (block.type === "list") {
      for (const item of block.items)
        for (const s of item.sentences) totalChars += s.text.length;
    }
  }

  return totalChars / Math.max(pageCount, 1) < 50 ? "poor" : "good";
}

// ---------------------------------------------------------------------------
// Main loader: PDF binary → PdfChapter[]
// ---------------------------------------------------------------------------

interface PdfLoadResult {
  chapters: PdfChapter[];
  outline: ResolvedOutlineEntry[];
  quality: "good" | "poor";
}

async function loadPdfContent(buffer: ArrayBuffer): Promise<PdfLoadResult> {
  const pdfjs = await import("pdfjs-dist");

  // Set up the PDF.js web worker for off-thread parsing
  if (typeof window !== "undefined") {
    pdfjs.GlobalWorkerOptions.workerSrc = `https://cdnjs.cloudflare.com/ajax/libs/pdf.js/${pdfjs.version}/pdf.worker.min.mjs`;
  }

  const doc = await pdfjs.getDocument({ data: buffer }).promise;
  const numPages = doc.numPages;

  // Resolve outline (bookmarks) with real page indices
  const outline = await resolveOutline(doc);

  // Extract raw text items per page
  const pageData: { lines: PdfLine[]; width: number }[] = [];
  for (let i = 1; i <= numPages; i++) {
    const page = await doc.getPage(i);
    const textContent = await page.getTextContent();
    const viewport = page.getViewport({ scale: 1.0 });
    pageData.push({
      lines: groupIntoLines(extractTextItems(textContent)),
      width: viewport.width,
    });
  }

  // Cross-page header/footer detection (need >= 3 pages)
  const { headers, footers } = detectRepeatedText(
    pageData.map((p) => p.lines)
  );

  // Per-page processing pipeline: column reorder → header/footer strip → hyphen merge
  const processedPages: PdfLine[][] = pageData.map(({ lines, width }) =>
    mergeHyphens(
      filterHeaderFooter(reorderColumns(lines, width), headers, footers)
    )
  );

  // Global body font size for heading threshold computation
  const allLines = processedPages.flat();
  const bodyFontSize = detectBodyFontSize(allLines);

  // Build chapters depending on outline availability
  let sentenceIdx = 0;
  const chapters: PdfChapter[] = [];

  if (outline.length > 0) {
    // ---- With outline: each entry spans from its page to the next entry's page ----

    // Pages before the first outline entry become a "Preamble" chapter
    if (outline[0].pageIndex > 0) {
      const preambleLines: PdfLine[] = [];
      for (let p = 0; p < outline[0].pageIndex; p++) {
        preambleLines.push(...processedPages[p]);
      }
      if (preambleLines.length > 0) {
        const median = computeMedianLineSpacing(preambleLines);
        const { blocks, nextIndex } = linesToBlocks(
          preambleLines,
          bodyFontSize,
          median,
          sentenceIdx
        );
        sentenceIdx = nextIndex;
        if (blocks.length > 0) {
          chapters.push({ id: "pdf-page-1", title: "Preamble", blocks });
        }
      }
    }

    for (let oi = 0; oi < outline.length; oi++) {
      const entry = outline[oi];
      const startPage = entry.pageIndex;
      const endPage =
        oi + 1 < outline.length ? outline[oi + 1].pageIndex : numPages;

      // Collect all lines in the page range for this chapter
      const chapterLines: PdfLine[] = [];
      for (let p = startPage; p < endPage && p < numPages; p++) {
        chapterLines.push(...processedPages[p]);
      }
      if (chapterLines.length === 0) continue;

      const median = computeMedianLineSpacing(chapterLines);
      const { blocks, nextIndex } = linesToBlocks(
        chapterLines,
        bodyFontSize,
        median,
        sentenceIdx
      );
      sentenceIdx = nextIndex;

      if (blocks.length > 0) {
        chapters.push({
          id: `pdf-page-${startPage + 1}`,
          title: entry.title,
          blocks,
        });
      }
    }
  } else {
    // ---- Without outline: each page is a section ----
    for (let p = 0; p < numPages; p++) {
      const pageLines = processedPages[p];
      if (pageLines.length === 0) continue;

      const median = computeMedianLineSpacing(pageLines);
      const { blocks, nextIndex } = linesToBlocks(
        pageLines,
        bodyFontSize,
        median,
        sentenceIdx
      );
      sentenceIdx = nextIndex;

      if (blocks.length > 0) {
        // Use first heading as page title, or fall back to "Page N"
        const firstHeading = blocks.find((b) => b.type === "heading");
        const title =
          firstHeading?.type === "heading"
            ? firstHeading.sentences.map((s) => s.text).join(" ")
            : `Page ${p + 1}`;

        chapters.push({ id: `pdf-page-${p + 1}`, title, blocks });
      }
    }
  }

  // Assess extraction quality across all chapters
  const allBlocks = chapters.flatMap((c) => c.blocks);
  const quality = assessExtractionQuality(allBlocks, numPages);

  return { chapters, outline, quality };
}

// ---------------------------------------------------------------------------
// PdfReader component
// ---------------------------------------------------------------------------

export function PdfReader({
  itemId,
  fallbackContent,
  onSentencesExtracted,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
  onChaptersExtracted,
}: PdfReaderProps) {
  const [chapters, setChapters] = useState<PdfChapter[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [useFallback, setUseFallback] = useState(false);
  const sentencesExtractedRef = useRef(false);

  // Flat sentence array derived from chapters (for TTS)
  const allSentences: Sentence[] = useMemo(() => {
    const result: Sentence[] = [];
    for (const chapter of chapters) {
      for (const block of chapter.blocks) {
        if (
          block.type === "image" ||
          block.type === "separator" ||
          block.type === "code"
        )
          continue;
        if (block.type === "list") {
          for (const item of block.items)
            for (const s of item.sentences)
              result.push({ index: s.index, text: s.text });
        } else {
          for (const s of block.sentences)
            result.push({ index: s.index, text: s.text });
        }
      }
    }
    return result;
  }, [chapters]);

  // Load, parse, and extract structured content from the PDF
  const loadPdf = useCallback(async () => {
    try {
      setLoading(true);
      const buffer = await fetchItemFile(itemId);
      const result = await loadPdfContent(buffer);

      // Poor extraction quality (scanned PDF) → fall back to PlainTextReader
      if (result.quality === "poor" && fallbackContent) {
        setUseFallback(true);
        return;
      }

      if (result.chapters.length === 0) {
        if (fallbackContent) {
          setUseFallback(true);
          return;
        }
        throw new Error("No text content found in PDF");
      }

      setChapters(result.chapters);

      // Report TOC chapters — prefer outline (has depth) over page-based chapters
      onChaptersExtracted?.(
        result.outline.length > 0
          ? result.outline.map((e) => ({
              id: `pdf-page-${e.pageIndex + 1}`,
              title: e.title,
              depth: e.depth,
            }))
          : result.chapters.map((c) => ({ id: c.id, title: c.title }))
      );
    } catch (err) {
      console.error("PDF load error:", err);
      setError(err instanceof Error ? err.message : "Failed to load PDF");
    } finally {
      setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [itemId]);

  useEffect(() => {
    loadPdf();
  }, [loadPdf]);

  // Emit extracted sentences to parent (once per load)
  useEffect(() => {
    if (allSentences.length > 0 && !sentencesExtractedRef.current) {
      sentencesExtractedRef.current = true;
      onSentencesExtracted(allSentences);
    }
  }, [allSentences, onSentencesExtracted]);

  // Auto-scroll to the currently active sentence during TTS playback
  useEffect(() => {
    if (currentSentenceIndex < 0) return;
    const el = document.querySelector(
      `[data-sentence-id="${currentSentenceIndex}"]`
    );
    if (el) {
      el.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, [currentSentenceIndex]);

  // --- Fallback: scanned PDF or poor extraction → PlainTextReader ---
  if (useFallback && fallbackContent) {
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
        <p>Failed to load PDF: {error}</p>
      </div>
    );
  }

  // --- Main render: text reflow via shared BlockRenderer ---
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
