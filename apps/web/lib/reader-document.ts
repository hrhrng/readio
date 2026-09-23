import type { DocumentTextIndex } from "./rust-core.ts"

export function sentenceIndexForBlock(
  index: DocumentTextIndex,
  blockId: string
): number | null {
  return index.sentences.find((sentence) => sentence.block_id === blockId)?.index ?? null
}

export function sentenceBlockCursor(
  index: DocumentTextIndex,
  sentenceIndex: number
): { blockId: string; offsetInBlock: number } | null {
  const sentence = index.sentences.find(
    (candidate) => candidate.index === sentenceIndex
  )
  return sentence
    ? { blockId: sentence.block_id, offsetInBlock: sentence.range[0] }
    : null
}
