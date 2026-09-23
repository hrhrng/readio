import { readFile } from 'node:fs/promises'
import { describe, expect, it } from 'vitest'

import { createReadioSdkClient } from '@readio/sdk'
import {
  ChromeExtensionLibraryRepository,
  type ExtensionStorageArea,
} from '../src/lib/extension-local-library'
import type { ExtensionLibraryRecord } from '../src/lib/library-client'
import { createExtensionReadioSdkLoader } from '../src/lib/readio-runtime'

describe('extension local-first library persistence', () => {
  it('round-trips captured metadata and source HTML without a server', async () => {
    const storage = new MemoryStorageArea()
    const sdk = await extensionSdk()
    const repository = new ChromeExtensionLibraryRepository(storage, sdk)
    const item = record('book-1', 'Offline capture')

    await repository.save(item, '<article><p>Offline body.</p></article>')

    expect(await repository.find('book-1')).toEqual(item)
    const persisted = storage.values['readio.library.v2'] as {
      syncState: string
      contents: Record<string, { content: string; sourceHtml: string }>
    }
    expect(persisted.contents).toEqual({
      'book-1': {
        content: 'Offline capture body',
        sourceHtml: '<article><p>Offline body.</p></article>',
      },
    })
    const sync = await sdk.newSyncState(persisted.syncState)
    expect(sync.libraryItem('book-1')?.title).toBe('Offline capture')
    expect(sync.outbox()).toHaveLength(1)
    sync.free()
  })

  it('preserves existing captures when another item is saved', async () => {
    const storage = new MemoryStorageArea()
    const repository = new ChromeExtensionLibraryRepository(
      storage,
      await extensionSdk(),
    )

    await repository.save(record('book-1', 'One'), '<p>One.</p>')
    await repository.save(record('book-2', 'Two'), '<p>Two.</p>')

    expect((await repository.find('book-1'))?.title).toBe('One')
    expect((await repository.find('book-2'))?.title).toBe('Two')
  })
})

async function extensionSdk() {
  const binary = await readFile(
    new URL('../../web/wasm/readio_wasm_bg.wasm', import.meta.url),
  )
  return createReadioSdkClient(createExtensionReadioSdkLoader(binary))
}

class MemoryStorageArea implements ExtensionStorageArea {
  values: Record<string, unknown> = {}

  async get(key: string): Promise<Record<string, unknown>> {
    return { [key]: structuredClone(this.values[key]) }
  }

  async set(values: Record<string, unknown>): Promise<void> {
    Object.assign(this.values, structuredClone(values))
  }
}

function record(id: string, title: string): ExtensionLibraryRecord {
  return {
    id,
    title,
    content: `${title} body`,
    type: 'web',
    progress: 0,
    category: 'imported',
    folder_id: 'f1',
    source: `https://example.com/${id}`,
    file_path: `extension-local:${id}`,
    cover_image: null,
    voice: null,
    speed: null,
    current_chapter: null,
    content_format: 'plaintext',
    import_record: {
      item_id: id,
      content_identity: {
        algorithm: 'blake3-prefix-v1',
        byte_length: 4,
        digest: id,
      },
      ownership: 'copy',
      source_key: `browser-extension:https://example.com/${id}`,
      storage_key: `extension-local:${id}`,
    },
    date: '2026-08-07T00:00:00.000Z',
  }
}
