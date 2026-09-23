import type { ReadioSdkClient } from '@readio/sdk'
import type {
  ExtensionLibraryRecord,
  ExtensionLibraryRepository,
} from './library-client'

export interface ExtensionStorageArea {
  get(key: string): Promise<Record<string, unknown>>
  set(values: Record<string, unknown>): Promise<void>
}

interface StoredContent {
  content: string
  sourceHtml: string
}

interface StoredExtensionLibrary {
  syncState?: string
  contents: Record<string, StoredContent>
}

const LIBRARY_STORAGE_KEY = 'readio.library.v2'

export class ChromeExtensionLibraryRepository
  implements ExtensionLibraryRepository
{
  constructor(
    private readonly storage: ExtensionStorageArea,
    private readonly sdk: Pick<ReadioSdkClient, 'newSyncState'>,
  ) {}

  async find(itemId: string): Promise<ExtensionLibraryRecord | null> {
    const stored = await this.load()
    const content = stored.contents[itemId]
    if (!stored.syncState || !content) return null

    const sync = await this.sdk.newSyncState(stored.syncState)
    try {
      const item = sync.libraryItem(itemId)
      if (
        !item ||
        item.type !== 'web' ||
        item.category !== 'imported' ||
        item.folder_id !== 'f1' ||
        !item.file_path ||
        item.content_format !== 'plaintext' ||
        !item.import_record
      ) {
        return null
      }
      return {
        ...item,
        content: content.content,
        type: 'web',
        category: 'imported',
        folder_id: 'f1',
        file_path: item.file_path,
        content_format: 'plaintext',
        import_record: item.import_record,
      }
    } finally {
      sync.free()
    }
  }

  async save(item: ExtensionLibraryRecord, sourceHtml: string): Promise<void> {
    const stored = await this.load()
    const sync = await this.sdk.newSyncState(stored.syncState)
    try {
      const { content, ...metadata } = item
      sync.stageLocal([
        {
          entityKind: 'library_item',
          entityId: item.id,
          payload: metadata,
        },
      ])
      stored.syncState = sync.serialize()
      stored.contents[item.id] = { content, sourceHtml }
      await this.storage.set({ [LIBRARY_STORAGE_KEY]: stored })
    } finally {
      sync.free()
    }
  }

  private async load(): Promise<StoredExtensionLibrary> {
    const stored = await this.storage.get(LIBRARY_STORAGE_KEY)
    const value = stored[LIBRARY_STORAGE_KEY]
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
      return { contents: {} }
    }
    const candidate = value as Partial<StoredExtensionLibrary>
    return {
      ...(typeof candidate.syncState === 'string'
        ? { syncState: candidate.syncState }
        : {}),
      contents:
        candidate.contents &&
        typeof candidate.contents === 'object' &&
        !Array.isArray(candidate.contents)
          ? candidate.contents
          : {},
    }
  }
}
