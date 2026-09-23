import {
  type ContentBlock,
  type RichSentence,
  type StyledRun,
} from "../components/reader/content-blocks"
import type {
  DocumentBlock,
  DocumentInlineSpan,
  DocumentTextIndex,
  ParsedEPUB,
  ReaderIR,
  SentenceRange,
} from "./rust-core"
import type { Sentence } from "./types"

export interface ProjectedEPUBChapter {
  id: string
  title: string
  blocks: ContentBlock[]
}

export interface ProjectedEPUB {
  reader: ReaderIR
  chapters: ProjectedEPUBChapter[]
  sentences: Sentence[]
  readerDocument: ReaderIR["document"]
  textIndex: DocumentTextIndex
  toc: {
    id: string
    title: string
    depth: number
    sentenceIndex: number | null
  }[]
}

export function projectParsedEPUB(publication: ParsedEPUB): ProjectedEPUB {
  if (!isPublication(publication)) {
    throw new Error("Invalid Readio EPUB publication")
  }
  return projectReaderIR(publication.reader, publication.assets ?? [])
}

export function projectReaderIR(
  reader: ReaderIR,
  assets: NonNullable<ParsedEPUB["assets"]> = []
): ProjectedEPUB {
  if (!isReaderIR(reader)) {
    throw new Error("Invalid Readio Reader IR")
  }
  const sentences: Sentence[] = reader.text_index.sentences.map((sentence) => ({
    index: sentence.index,
    text: sentence.text,
  }))
  const publicationAssets = new Map(
    assets.map((asset) => [asset.id, asset])
  )
  const sentencesByBlock = new Map<string, SentenceRange[]>()
  for (const sentence of reader.text_index.sentences) {
    const current = sentencesByBlock.get(sentence.block_id) ?? []
    current.push(sentence)
    sentencesByBlock.set(sentence.block_id, current)
  }
  const chapters = reader.chapters.map((chapter) => ({
    id: chapter.id,
    title: chapter.title,
    blocks: reader.document.blocks
      .slice(chapter.block_range[0], chapter.block_range[1])
      .flatMap<ContentBlock>((block) =>
        projectBlock(
          block,
          sentencesByBlock.get(block.id) ?? [],
          reader,
          publicationAssets
        )
      ),
  }))
  const chapterIds = new Set(reader.chapters.map((chapter) => chapter.id))
  const toc = reader.navigation.flatMap((entry) => {
    if (!chapterIds.has(entry.chapter_id)) return []
    return [{
      id: entry.block_id === null
        ? entry.chapter_id
        : `${entry.chapter_id}#${entry.block_id}`,
      title: entry.title,
      depth: entry.depth,
      sentenceIndex: entry.sentence_index,
    }]
  })

  return {
    reader,
    chapters,
    sentences,
    readerDocument: reader.document,
    textIndex: reader.text_index,
    toc:
      toc.length > 0
        ? toc
        : chapters.map((chapter) => ({
            id: chapter.id,
            title: chapter.title,
            depth: 0,
            sentenceIndex:
              reader.chapters.find((candidate) => candidate.id === chapter.id)
                ?.sentence_range[0] ?? null,
          })),
  }
}

function projectBlock(
  block: DocumentBlock,
  sentences: SentenceRange[],
  reader: ReaderIR,
  publicationAssets: Map<string, NonNullable<ParsedEPUB["assets"]>[number]>
): ContentBlock[] {
  const richSentences = (): RichSentence[] =>
    sentences.map((sentence) => ({
      index: sentence.index,
      text: sentence.text,
      runs: projectSentenceRuns(block, sentence),
    }))

  switch (block.kind.type) {
    case "heading": {
      const projected = richSentences()
      return projected.length === 0
        ? []
        : [{
            type: "heading",
            level: block.kind.level,
            sentences: projected,
            anchorId: block.id,
          }]
    }
    case "paragraph": {
      const projected = richSentences()
      return projected.length === 0
        ? []
        : [{ type: "paragraph", sentences: projected, anchorId: block.id }]
    }
    case "quote": {
      const projected = richSentences()
      return projected.length === 0
        ? []
        : [{ type: "blockquote", sentences: projected, anchorId: block.id }]
    }
    case "code":
      return block.text.trim() ? [{ type: "code", text: block.text }] : []
    case "image": {
      const assetId = block.kind.asset_id
      const documentAsset = reader.document.assets.find(
        (asset) => asset.id === assetId
      )
      const publicationAsset = publicationAssets.get(assetId)
      if (!documentAsset) return []
      if (!publicationAsset) {
        return [{
          type: "image",
          src: documentAsset.source,
          assetId: documentAsset.id,
          alt: documentAsset.alt || block.text || undefined,
        }]
      }
      return [{
        type: "image",
        src: null,
        assetId: publicationAsset.id,
        mediaType: publicationAsset.media_type,
        alt: documentAsset.alt || block.text || undefined,
      }]
    }
  }
}

function projectSentenceRuns(
  block: DocumentBlock,
  sentence: SentenceRange
): StyledRun[] {
  const [sentenceStart, sentenceEnd] = sentence.range
  if (
    !Number.isInteger(sentenceStart) ||
    !Number.isInteger(sentenceEnd) ||
    sentenceStart < 0 ||
    sentenceEnd <= sentenceStart
  ) {
    return sentence.text ? [{ text: sentence.text }] : []
  }

  const runs = projectInlineRuns(block.text, block.inline_spans)
  const projected: StyledRun[] = []
  let offset = 0
  for (const run of runs) {
    const scalars = Array.from(run.text)
    const runStart = offset
    const runEnd = runStart + scalars.length
    offset = runEnd
    const start = Math.max(sentenceStart, runStart)
    const end = Math.min(sentenceEnd, runEnd)
    if (start >= end) continue
    projected.push({
      text: scalars.slice(start - runStart, end - runStart).join(""),
      ...(run.bold ? { bold: true } : {}),
      ...(run.italic ? { italic: true } : {}),
      ...(run.href ? { href: run.href } : {}),
    })
  }
  return projected.length > 0 ? projected : sentence.text ? [{ text: sentence.text }] : []
}

function projectInlineRuns(
  text: string,
  spans: DocumentInlineSpan[] | undefined
): StyledRun[] {
  if (!spans || spans.length === 0) return text ? [{ text }] : []

  const scalars = Array.from(text)
  const runs: StyledRun[] = []
  let cursor = 0
  for (const span of spans) {
    const [start, end] = span.range
    if (
      !Number.isInteger(start) ||
      !Number.isInteger(end) ||
      start < cursor ||
      end <= start ||
      end > scalars.length
    ) {
      return text ? [{ text }] : []
    }
    if (start > cursor) {
      runs.push({ text: scalars.slice(cursor, start).join("") })
    }
    runs.push({
      text: scalars.slice(start, end).join(""),
      ...(span.bold ? { bold: true } : {}),
      ...(span.italic ? { italic: true } : {}),
    })
    cursor = end
  }
  if (cursor < scalars.length) {
    runs.push({ text: scalars.slice(cursor).join("") })
  }
  return runs
}

function isPublication(value: unknown): value is ParsedEPUB {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false
  const publication = value as Partial<ParsedEPUB>
  return isReaderIR(publication.reader)
}

function isReaderIR(value: unknown): value is ReaderIR {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false
  const reader = value as Partial<ReaderIR>
  return (
    Number.isInteger(reader.ir_version) &&
    reader.document !== null &&
    typeof reader.document === "object" &&
    Array.isArray(reader.document.blocks) &&
    Array.isArray(reader.document.assets) &&
    reader.text_index !== null &&
    typeof reader.text_index === "object" &&
    Array.isArray(reader.text_index.sentences) &&
    Array.isArray(reader.chapters) &&
    Array.isArray(reader.navigation)
  )
}
