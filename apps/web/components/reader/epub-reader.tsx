"use client"

/**
 * Compatibility adapter for EPUB records created before `readio-epub-v1`.
 * The archive is parsed by Rust; EPUB.js is not a second source of document
 * order, navigation, HTML semantics, or asset identity.
 */

import { useEffect, useState } from "react"
import { fetchItemFile } from "@/lib/api"
import { webReadioSdk } from "@/lib/readio-core-runtime"
import type { ParsedEPUB, ReaderIR } from "@/lib/rust-core"
import type { Sentence } from "@/lib/types"
import { SharedDocumentReader } from "./shared-document-reader"
import { SharedEpubReader } from "./shared-epub-reader"
import type { TOCItem } from "./toc-panel"

export interface EpubCompatibilityIO {
  parseItem(itemId: string): Promise<ParsedEPUB>
}

interface EpubReaderProps {
  itemId: string
  fallbackContent?: string
  onSentencesExtracted: (sentences: Sentence[]) => void
  onReaderIRExtracted?: (reader: ReaderIR) => void
  currentSentenceIndex: number
  currentWordProgress: number
  onSentenceClick: (index: number) => void
  onChaptersExtracted?: (chapters: TOCItem[]) => void
  compatibilityIO?: EpubCompatibilityIO
}

const defaultCompatibilityIO: EpubCompatibilityIO = {
  async parseItem(itemId) {
    const bytes = new Uint8Array(await fetchItemFile(itemId))
    return webReadioSdk.parseEPUB(bytes)
  },
}

export function EpubReader({
  itemId,
  fallbackContent,
  onSentencesExtracted,
  onReaderIRExtracted,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
  onChaptersExtracted,
  compatibilityIO = defaultCompatibilityIO,
}: EpubReaderProps) {
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
          reason instanceof Error ? reason.message : "Failed to load EPUB"
        )
      }
    )
    return () => {
      active = false
    }
  }, [compatibilityIO, itemId])

  if (publication) {
    return (
      <SharedEpubReader
        itemId={itemId}
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
        <p>Failed to load EPUB: {error}</p>
      </div>
    )
  }

  return (
    <div className="reader-loading-copy" aria-label="Loading EPUB">
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
