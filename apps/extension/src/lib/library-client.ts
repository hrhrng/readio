export type CreateLibraryItemPayload = {
  title: string
  content: string
  type: 'web'
  category: 'imported'
  folder_id: 'f1'
  source: string
}

export function buildCreateLibraryItemUrl(apiBase: string): string {
  const normalizedBase = apiBase.replace(/\/$/, '')
  return `${normalizedBase}/api/library/items`
}

export function buildLibraryItemPayload(input: {
  title: string
  content: string
  sourceUrl: string
}): CreateLibraryItemPayload {
  return {
    title: input.title,
    content: input.content,
    type: 'web',
    category: 'imported',
    folder_id: 'f1',
    source: input.sourceUrl,
  }
}
