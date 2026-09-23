"use client"

/**
 * Compatibility adapter for PDF records created before `readio-pdf-v1`.
 *
 * The original bytes are parsed by the same Rust Core used at import time.
 * React never infers lines, columns, headers, paragraphs, or chapters.
 */

import { useEffect, useState } from "react"
import { fetchItemFile } from "@/lib/api"
import { webReadioSdk } from "@/lib/readio-core-runtime"
import type { ParsedPDF, ReaderIR } from "@/lib/rust-core"
import type { Sentence } from "@/lib/types"
import { SharedDocumentReader } from "./shared-document-reader"
import { SharedPdfReader } from "./shared-pdf-reader"
import type { TOCItem } from "./toc-panel"

export interface PdfCompatibilityIO {
  parseItem(itemId: string): Promise<ParsedPDF>
}

interface PdfReaderProps {
  itemId: string
  fallbackContent?: string
  onSentencesExtracted: (sentences: Sentence[]) => void
  onReaderIRExtracted?: (reader: ReaderIR) => void
  currentSentenceIndex: number
  currentWordProgress: number
  onSentenceClick: (index: number) => void
  onChaptersExtracted?: (chapters: TOCItem[]) => void
  compatibilityIO?: PdfCompatibilityIO
}

const defaultCompatibilityIO: PdfCompatibilityIO = {
  async parseItem(itemId) {
    const bytes = new Uint8Array(await fetchItemFile(itemId))
    return webReadioSdk.parsePDF(bytes)
  },
}

export function PdfReader({
  itemId,
  fallbackContent,
  onSentencesExtracted,
  onReaderIRExtracted,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
  onChaptersExtracted,
  compatibilityIO = defaultCompatibilityIO,
}: PdfReaderProps) {
  const [publication, setPublication] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let active = true
    setPublication(null)
    setError(null)
    compatibilityIO.parseItem(itemId).then(
      (parsed) => {
        if (active) setPublication(JSON.stringify(parsed))
      },
      (reason) => {
        if (!active) return
        setError(
          reason instanceof Error ? reason.message : "Failed to load PDF"
        )
      }
    )
    return () => {
      active = false
    }
  }, [compatibilityIO, itemId])

  if (publication) {
    return (
      <SharedPdfReader
        content={publication}
        onSentencesExtracted={onSentencesExtracted}
        onReaderIRExtracted={onReaderIRExtracted}
        currentSentenceIndex={currentSentenceIndex}
        currentWordProgress={currentWordProgress}
        onSentenceClick={onSentenceClick}
        onChaptersExtracted={onChaptersExtracted}
      />
    )
  }

  if (error) {
    if (fallbackContent) {
      return (
        <SharedDocumentReader
          content={fallbackContent}
          format="text"
          onSentencesExtracted={onSentencesExtracted}
          onReaderIRExtracted={onReaderIRExtracted}
          currentSentenceIndex={currentSentenceIndex}
          currentWordProgress={currentWordProgress}
          onSentenceClick={onSentenceClick}
          onChaptersExtracted={onChaptersExtracted}
        />
      )
    }
    return (
      <div className="text-center py-12 text-text-secondary" role="alert">
        <p>Failed to load PDF: {error}</p>
      </div>
    )
  }

  return (
    <div className="reader-loading-copy" aria-label="Loading PDF">
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
