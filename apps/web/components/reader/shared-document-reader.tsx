"use client"

import { useEffect, useState } from "react"
import { projectReaderIR, type ProjectedEPUB } from "@/lib/epub-publication"
import { webReadioSdk } from "@/lib/readio-core-runtime"
import { webDiagnostics, type WebDiagnosticLog } from "@/lib/diagnostics"
import type { ReaderIR } from "@/lib/rust-core"
import type { Sentence } from "@/lib/types"
import { ReaderDocumentView } from "./reader-document-view"
import type { TOCItem } from "./toc-panel"

export interface DocumentReaderIO {
  parseHTML(content: string): Promise<ReaderIR>
  parseMarkdown(content: string): Promise<ReaderIR>
  parsePlainText(content: string): Promise<ReaderIR>
}

interface SharedDocumentReaderProps {
  content: string
  format: "html" | "markdown" | "text"
  onSentencesExtracted: (sentences: Sentence[]) => void
  onReaderIRExtracted?: (reader: ReaderIR) => void
  currentSentenceIndex: number
  currentWordProgress: number
  onSentenceClick: (index: number) => void
  onChaptersExtracted?: (chapters: TOCItem[]) => void
  readerIO?: DocumentReaderIO
  diagnostics?: Pick<WebDiagnosticLog, "beginOperation" | "record">
}

const defaultReaderIO: DocumentReaderIO = {
  parseHTML(content) {
    return webReadioSdk.parseHTMLReader(content)
  },
  parseMarkdown(content) {
    return webReadioSdk.parseMarkdownReader(content)
  },
  parsePlainText(content) {
    return webReadioSdk.parsePlainTextReader(content)
  },
}

export function SharedDocumentReader({
  content,
  format,
  onSentencesExtracted,
  onReaderIRExtracted,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
  onChaptersExtracted,
  readerIO = defaultReaderIO,
  diagnostics = webDiagnostics,
}: SharedDocumentReaderProps) {
  const [publication, setPublication] = useState<ProjectedEPUB | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let active = true
    setPublication(null)
    setError(null)
    const operationId = diagnostics.beginOperation()
    const documentFormat = format === "text" ? "plain_text" : format
    const byteCount = new TextEncoder().encode(content).byteLength
    const parse =
      format === "html"
        ? readerIO.parseHTML
        : format === "markdown"
          ? readerIO.parseMarkdown
          : readerIO.parsePlainText
    void parse.call(readerIO, content).then(
      (reader) => {
        diagnostics.record({
          operationId,
          component: "web-reader-parser",
          category: "parsing",
          name: "parse_completed",
          fields: {
            document_format: documentFormat,
            byte_count: byteCount,
            block_count: reader.document.blocks.length,
            chapter_count: reader.chapters.length,
            outcome: "completed",
          },
        })
        if (!active) return
        const projected = projectReaderIR(reader)
        setPublication(projected)
        onSentencesExtracted(projected.sentences)
        onReaderIRExtracted?.(reader)
        onChaptersExtracted?.(projected.toc)
      },
      (reason: unknown) => {
        diagnostics.record({
          operationId,
          component: "web-reader-parser",
          category: "parsing",
          name: "parse_failed",
          fields: {
            document_format: documentFormat,
            byte_count: byteCount,
            outcome: "failed",
            reason_code: "parse_failed",
          },
        })
        if (!active) {
          return
        }
        setError(
          reason instanceof Error ? reason.message : "Could not parse document"
        )
      }
    )
    return () => {
      active = false
    }
  }, [
    content,
    diagnostics,
    format,
    onChaptersExtracted,
    onReaderIRExtracted,
    onSentencesExtracted,
    readerIO,
  ])

  useEffect(() => {
    if (currentSentenceIndex < 0) return
    document
      .querySelector(`[data-sentence-id="${currentSentenceIndex}"]`)
      ?.scrollIntoView({ behavior: "smooth", block: "center" })
  }, [currentSentenceIndex])

  if (error) {
    return (
      <div className="text-center py-12 text-text-secondary" role="alert">
        <p>Failed to load document: {error}</p>
      </div>
    )
  }

  if (!publication) {
    return (
      <div className="reader-loading-copy" aria-label="Loading document">
        {Array.from({ length: 8 }, (_, index) => (
          <div
            key={index}
            className={`skeleton skeleton-line ${
              index % 4 === 3 ? "skeleton-line--short" : ""
            }`}
          />
        ))}
      </div>
    )
  }

  return (
    <ReaderDocumentView
      projection={publication}
      currentSentenceIndex={currentSentenceIndex}
      currentWordProgress={currentWordProgress}
      onSentenceClick={onSentenceClick}
    />
  )
}
