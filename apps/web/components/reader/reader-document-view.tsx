"use client"

import type { ImageAssetResolver } from "./content-blocks"
import { BlockRenderer } from "./content-blocks"
import type { ProjectedEPUB } from "@/lib/epub-publication"

interface ReaderDocumentViewProps {
  projection: ProjectedEPUB
  currentSentenceIndex: number
  currentWordProgress: number
  onSentenceClick: (index: number) => void
  resolveImageAsset?: ImageAssetResolver
}

/**
 * The one Web projection surface for Reader IR. Format loaders may differ in
 * how they acquire a Reader IR or resolve assets, but semantic blocks, chapter
 * rhythm, highlighting, and interaction geometry must not branch by format.
 */
export function ReaderDocumentView({
  projection,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
  resolveImageAsset,
}: ReaderDocumentViewProps) {
  return (
    <article className="reader-document" data-reader-document>
      {projection.chapters.map((chapter) => (
        <section
          key={chapter.id}
          className="reader-chapter"
          data-reader-chapter={chapter.id}
        >
          {chapter.blocks.map((block, blockIndex) => (
            <BlockRenderer
              key={`${chapter.id}:${blockIndex}`}
              block={block}
              currentSentenceIndex={currentSentenceIndex}
              currentWordProgress={currentWordProgress}
              onSentenceClick={onSentenceClick}
              resolveImageAsset={resolveImageAsset}
            />
          ))}
        </section>
      ))}
    </article>
  )
}
