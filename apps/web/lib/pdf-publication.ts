import { projectReaderIR, type ProjectedEPUB } from "./epub-publication"
import type { ParsedPDF } from "./rust-core"

export type ProjectedPDF = ProjectedEPUB

/**
 * Projects the shared PDF publication through the same semantic Document IR
 * renderer as EPUB. Page spans become stable chapter IDs; no glyph/layout
 * heuristics are allowed on the React side.
 */
export function projectParsedPDF(publication: ParsedPDF): ProjectedPDF {
  if (!isPublication(publication)) {
    throw new Error("Invalid Readio PDF publication")
  }
  return projectReaderIR(publication.reader)
}

function isPublication(value: unknown): value is ParsedPDF {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false
  const publication = value as Partial<ParsedPDF>
  return (
    publication.reader !== null
    && typeof publication.reader === "object"
    && !Array.isArray(publication.reader)
  )
}
