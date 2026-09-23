import { readFile } from 'node:fs/promises'
import { describe, expect, it } from 'vitest'

import {
  saveCapturedPage,
  type ExtensionLibraryRecord,
  type ExtensionLibraryRepository,
} from '../src/lib/library-client'
import { createRustCoreClient } from '@readio/sdk/rust-core'
import { createExtensionReadioSdkLoader } from '../src/lib/readio-runtime'


describe('extension library client', () => {
  it('uses the shared Rust parser and canonical identity before a local save', async () => {
    const repository = new MemoryExtensionLibraryRepository()
    const binary = await readFile(
      new URL('../../web/wasm/readio_wasm_bg.wasm', import.meta.url),
    )
    const core = createRustCoreClient(createExtensionReadioSdkLoader(binary))

    const saved = await saveCapturedPage(
      {
        selectedHtml:
          '<section><h1>Selected title</h1><p>Keep this passage.</p><script>steal()</script></section>',
        articleHtml: '<article><h1>Whole article</h1><p>Not selected.</p></article>',
        pageTitle: 'Browser title',
        pageUrl: 'https://example.com/articles/one',
      },
      repository,
      core,
    )

    expect(saved.title).toBe('Browser title')
    expect(saved.content).toBe('Selected title\nKeep this passage.')
    expect(saved.content).not.toContain('steal')
    expect(saved.content).not.toContain('Whole article')
    expect(saved.source).toBe('https://example.com/articles/one')
    expect(saved.import_record.item_id).toBe(saved.id)
    expect(repository.records.get(saved.id)?.sourceHtml).toContain('Selected title')
  })

  it('reuses canonical captured content instead of creating a duplicate', async () => {
    const repository = new MemoryExtensionLibraryRepository()
    const binary = await readFile(
      new URL('../../web/wasm/readio_wasm_bg.wasm', import.meta.url),
    )
    const core = createRustCoreClient(createExtensionReadioSdkLoader(binary))
    const first = await saveCapturedPage(
      {
        selectedHtml: '<p>Same passage.</p>',
        articleHtml: '',
        pageTitle: 'Original title',
        pageUrl: 'https://example.com/first',
      },
      repository,
      core,
    )
    const duplicate = await saveCapturedPage(
      {
        selectedHtml: '<main><p>Same passage.</p></main>',
        articleHtml: '',
        pageTitle: 'Replacement title',
        pageUrl: 'https://example.com/second',
      },
      repository,
      core,
    )

    expect(duplicate.id).toBe(first.id)
    expect(duplicate.title).toBe('Original title')
    expect(repository.records.size).toBe(1)
  })
})

class MemoryExtensionLibraryRepository implements ExtensionLibraryRepository {
  records = new Map<string, { item: ExtensionLibraryRecord; sourceHtml: string }>()

  async find(itemId: string): Promise<ExtensionLibraryRecord | null> {
    return this.records.get(itemId)?.item ?? null
  }

  async save(item: ExtensionLibraryRecord, sourceHtml: string): Promise<void> {
    this.records.set(item.id, { item, sourceHtml })
  }
}
