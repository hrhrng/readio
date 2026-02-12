"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import { fetchItemFile } from "@/lib/api";
import { wrapSentencesInDOM } from "@/lib/sentences";
import { Sentence } from "@/lib/types";

interface EpubChapter {
  id: string;
  title: string;
  html: string;
  images: Map<string, string>; // relative path -> blob URL
}

interface EpubReaderProps {
  itemId: string;
  fallbackContent?: string;
  onSentencesExtracted: (sentences: Sentence[]) => void;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
  onChaptersExtracted?: (chapters: { id: string; title: string }[]) => void;
}

const ALLOWED_TAGS = new Set([
  "p",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "em",
  "strong",
  "span",
  "img",
  "ul",
  "ol",
  "li",
  "blockquote",
  "br",
  "div",
  "a",
  "table",
  "tr",
  "td",
  "th",
  "figure",
  "figcaption",
  "sup",
  "sub",
  "i",
  "b",
]);

function sanitizeHTML(html: string): string {
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, "text/html");
  const body = doc.body;

  function clean(node: Node): Node | null {
    if (node.nodeType === Node.TEXT_NODE) return node.cloneNode();
    if (node.nodeType !== Node.ELEMENT_NODE) return null;

    const el = node as HTMLElement;
    const tag = el.tagName.toLowerCase();

    if (!ALLOWED_TAGS.has(tag)) {
      // Keep children of disallowed tags
      const frag = document.createDocumentFragment();
      for (const child of Array.from(el.childNodes)) {
        const cleaned = clean(child);
        if (cleaned) frag.appendChild(cleaned);
      }
      return frag;
    }

    const newEl = document.createElement(tag);
    // Only copy safe attributes
    if (tag === "img") {
      const src = el.getAttribute("src");
      if (src) newEl.setAttribute("src", src);
      const alt = el.getAttribute("alt");
      if (alt) newEl.setAttribute("alt", alt);
    } else if (tag === "a") {
      const href = el.getAttribute("href");
      if (href && !href.startsWith("javascript:")) {
        newEl.setAttribute("href", href);
        newEl.setAttribute("target", "_blank");
        newEl.setAttribute("rel", "noopener noreferrer");
      }
    }

    for (const child of Array.from(el.childNodes)) {
      const cleaned = clean(child);
      if (cleaned) newEl.appendChild(cleaned);
    }
    return newEl;
  }

  const result = document.createElement("div");
  for (const child of Array.from(body.childNodes)) {
    const cleaned = clean(child);
    if (cleaned) result.appendChild(cleaned);
  }
  return result.innerHTML;
}

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
  const containerRef = useRef<HTMLDivElement>(null);
  const sentencesRef = useRef<Sentence[]>([]);
  const processedRef = useRef(false);

  const loadEpub = useCallback(async () => {
    try {
      setLoading(true);
      const buffer = await fetchItemFile(itemId);
      const JSZip = (await import("jszip")).default;
      const zip = await JSZip.loadAsync(buffer);

      // Find OPF
      let opfPath = "";
      const containerXml = zip.file("META-INF/container.xml");
      if (containerXml) {
        const containerText = await containerXml.async("text");
        const match = containerText.match(/full-path="([^"]+)"/);
        if (match) opfPath = match[1];
      }
      if (!opfPath) {
        const opfFile = Object.keys(zip.files).find((n) =>
          n.toLowerCase().endsWith(".opf")
        );
        if (opfFile) opfPath = opfFile;
      }
      if (!opfPath) throw new Error("No OPF found in EPUB");

      const opfDir = opfPath.includes("/")
        ? opfPath.substring(0, opfPath.lastIndexOf("/") + 1)
        : "";
      const opfText = await zip.file(opfPath)!.async("text");
      const parser = new DOMParser();
      const opfDoc = parser.parseFromString(opfText, "application/xml");

      // Build manifest map
      const manifest = new Map<string, { href: string; type: string }>();
      opfDoc.querySelectorAll("manifest > item").forEach((item) => {
        const id = item.getAttribute("id") || "";
        const href = item.getAttribute("href") || "";
        const mediaType = item.getAttribute("media-type") || "";
        if (id && href) manifest.set(id, { href, type: mediaType });
      });

      // Get spine order
      const spineIds: string[] = [];
      opfDoc.querySelectorAll("spine > itemref").forEach((ref) => {
        const idref = ref.getAttribute("idref");
        if (idref) spineIds.push(idref);
      });

      const loadedChapters: EpubChapter[] = [];
      for (const spineId of spineIds) {
        const item = manifest.get(spineId);
        if (!item) continue;
        const fullPath = opfDir + item.href;
        const file = zip.file(fullPath);
        if (!file) continue;

        let html = await file.async("text");
        // Extract body content
        const bodyMatch = html.match(/<body[^>]*>([\s\S]*?)<\/body>/i);
        const bodyHtml = bodyMatch ? bodyMatch[1] : html;
        const cleanHtml = sanitizeHTML(bodyHtml);

        // Resolve images
        const imgDir = fullPath.includes("/")
          ? fullPath.substring(0, fullPath.lastIndexOf("/") + 1)
          : "";
        const images = new Map<string, string>();
        const imgMatches = cleanHtml.matchAll(/src="([^"]+)"/g);
        for (const m of imgMatches) {
          const src = m[1];
          if (src.startsWith("data:") || src.startsWith("blob:")) continue;
          const imgPath = imgDir + src;
          const imgFile = zip.file(imgPath);
          if (imgFile) {
            const imgData = await imgFile.async("arraybuffer");
            const blob = new Blob([imgData]);
            images.set(src, URL.createObjectURL(blob));
          }
        }

        // Extract title from first heading
        const titleMatch = cleanHtml.match(/<h[1-3][^>]*>(.*?)<\/h[1-3]>/i);
        const title = titleMatch
          ? titleMatch[1].replace(/<[^>]+>/g, "").trim()
          : `Chapter ${loadedChapters.length + 1}`;

        loadedChapters.push({
          id: `chapter-${loadedChapters.length + 1}`,
          title,
          html: cleanHtml,
          images,
        });
      }

      if (loadedChapters.length === 0) throw new Error("No chapters found");
      setChapters(loadedChapters);
      onChaptersExtracted?.(
        loadedChapters.map((c) => ({ id: c.id, title: c.title }))
      );
    } catch (err) {
      console.error("EPUB load error:", err);
      setError(
        err instanceof Error ? err.message : "Failed to load EPUB"
      );
    } finally {
      setLoading(false);
    }
  }, [itemId, onChaptersExtracted]);

  useEffect(() => {
    loadEpub();
  }, [loadEpub]);

  // Process sentences after chapters render
  useEffect(() => {
    if (chapters.length === 0 || processedRef.current || !containerRef.current)
      return;

    // Wait for DOM to be ready
    requestAnimationFrame(() => {
      if (!containerRef.current) return;
      const allSentences: Sentence[] = [];
      const chapterEls =
        containerRef.current.querySelectorAll("[data-chapter]");
      chapterEls.forEach((chapterEl) => {
        const contentEl = chapterEl.querySelector(".epub-content");
        if (contentEl) {
          const sents = wrapSentencesInDOM(
            contentEl as HTMLElement,
            allSentences.length
          );
          allSentences.push(...sents);
        }
      });

      // Add click handlers
      containerRef.current
        .querySelectorAll("[data-sentence-id]")
        .forEach((el) => {
          const idx = parseInt(el.getAttribute("data-sentence-id") || "0");
          el.addEventListener("click", () => onSentenceClick(idx));
          (el as HTMLElement).style.cursor = "pointer";
        });

      sentencesRef.current = allSentences;
      processedRef.current = true;
      onSentencesExtracted(allSentences);
    });
  }, [chapters, onSentencesExtracted, onSentenceClick]);

  // Highlight current sentence
  useEffect(() => {
    if (!containerRef.current) return;
    containerRef.current
      .querySelectorAll("[data-sentence-id]")
      .forEach((el) => {
        const idx = parseInt(el.getAttribute("data-sentence-id") || "-1");
        const htmlEl = el as HTMLElement;
        if (idx === currentSentenceIndex) {
          htmlEl.style.backgroundColor = "var(--highlight-sentence)";
          htmlEl.style.borderRadius = "2px";
          el.scrollIntoView({ behavior: "smooth", block: "center" });
        } else {
          htmlEl.style.backgroundColor = "";
          htmlEl.style.borderRadius = "";
        }
      });
  }, [currentSentenceIndex]);

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

  if (error) {
    if (fallbackContent) {
      // Import PlainTextReader dynamically to avoid circular
      const PlainTextReaderFallback = require("./plain-text-reader").PlainTextReader;
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

  return (
    <div ref={containerRef} className="space-y-8">
      {chapters.map((chapter, idx) => {
        // Replace image src with blob URLs
        let processedHtml = chapter.html;
        chapter.images.forEach((blobUrl, origSrc) => {
          processedHtml = processedHtml.replaceAll(
            `src="${origSrc}"`,
            `src="${blobUrl}"`
          );
        });

        return (
          <div key={chapter.id} data-chapter={chapter.id}>
            {idx > 0 && (
              <hr className="border-border my-8" />
            )}
            <h2 className="text-xl font-semibold text-text-primary mb-4 font-serif">
              {chapter.title}
            </h2>
            <div
              className="epub-content text-lg leading-relaxed text-text-primary [&_img]:max-w-full [&_img]:h-auto [&_img]:rounded-lg [&_img]:my-4 [&_p]:mb-4 [&_h1]:text-2xl [&_h1]:font-bold [&_h1]:mb-3 [&_h2]:text-xl [&_h2]:font-semibold [&_h2]:mb-3 [&_h3]:text-lg [&_h3]:font-medium [&_h3]:mb-2 [&_blockquote]:border-l-4 [&_blockquote]:border-border [&_blockquote]:pl-4 [&_blockquote]:italic [&_blockquote]:text-text-secondary [&_ul]:list-disc [&_ul]:pl-6 [&_ul]:mb-4 [&_ol]:list-decimal [&_ol]:pl-6 [&_ol]:mb-4 [&_li]:mb-1 [&_a]:text-accent [&_a]:underline"
              dangerouslySetInnerHTML={{ __html: processedHtml }}
            />
          </div>
        );
      })}
    </div>
  );
}
