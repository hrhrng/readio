"use client"

import { useEffect, useMemo, useRef } from "react"
import { projectParsedPDF } from "@/lib/pdf-publication"
import type { ParsedPDF, ReaderIR } from "@/lib/rust-core"
import type { Sentence } from "@/lib/types"
import { ReaderDocumentView } from "./reader-document-view"
import type { TOCItem } from "./toc-panel"

interface SharedPdfReaderProps {
  content: string
  onSentencesExtracted: (sentences: Sentence[]) => void
  onReaderIRExtracted?: (reader: ReaderIR) => void
  currentSentenceIndex: number
  currentWordProgress: number
  onSentenceClick: (index: number) => void
  onChaptersExtracted?: (chapters: TOCItem[]) => void
}

export function SharedPdfReader({
  content,
  onSentencesExtracted,
  onReaderIRExtracted,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
  onChaptersExtracted,
}: SharedPdfReaderProps) {
  const emittedReaderRef = useRef<ReaderIR | null>(null)
  const result = useMemo(() => {
    try {
      return {
        publication: projectParsedPDF(JSON.parse(content) as ParsedPDF),
        error: null,
      }
    } catch (error) {
      return {
        publication: null,
        error:
          error instanceof Error
            ? error.message
            : "Invalid Readio PDF publication",
      }
    }
  }, [content])

  useEffect(() => {
    if (
      !result.publication
      || emittedReaderRef.current === result.publication.reader
    ) return
    emittedReaderRef.current = result.publication.reader
    onSentencesExtracted(result.publication.sentences)
    onReaderIRExtracted?.(result.publication.reader)
    onChaptersExtracted?.(result.publication.toc)
  }, [
    onChaptersExtracted,
    onReaderIRExtracted,
    onSentencesExtracted,
    result.publication,
  ])

  useEffect(() => {
    if (currentSentenceIndex < 0) return
    document
      .querySelector(`[data-sentence-id="${currentSentenceIndex}"]`)
      ?.scrollIntoView({ behavior: "smooth", block: "center" })
  }, [currentSentenceIndex])

  if (!result.publication) {
    return (
      <div className="text-center py-12 text-text-secondary" role="alert">
        <p>Failed to load PDF: {result.error}</p>
      </div>
    )
  }

  return (
    <ReaderDocumentView
      projection={result.publication}
      currentSentenceIndex={currentSentenceIndex}
      currentWordProgress={currentWordProgress}
      onSentenceClick={onSentenceClick}
    />
  )
}
