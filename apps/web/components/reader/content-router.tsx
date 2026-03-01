"use client";

import { LibraryItem, Sentence } from "@/lib/types";
import { PlainTextReader } from "./plain-text-reader";
import { EpubReader } from "./epub-reader";
import { PdfReader } from "./pdf-reader";
import { HtmlReader } from "./html-reader";

interface ContentRouterProps {
  item: LibraryItem;
  onSentencesExtracted: (sentences: Sentence[]) => void;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
  onChaptersExtracted?: (chapters: { id: string; title: string; depth?: number }[]) => void;
}

export function ContentRouter({
  item,
  onSentencesExtracted,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
  onChaptersExtracted,
}: ContentRouterProps) {
  if (!item.content && !item.file_path) {
    return (
      <div className="text-center py-12 text-text-secondary">
        <p className="text-lg">No readable content</p>
      </div>
    );
  }

  if (item.type === "epub" && item.file_path) {
    return (
      <EpubReader
        itemId={item.id}
        fallbackContent={item.content}
        onSentencesExtracted={onSentencesExtracted}
        currentSentenceIndex={currentSentenceIndex}
        currentWordProgress={currentWordProgress}
        onSentenceClick={onSentenceClick}
        onChaptersExtracted={onChaptersExtracted}
      />
    );
  }

  if (item.type === "pdf" && item.file_path) {
    return (
      <PdfReader
        itemId={item.id}
        fallbackContent={item.content}
        onSentencesExtracted={onSentencesExtracted}
        currentSentenceIndex={currentSentenceIndex}
        currentWordProgress={currentWordProgress}
        onSentenceClick={onSentenceClick}
        onChaptersExtracted={onChaptersExtracted}
      />
    );
  }

  // HTML-formatted web imports get rich rendering with TTS integration
  if (item.type === "web" && item.content_format === "html") {
    return (
      <HtmlReader
        content={item.content}
        onSentencesExtracted={onSentencesExtracted}
        currentSentenceIndex={currentSentenceIndex}
        currentWordProgress={currentWordProgress}
        onSentenceClick={onSentenceClick}
      />
    );
  }

  // web / txt / fallback for epub/pdf without file_path
  return (
    <PlainTextReader
      content={item.content}
      onSentencesExtracted={onSentencesExtracted}
      currentSentenceIndex={currentSentenceIndex}
      currentWordProgress={currentWordProgress}
      onSentenceClick={onSentenceClick}
    />
  );
}
