import type {
  ImportRecord,
  RustCoreClient,
} from '@readio/sdk/rust-core'

export interface CapturedPage {
  selectedHtml: string
  articleHtml: string
  pageTitle: string
  pageUrl: string
}

export interface ExtensionLibraryRecord {
  id: string
  title: string
  content: string
  type: 'web'
  progress: number
  category: 'imported'
  folder_id: 'f1'
  source: string
  file_path: string
  cover_image: string | null
  voice: string | null
  speed: number | null
  current_chapter: string | null
  content_format: 'plaintext'
  import_record: ImportRecord
  date: string
}

export interface ExtensionLibraryRepository {
  find(itemId: string): Promise<ExtensionLibraryRecord | null>
  save(item: ExtensionLibraryRecord, sourceHtml: string): Promise<void>
}

type CaptureRustSdk = Pick<
  RustCoreClient,
  'contentIdentity' | 'parseHTML' | 'planImport'
>

const MAX_CAPTURE_HTML_BYTES = 4 * 1024 * 1024
const MAX_CAPTURE_BLOCKS = 100_000

export async function saveCapturedPage(
  capture: CapturedPage,
  repository: ExtensionLibraryRepository,
  core: CaptureRustSdk,
): Promise<ExtensionLibraryRecord> {
  const sourceHtml = capture.selectedHtml.trim() || capture.articleHtml.trim()
  if (!sourceHtml) {
    throw new Error('No readable page content was captured')
  }
  const sourceBytes = new TextEncoder().encode(sourceHtml)
  const document = await core.parseHTML(sourceHtml, {
    max_input_bytes: MAX_CAPTURE_HTML_BYTES,
    max_blocks: MAX_CAPTURE_BLOCKS,
    base_url: capture.pageUrl,
  })
  const content = document.blocks
    .map((block) => block.text.trim())
    .filter(Boolean)
    .join('\n')
  if (!content) {
    throw new Error('No readable page content was captured')
  }

  const identity = await core.contentIdentity(new TextEncoder().encode(content))
  const existingItem = await repository.find(identity.digest)
  const ownedStorageKey = `extension-local:${identity.digest}`
  const plan = await core.planImport({
    content_identity: identity,
    ownership: 'copy',
    source_key: `browser-extension:${capture.pageUrl}`,
    owned_storage_key: ownedStorageKey,
    existing: existingItem?.import_record ?? null,
  })
  if (plan.outcome === 'reused' && existingItem) {
    return existingItem
  }

  const item: ExtensionLibraryRecord = {
    id: plan.record.item_id,
    title: capture.pageTitle.trim() || 'Captured page',
    content,
    type: 'web',
    progress: 0,
    category: 'imported',
    folder_id: 'f1',
    source: capture.pageUrl,
    file_path: plan.record.storage_key,
    cover_image: null,
    voice: null,
    speed: null,
    current_chapter: null,
    content_format: 'plaintext',
    import_record: plan.record,
    date: new Date().toISOString(),
  }
  await repository.save(item, sourceHtml)
  return item
}
