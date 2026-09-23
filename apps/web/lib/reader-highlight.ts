import type {
  DocumentIR,
  DocumentTextIndex,
  HighlightTarget,
  RustCoreClient,
} from "./rust-core.ts"

/** Browser adapter: normalize a media fraction, then let Rust choose the word. */
export async function resolveReaderHighlight(
  core: RustCoreClient,
  document: DocumentIR,
  index: DocumentTextIndex,
  sentenceIndex: number,
  wordProgress: number
): Promise<HighlightTarget | null> {
  const revision = document.revision?.revision_digest
  if (!revision || !index.sentences[sentenceIndex]) return null
  const progress = Number.isFinite(wordProgress)
    ? Math.min(1, Math.max(0, wordProgress))
    : 0
  return core.resolveHighlight(document, index, {
    document_revision: revision,
    sentence_index: sentenceIndex,
    word_index: null,
    word_progress_basis_points: Math.round(progress * 10_000),
  })
}
