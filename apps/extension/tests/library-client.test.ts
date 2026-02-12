import { describe, expect, it } from 'vitest'

import { buildCreateLibraryItemUrl, buildLibraryItemPayload } from '../src/lib/library-client'


describe('extension library client', () => {
  it('builds library create endpoint url', () => {
    expect(buildCreateLibraryItemUrl('http://localhost:8000/')).toBe('http://localhost:8000/api/library/items')
  })

  it('builds a web library item payload from captured content', () => {
    const payload = buildLibraryItemPayload({
      title: 'Captured article',
      content: 'Hello from extension',
      sourceUrl: 'https://example.com/article',
    })

    expect(payload).toEqual({
      title: 'Captured article',
      content: 'Hello from extension',
      type: 'web',
      category: 'imported',
      folder_id: 'f1',
      source: 'https://example.com/article',
    })
  })
})
