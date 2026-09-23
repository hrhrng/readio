"use client";

import { LibraryItem, Sentence } from "@/lib/types";
import { EpubReader } from "./epub-reader";
import { PdfReader } from "./pdf-reader";
import { SharedEpubReader } from "./shared-epub-reader";
import { SharedPdfReader } from "./shared-pdf-reader";
import {
  SharedDocumentReader,
  type DocumentReaderIO,
} from "./shared-document-reader";
import type { TOCItem } from "./toc-panel";
import type { ReaderIR } from "@/lib/rust-core";
import type { WebDiagnosticLog } from "@/lib/diagnostics";

interface ContentRouterProps {
  item: LibraryItem;
  onSentencesExtracted: (sentences: Sentence[]) => void;
  onReaderIRExtracted?: (reader: ReaderIR) => void;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
  onChaptersExtracted?: (chapters: TOCItem[]) => void;
  documentReaderIO?: DocumentReaderIO;
  diagnostics?: Pick<WebDiagnosticLog, "beginOperation" | "record">;
}

export function ContentRouter({
  item,
  onSentencesExtracted,
  onReaderIRExtracted,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
  onChaptersExtracted,
  documentReaderIO,
  diagnostics,
}: ContentRouterProps) {
  if (!item.content && !item.file_path) {
    return (
      <div className="text-center py-12 text-text-secondary">
        <p className="text-lg">No readable content</p>
      </div>
    );
  }

  if (item.type === "epub") {
    if (item.content_format === "readio-epub-v1" && item.content) {
      return (
        <SharedEpubReader
          itemId={item.id}
          content={item.content}
          onSentencesExtracted={onSentencesExtracted}
          onReaderIRExtracted={onReaderIRExtracted}
          currentSentenceIndex={currentSentenceIndex}
          currentWordProgress={currentWordProgress}
          onSentenceClick={onSentenceClick}
          onChaptersExtracted={onChaptersExtracted}
        />
      );
    }
    if (item.file_path) {
      return (
        <EpubReader
          itemId={item.id}
          fallbackContent={item.content}
          onSentencesExtracted={onSentencesExtracted}
          onReaderIRExtracted={onReaderIRExtracted}
          currentSentenceIndex={currentSentenceIndex}
          currentWordProgress={currentWordProgress}
          onSentenceClick={onSentenceClick}
          onChaptersExtracted={onChaptersExtracted}
        />
      );
    }
  }

  if (item.type === "pdf") {
    if (item.content_format === "readio-pdf-v1" && item.content) {
      return (
        <SharedPdfReader
          content={item.content}
          onSentencesExtracted={onSentencesExtracted}
          onReaderIRExtracted={onReaderIRExtracted}
          currentSentenceIndex={currentSentenceIndex}
          currentWordProgress={currentWordProgress}
          onSentenceClick={onSentenceClick}
          onChaptersExtracted={onChaptersExtracted}
        />
      );
    }
    if (item.file_path) {
      return (
        <PdfReader
          itemId={item.id}
          fallbackContent={item.content}
          onSentencesExtracted={onSentencesExtracted}
          onReaderIRExtracted={onReaderIRExtracted}
          currentSentenceIndex={currentSentenceIndex}
          currentWordProgress={currentWordProgress}
          onSentenceClick={onSentenceClick}
          onChaptersExtracted={onChaptersExtracted}
        />
      );
    }
  }

  const markdownSource =
    item.markdown_source ??
    (item.content_format === "markdown" ? item.content : null);
  if (markdownSource !== null) {
    return (
      <SharedDocumentReader
        content={markdownSource}
        format="markdown"
        onSentencesExtracted={onSentencesExtracted}
        onReaderIRExtracted={onReaderIRExtracted}
        currentSentenceIndex={currentSentenceIndex}
        currentWordProgress={currentWordProgress}
        onSentenceClick={onSentenceClick}
        onChaptersExtracted={onChaptersExtracted}
        readerIO={documentReaderIO}
        diagnostics={diagnostics}
      />
    );
  }

  // HTML-formatted web imports get rich rendering with TTS integration
  if (item.type === "web" && item.content_format === "html") {
    return (
      <SharedDocumentReader
        content={item.content}
        format="html"
        onSentencesExtracted={onSentencesExtracted}
        onReaderIRExtracted={onReaderIRExtracted}
        currentSentenceIndex={currentSentenceIndex}
        currentWordProgress={currentWordProgress}
        onSentenceClick={onSentenceClick}
        onChaptersExtracted={onChaptersExtracted}
        readerIO={documentReaderIO}
        diagnostics={diagnostics}
      />
    );
  }

  // web / txt / fallback for epub/pdf without file_path
  return (
    <SharedDocumentReader
      content={item.content}
      format="text"
      onSentencesExtracted={onSentencesExtracted}
      onReaderIRExtracted={onReaderIRExtracted}
      currentSentenceIndex={currentSentenceIndex}
      currentWordProgress={currentWordProgress}
      onSentenceClick={onSentenceClick}
      onChaptersExtracted={onChaptersExtracted}
      readerIO={documentReaderIO}
      diagnostics={diagnostics}
    />
  );
}
