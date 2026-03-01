"use client";

import { useEffect, useRef, useState, useCallback, useMemo, memo } from "react";
import { fetchItemFile } from "@/lib/api";
import { Sentence } from "@/lib/types";
import { BOUNDARY } from "@/lib/sentences";
import { tokenize, countTokens } from "@/lib/cjk";
import {
  type StyledRun,
  type RichSentence,
  type ContentBlock,
  splitRunsIntoSentences,
  SentenceSpan,
  BlockRenderer,
  renderSentences,
  getBlockSentenceRange,
} from "./content-blocks";

interface EpubChapter {
  id: string;
  title: string;
  blocks: ContentBlock[];
}

// ---------------------------------------------------------------------------
// Props — identical to the old EpubReader, so ContentRouter needs no changes
// ---------------------------------------------------------------------------

/** A single entry in the navigation TOC, with hierarchical depth for indentation. */
interface NavTocEntry {
  id: string;     // scroll target: "chapter-5" or "chapter-5#anchor"
  title: string;
  depth: number;  // 0 = top-level, 1 = sub-item, 2 = sub-sub-item, ...
}

/**
 * Recursively build a flat list of NavTocEntry from epub.js navigation.toc,
 * mapping each entry's href to the corresponding chapter element id.
 */
function buildNavToc(
  entries: any[],
  hrefToChapterMap: Map<string, string>,
  depth: number = 0
): NavTocEntry[] {
  const result: NavTocEntry[] = [];
  for (const entry of entries) {
    const rawHref = entry.href || "";
    const [file, anchor] = rawHref.split("#");
    const label = (entry.label || "").trim();
    if (!label) continue;

    // Try both the full href path and basename for matching
    const fileBasename = file.split("/").pop() || file;
    const chapterId = hrefToChapterMap.get(file) || hrefToChapterMap.get(fileBasename);
    if (chapterId) {
      result.push({
        id: anchor ? `${chapterId}#${anchor}` : chapterId,
        title: label,
        depth,
      });
    }

    // Recurse into child items (epub.js stores them as `subitems`)
    if (entry.subitems?.length) {
      result.push(...buildNavToc(entry.subitems, hrefToChapterMap, depth + 1));
    }
  }
  return result;
}

interface EpubReaderProps {
  itemId: string;
  fallbackContent?: string;
  onSentencesExtracted: (sentences: Sentence[]) => void;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
  onChaptersExtracted?: (chapters: { id: string; title: string; depth?: number }[]) => void;
}

// splitRunsIntoSentences, SentenceSpan, BlockRenderer, renderSentences,
// getBlockSentenceRange are imported from ./content-blocks

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
  inheritItalic: boolean,
  inheritHref?: string
): StyledRun[] {
  if (node.nodeType === Node.TEXT_NODE) {
    const text = node.textContent || "";
    if (!text) return [];
    return [
      {
        text,
        ...(inheritBold ? { bold: true } : {}),
        ...(inheritItalic ? { italic: true } : {}),
        ...(inheritHref ? { href: inheritHref } : {}),
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

  // Capture href from <a> tags and propagate to all child runs
  const currentHref =
    tag === "a" ? el.getAttribute("href") || inheritHref : inheritHref;

  const runs: StyledRun[] = [];
  for (const child of Array.from(el.childNodes)) {
    runs.push(...extractStyledRuns(child, isBold, isItalic, currentHref));
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

    // Capture element id for anchor-based navigation (e.g. <h2 id="section5">)
    const anchorId = el.getAttribute("id") || undefined;

    // --- Headings ---
    if (/^h[1-6]$/.test(tag)) {
      const level = parseInt(tag[1]);
      const runs = extractStyledRuns(el, false, false);
      const { sentences, nextIndex } = splitRunsIntoSentences(runs, sentenceIdx);
      if (sentences.length > 0) {
        blocks.push({ type: "heading", level, sentences, anchorId });
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
        blocks.push({ type: "list", ordered, items, anchorId });
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
        blocks.push({ type: "blockquote", sentences, anchorId });
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
        blocks.push({ type: "paragraph", sentences, anchorId });
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
        blocks.push({ type: "paragraph", sentences, anchorId });
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

// SentenceSpan, BlockRenderer, renderSentences, getBlockSentenceRange
// are now imported from ./content-blocks

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
  /** Maps spine section href (and basename) → chapter id for internal link navigation. */
  hrefToChapterMap: Map<string, string>;
  /** Cover blob URL if present (caller should revoke on unmount). */
  coverUrl?: string;
  /** Hierarchical TOC built from epub.js navigation (NCX/nav). */
  navToc: NavTocEntry[];
}

/**
 * Recursively flatten nested TOC entries from epub.js navigation.
 * epub.js stores child items in `entry.subitems`.
 */
function flattenToc(entries: any[]): { href: string; label: string }[] {
  const result: { href: string; label: string }[] = [];
  for (const entry of entries) {
    const href = (entry.href || "").split("#")[0];
    const label = (entry.label || "").trim();
    if (href && label) result.push({ href, label });
    if (entry.subitems && Array.isArray(entry.subitems)) {
      result.push(...flattenToc(entry.subitems));
    }
  }
  return result;
}

async function loadEpubChapters(
  buffer: ArrayBuffer
): Promise<EpubLoadResult> {
  const ePub = (await import("epubjs")).default;
  const book = ePub(buffer);

  // Wait for book metadata and navigation to load
  await book.ready;
  await book.loaded.navigation;

  // --- Cover image: fetch blob URL before processing spine ---
  // Use a timeout to prevent hanging on EPUBs where coverUrl() never resolves
  let coverUrl: string | undefined;
  try {
    const url = await Promise.race([
      book.coverUrl(),
      new Promise<null>((resolve) => setTimeout(() => resolve(null), 3000)),
    ]);
    if (url) coverUrl = url;
  } catch {
    // Some EPUBs don't have a cover — that's fine
  }

  // --- Build TOC label lookup with multi-strategy matching ---
  // Two maps: exact href → label, and basename → label (fallback)
  const tocByHref = new Map<string, string>();
  const tocByBasename = new Map<string, string>();
  const navToc = (book.navigation as any)?.toc;
  if (navToc && Array.isArray(navToc)) {
    for (const { href, label } of flattenToc(navToc)) {
      tocByHref.set(href, label);
      // basename fallback: "OEBPS/Text/ch1.xhtml" → "ch1.xhtml"
      const basename = href.split("/").pop() || href;
      if (!tocByBasename.has(basename)) {
        tocByBasename.set(basename, label);
      }
    }
  }

  const chapters: EpubChapter[] = [];
  const hrefToChapterMap = new Map<string, string>();
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

    // Determine chapter title: prefer TOC label (exact match, then basename fallback),
    // then first heading in content, then generic "Chapter N"
    const sectionHref = (section.href || "").split("#")[0];
    const sectionBasename = sectionHref.split("/").pop() || sectionHref;
    let title =
      tocByHref.get(sectionHref) ||
      tocByBasename.get(sectionBasename) ||
      "";
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

    const chapterId = `chapter-${chapters.length + 1}`;
    chapters.push({ id: chapterId, title, blocks });

    // Build href → chapter id mapping for internal link navigation
    hrefToChapterMap.set(sectionHref, chapterId);
    if (sectionBasename !== sectionHref) {
      hrefToChapterMap.set(sectionBasename, chapterId);
    }
  }

  // --- Insert cover chapter at the beginning if cover image exists ---
  if (coverUrl) {
    const coverChapter: EpubChapter = {
      id: "cover",
      title: "Cover",
      blocks: [{ type: "image", src: coverUrl }],
    };
    chapters.unshift(coverChapter);
  }

  // Build hierarchical navigation TOC from epub.js's parsed NCX/nav document.
  // Falls back to empty array for EPUBs without navigation metadata.
  const navEntries = navToc && Array.isArray(navToc)
    ? buildNavToc(navToc, hrefToChapterMap)
    : [];

  // Clean up epub.js resources (blob URLs survive destroy)
  book.destroy();

  return { chapters, hrefToChapterMap, coverUrl, navToc: navEntries };
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
  const [hrefToChapterMap, setHrefToChapterMap] = useState<Map<string, string>>(
    new Map()
  );

  // Track blob URLs from EPUB images and revoke them on unmount to prevent memory leaks.
  // Declared early so loadEpub can add the cover blob URL.
  const blobUrlsRef = useRef<Set<string>>(new Set());

  // Collect all sentences across all chapters into a flat array (for TTS)
  const allSentences: Sentence[] = useMemo(() => {
    const result: Sentence[] = [];
    for (const chapter of chapters) {
      for (const block of chapter.blocks) {
        if (block.type === "image" || block.type === "separator" || block.type === "code") continue;
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
      setHrefToChapterMap(result.hrefToChapterMap);

      // Track cover blob URL for cleanup
      if (result.coverUrl) {
        blobUrlsRef.current.add(result.coverUrl);
      }

      // Prefer the hierarchical navigation TOC (NCX/nav) for meaningful entries;
      // fall back to spine-based chapters only if navToc is empty (rare EPUBs).
      onChaptersExtracted?.(
        result.navToc.length > 0
          ? result.navToc
          : result.chapters.map((c) => ({ id: c.id, title: c.title }))
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

  // Collect image blob URLs from chapters and revoke all on unmount
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

  // Handle internal EPUB link clicks: resolve href → chapter, scroll to target
  const handleLinkClick = useCallback(
    (href: string) => {
      const targetFile = href.split("#")[0];
      const anchor = href.split("#")[1];
      // Try exact match first, then basename fallback
      const chapterId =
        hrefToChapterMap.get(targetFile) ||
        hrefToChapterMap.get(targetFile.split("/").pop() || "");
      if (chapterId) {
        // If there's an anchor fragment, try to find the specific element first
        const el = anchor
          ? document.querySelector(
              `[data-chapter="${chapterId}"] [id="${anchor}"]`
            )
          : document.querySelector(`[data-chapter="${chapterId}"]`);
        el?.scrollIntoView({ behavior: "smooth", block: "start" });
      }
    },
    [hrefToChapterMap]
  );

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
                onLinkClick={handleLinkClick}
              />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
