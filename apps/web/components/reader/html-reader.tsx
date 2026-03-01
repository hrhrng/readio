"use client";

/**
 * HtmlReader — renders trafilatura-extracted HTML with rich formatting and
 * full TTS integration (sentence highlighting, word-level progress).
 *
 * Self-contained: does NOT depend on epub-reader or markdown-reader.
 *
 * Pipeline:
 *   HTML string (from trafilatura)
 *     → DOMParser.parseFromString()   → Document
 *     → DOMPurify.sanitize()          → clean DOM
 *     → walk DOM body children         → extractStyledRuns + splitRunsIntoSentences
 *     → ContentBlock[]                 + flat Sentence[]
 *     → BlockRenderer                  (from content-blocks.tsx)
 */

import { useEffect, useRef, useMemo } from "react";
import DOMPurify from "dompurify";
import { Sentence } from "@/lib/types";
import {
  type StyledRun,
  type RichSentence,
  type ContentBlock,
  splitRunsIntoSentences,
  BlockRenderer,
} from "./content-blocks";

// ---------------------------------------------------------------------------
// DOM → StyledRun[] extraction (preserves bold/italic/href from inline elements)
// ---------------------------------------------------------------------------

/**
 * Recursively extract styled text runs from a DOM node, inheriting
 * bold/italic state from ancestor elements like <b>, <strong>, <em>, <i>.
 *
 * This is intentionally a local copy — each reader owns its DOM extraction
 * logic to avoid cross-format coupling.
 *
 * trafilatura quirk: it uses <pre> for inline code (e.g. `<pre>chat</pre>`),
 * so short single-line <pre> elements are treated as inline bold runs rather
 * than block-level code.
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

  // trafilatura uses <pre> for inline code — treat short, single-line <pre>
  // as inline bold text (like <code>) instead of a block code element.
  if (tag === "pre" && !el.querySelector("pre")) {
    const text = el.textContent || "";
    if (text && !text.includes("\n")) {
      return [{ text, bold: true }];
    }
  }

  const isBold = inheritBold || tag === "b" || tag === "strong";
  const isItalic = inheritItalic || tag === "i" || tag === "em";
  const currentHref =
    tag === "a" ? el.getAttribute("href") || inheritHref : inheritHref;

  const runs: StyledRun[] = [];
  for (const child of Array.from(el.childNodes)) {
    runs.push(...extractStyledRuns(child, isBold, isItalic, currentHref));
  }
  return runs;
}

/**
 * Check if a <pre> element is a real multi-line code block (not trafilatura's
 * inline code usage). Returns true for multi-line or nested-pre blocks.
 */
function isBlockLevelPre(el: Element): boolean {
  const text = el.textContent || "";
  return text.includes("\n") || !!el.querySelector("pre");
}

// ---------------------------------------------------------------------------
// HTML DOM → ContentBlock[] extraction
// ---------------------------------------------------------------------------

/**
 * Walk the sanitized DOM body and produce ContentBlock[] + flat Sentence[].
 *
 * Handles trafilatura's semantic HTML output:
 *   <h1>–<h6>, <p>, <ul>/<ol>, <blockquote>, <pre>, <hr>,
 *   <img>, <graphic> (trafilatura's image tag), <figure>.
 */
function parseHtmlToBlocks(html: string): {
  blocks: ContentBlock[];
  sentences: Sentence[];
} {
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, "text/html");

  // Sanitize: allow semantic tags, strip scripts/event handlers.
  // Use RETURN_DOM_FRAGMENT for a DocumentFragment with .children access.
  const sanitizedHtml = DOMPurify.sanitize(doc.body.innerHTML, {
    ADD_TAGS: ["graphic"],       // trafilatura uses <graphic> for images
    ADD_ATTR: ["src", "alt", "href", "title"],
  });
  // Re-parse sanitized HTML to get a proper DOM tree for walking
  const cleanDoc = parser.parseFromString(sanitizedHtml, "text/html");
  const clean = cleanDoc.body;

  const blocks: ContentBlock[] = [];
  const sentences: Sentence[] = [];
  let sentenceIdx = 0;

  /** Helper: runs → rich sentences, advancing global index. */
  function processRuns(runs: StyledRun[]): RichSentence[] {
    const { sentences: rich, nextIndex } = splitRunsIntoSentences(runs, sentenceIdx);
    sentenceIdx = nextIndex;
    for (const s of rich) {
      sentences.push({ index: s.index, text: s.text });
    }
    return rich;
  }

  // --- Block-level tag set: elements that interrupt inline flow ---
  const BLOCK_TAGS = new Set([
    "h1", "h2", "h3", "h4", "h5", "h6",
    "p", "ul", "ol", "li", "blockquote", "hr",
    "figure", "table", "div", "section", "article", "aside", "main",
    "header", "footer", "nav", "details", "summary",
  ]);

  /** Check if a node is a block-level element. */
  function isBlockElement(node: Node): boolean {
    if (node.nodeType !== Node.ELEMENT_NODE) return false;
    const tag = (node as Element).tagName.toLowerCase();
    if (BLOCK_TAGS.has(tag)) return true;
    // Multi-line <pre> is block-level; short inline <pre> is not
    if (tag === "pre") return isBlockLevelPre(node as Element);
    return false;
  }

  /** Flush accumulated inline runs as a paragraph block. */
  function flushInlineRuns(runs: StyledRun[]): void {
    if (runs.length === 0) return;
    const rich = processRuns(runs);
    if (rich.length > 0) {
      blocks.push({ type: "paragraph", sentences: rich });
    }
  }

  /** Process a single block-level DOM element into ContentBlocks. */
  function processBlockElement(el: Element): void {
    const tag = el.tagName.toLowerCase();

    // --- Headings ---
    if (/^h[1-6]$/.test(tag)) {
      const runs = extractStyledRuns(el, false, false);
      const rich = processRuns(runs);
      if (rich.length > 0) {
        blocks.push({ type: "heading", level: parseInt(tag[1]), sentences: rich });
      }
      return;
    }

    // --- Images: <img> and trafilatura's <graphic> tag ---
    if (tag === "img" || tag === "graphic") {
      const src = el.getAttribute("src");
      if (src) {
        blocks.push({ type: "image", src, alt: el.getAttribute("alt") || undefined });
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
        const rich = processRuns(runs);
        if (rich.length > 0) {
          items.push({ sentences: rich });
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
      const rich = processRuns(runs);
      if (rich.length > 0) {
        blocks.push({ type: "blockquote", sentences: rich });
      }
      return;
    }

    // --- Multi-line <pre>: real code block ---
    if (tag === "pre" && isBlockLevelPre(el)) {
      const text = el.textContent || "";
      if (text.trim()) {
        blocks.push({ type: "code", text: text.trim() });
      }
      return;
    }

    // --- Paragraphs ---
    if (tag === "p") {
      const imgs = el.querySelectorAll("img, graphic");
      if (imgs.length > 0 && !el.textContent?.trim()) {
        for (const img of Array.from(imgs)) {
          const src = img.getAttribute("src");
          if (src) {
            blocks.push({ type: "image", src, alt: img.getAttribute("alt") || undefined });
          }
        }
        return;
      }
      const runs = extractStyledRuns(el, false, false);
      const rich = processRuns(runs);
      if (rich.length > 0) {
        blocks.push({ type: "paragraph", sentences: rich });
      }
      return;
    }

    // --- Figure ---
    if (tag === "figure") {
      const img = el.querySelector("img, graphic");
      if (img) {
        const src = img.getAttribute("src");
        if (src) {
          blocks.push({ type: "image", src, alt: img.getAttribute("alt") || undefined });
        }
      }
      const caption = el.querySelector("figcaption");
      if (caption) {
        const runs = extractStyledRuns(caption, false, true);
        const rich = processRuns(runs);
        if (rich.length > 0) {
          blocks.push({ type: "paragraph", sentences: rich });
        }
      }
      return;
    }

    // --- Generic container (div, section, article, etc.) → walk children ---
    walkChildren(el);
  }

  /**
   * Walk the childNodes of a container, collecting adjacent inline nodes
   * (text + inline elements like <a>, <b>, <em>, inline <pre>) into
   * paragraph blocks, and dispatching block-level children individually.
   *
   * This handles trafilatura's quirk of placing bare text mixed with inline
   * <pre> and <a> tags directly under <body> without wrapping <p> tags.
   */
  function walkChildren(container: Element): void {
    let pendingRuns: StyledRun[] = [];

    for (const child of Array.from(container.childNodes)) {
      if (child.nodeType === Node.TEXT_NODE) {
        // Bare text node — accumulate as inline run
        const text = child.textContent || "";
        if (text) {
          pendingRuns.push({ text });
        }
        continue;
      }

      if (child.nodeType !== Node.ELEMENT_NODE) continue;
      const childEl = child as Element;

      if (isBlockElement(childEl)) {
        // Block element breaks the inline flow — flush pending runs first
        flushInlineRuns(pendingRuns);
        pendingRuns = [];
        processBlockElement(childEl);
      } else {
        // Inline element (<a>, <b>, <em>, <pre> for inline code, etc.)
        // — extract runs and accumulate with surrounding text
        pendingRuns.push(...extractStyledRuns(childEl, false, false));
      }
    }

    // Flush any trailing inline runs
    flushInlineRuns(pendingRuns);
  }

  // Start by walking the sanitized body's children
  walkChildren(clean);

  return { blocks, sentences };
}

// ---------------------------------------------------------------------------
// HtmlReader component
// ---------------------------------------------------------------------------

interface HtmlReaderProps {
  content: string;
  onSentencesExtracted: (sentences: Sentence[]) => void;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
}

export function HtmlReader({
  content,
  onSentencesExtracted,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
}: HtmlReaderProps) {
  const extractedRef = useRef(false);

  const { blocks, sentences } = useMemo(
    () => parseHtmlToBlocks(content),
    [content]
  );

  // Emit extracted sentences to the parent (once per content change)
  useEffect(() => {
    if (!extractedRef.current && sentences.length > 0) {
      extractedRef.current = true;
      onSentencesExtracted(sentences);
    }
  }, [sentences, onSentencesExtracted]);

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

  return (
    <div className="space-y-0">
      {blocks.map((block, bi) => (
        <BlockRenderer
          key={bi}
          block={block}
          currentSentenceIndex={currentSentenceIndex}
          currentWordProgress={currentWordProgress}
          onSentenceClick={onSentenceClick}
        />
      ))}
    </div>
  );
}
