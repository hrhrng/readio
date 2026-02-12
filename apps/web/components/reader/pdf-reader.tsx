"use client";

import {
  useEffect,
  useRef,
  useState,
  useCallback,
} from "react";
import { fetchItemFile } from "@/lib/api";
import { extractSentencesFromText } from "@/lib/sentences";
import { Sentence } from "@/lib/types";

interface PdfReaderProps {
  itemId: string;
  fallbackContent?: string;
  onSentencesExtracted: (sentences: Sentence[]) => void;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
  onOutlineExtracted?: (
    outline: { title: string; page: number }[]
  ) => void;
}

export function PdfReader({
  itemId,
  fallbackContent,
  onSentencesExtracted,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
  onOutlineExtracted,
}: PdfReaderProps) {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [pageCount, setPageCount] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);
  const pdfDocRef = useRef<any>(null);
  const renderedPages = useRef<Set<number>>(new Set());
  const sentencesRef = useRef<Sentence[]>([]);

  const loadPdf = useCallback(async () => {
    try {
      setLoading(true);
      const buffer = await fetchItemFile(itemId);
      const pdfjs = await import("pdfjs-dist");

      // Set worker
      if (typeof window !== "undefined") {
        pdfjs.GlobalWorkerOptions.workerSrc = `https://cdnjs.cloudflare.com/ajax/libs/pdf.js/${pdfjs.version}/pdf.worker.min.mjs`;
      }

      const doc = await pdfjs.getDocument({ data: buffer }).promise;
      pdfDocRef.current = doc;
      setPageCount(doc.numPages);

      // Extract outline
      try {
        const outline = await doc.getOutline();
        if (outline && outline.length > 0) {
          const items = outline.map((item: any) => ({
            title: item.title || "Untitled",
            page: 1,
          }));
          onOutlineExtracted?.(items);
        }
      } catch {
        // Outline extraction is optional
      }

      // Extract all text for sentences
      let allText = "";
      for (let i = 1; i <= doc.numPages; i++) {
        const page = await doc.getPage(i);
        const textContent = await page.getTextContent();
        const pageText = textContent.items
          .map((item: any) => item.str)
          .join(" ");
        if (pageText.trim()) {
          allText += pageText + "\n\n";
        }
      }

      const sentences = extractSentencesFromText(allText);
      sentencesRef.current = sentences;
      onSentencesExtracted(sentences);
    } catch (err) {
      console.error("PDF load error:", err);
      setError(
        err instanceof Error ? err.message : "Failed to load PDF"
      );
    } finally {
      setLoading(false);
    }
  }, [itemId, onSentencesExtracted, onOutlineExtracted]);

  useEffect(() => {
    loadPdf();
  }, [loadPdf]);

  // Render visible pages
  const renderPage = useCallback(
    async (pageNum: number) => {
      if (!pdfDocRef.current || renderedPages.current.has(pageNum)) return;
      renderedPages.current.add(pageNum);

      const page = await pdfDocRef.current.getPage(pageNum);
      const scale = 1.5;
      const viewport = page.getViewport({ scale });

      const pageEl = containerRef.current?.querySelector(
        `[data-page="${pageNum}"]`
      );
      if (!pageEl) return;

      const canvas = document.createElement("canvas");
      canvas.width = viewport.width;
      canvas.height = viewport.height;
      canvas.style.width = "100%";
      canvas.style.height = "auto";
      canvas.style.display = "block";

      const ctx = canvas.getContext("2d")!;
      await page.render({ canvasContext: ctx, viewport }).promise;

      // Clear placeholder and add canvas
      pageEl.innerHTML = "";
      pageEl.appendChild(canvas);
    },
    []
  );

  // IntersectionObserver for lazy page rendering
  useEffect(() => {
    if (pageCount === 0 || !containerRef.current) return;

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            const pageNum = parseInt(
              (entry.target as HTMLElement).dataset.page || "0"
            );
            if (pageNum > 0) {
              renderPage(pageNum);
              // Also render adjacent pages
              if (pageNum > 1) renderPage(pageNum - 1);
              if (pageNum < pageCount) renderPage(pageNum + 1);
            }
          }
        }
      },
      { rootMargin: "200px" }
    );

    const pageEls = containerRef.current.querySelectorAll("[data-page]");
    pageEls.forEach((el) => observer.observe(el));

    return () => observer.disconnect();
  }, [pageCount, renderPage]);

  if (loading) {
    return (
      <div className="space-y-4 animate-pulse">
        {Array.from({ length: 6 }).map((_, i) => (
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

  return (
    <div>
      <div className="mb-4 px-4 py-2 bg-surface-hover rounded-lg text-sm text-text-secondary">
        PDF rendered from original file. Some complex layouts may vary.
      </div>
      <div ref={containerRef} className="space-y-4">
        {Array.from({ length: pageCount }).map((_, i) => (
          <div
            key={i + 1}
            data-page={i + 1}
            className="bg-white dark:bg-surface-card rounded-lg shadow-sm overflow-hidden min-h-[400px] flex items-center justify-center"
          >
            <div className="animate-pulse text-text-tertiary text-sm">
              Page {i + 1}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
