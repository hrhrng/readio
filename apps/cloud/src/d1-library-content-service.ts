import type {
  LibraryContentDownload,
  LibraryContentRecord,
  LibraryContentService,
  LibraryContentUploadInput,
  LibraryContentVariant,
} from "./app"

interface ContentRow {
  item_id: string
  variant: LibraryContentVariant
  object_key: string
  sha256_base64: string
  etag: string
  byte_length: number
  content_type: string
}

function hex(value: ArrayBuffer): string {
  return [...new Uint8Array(value)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("")
}

async function privateObjectKey(
  userId: string,
  itemId: string,
  variant: LibraryContentVariant,
  digest: ArrayBuffer,
): Promise<string> {
  const encoder = new TextEncoder()
  const [userNamespace, itemNamespace] = await Promise.all([
    crypto.subtle.digest("SHA-256", encoder.encode(userId)),
    crypto.subtle.digest("SHA-256", encoder.encode(itemId)),
  ])
  return `library-content/${hex(userNamespace)}/${hex(itemNamespace)}/${variant}/${hex(digest)}`
}

function record(row: ContentRow): LibraryContentRecord {
  return {
    itemId: row.item_id,
    variant: row.variant,
    byteLength: row.byte_length,
    contentType: row.content_type,
    sha256Base64: row.sha256_base64,
    etag: row.etag,
  }
}

export class D1LibraryContentService implements LibraryContentService {
  constructor(
    private readonly database: D1Database,
    private readonly media: R2Bucket,
    private readonly now: () => number = Date.now,
  ) {}

  async put(userId: string, input: LibraryContentUploadInput): Promise<LibraryContentRecord> {
    const objectKey = await privateObjectKey(
      userId,
      input.itemId,
      input.variant,
      input.sha256,
    )
    const existing = await this.media.head(objectKey)
    const object =
      existing ??
      (await this.media.put(objectKey, input.body, {
        httpMetadata: { contentType: input.contentType },
        customMetadata: {
          variant: input.variant,
          sha256: input.sha256Base64,
        },
        sha256: input.sha256,
      }))
    if (existing) {
      await input.body.cancel()
    }
    if (object.size !== input.byteLength) {
      throw new Error("library_content_length_mismatch")
    }

    await this.database
      .withSession("first-primary")
      .prepare(
        `INSERT INTO library_content
           (user_id, item_id, variant, object_key, sha256_base64, etag,
            byte_length, content_type, updated_at_unix_ms)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id, item_id, variant) DO UPDATE SET
           object_key = excluded.object_key,
           sha256_base64 = excluded.sha256_base64,
           etag = excluded.etag,
           byte_length = excluded.byte_length,
           content_type = excluded.content_type,
           updated_at_unix_ms = excluded.updated_at_unix_ms`,
      )
      .bind(
        userId,
        input.itemId,
        input.variant,
        objectKey,
        input.sha256Base64,
        object.httpEtag,
        object.size,
        input.contentType,
        this.now(),
      )
      .run()

    return {
      itemId: input.itemId,
      variant: input.variant,
      byteLength: object.size,
      contentType: input.contentType,
      sha256Base64: input.sha256Base64,
      etag: object.httpEtag,
    }
  }

  async get(
    userId: string,
    itemId: string,
    variant: LibraryContentVariant,
  ): Promise<LibraryContentDownload | null> {
    const row = await this.database
      .withSession("first-primary")
      .prepare(
        `SELECT item_id, variant, object_key, sha256_base64, etag,
                byte_length, content_type
           FROM library_content
          WHERE user_id = ? AND item_id = ? AND variant = ?`,
      )
      .bind(userId, itemId, variant)
      .first<ContentRow>()
    if (!row) return null

    const object = await this.media.get(row.object_key)
    if (!object || object.size !== row.byte_length) return null
    return { ...record(row), etag: object.httpEtag, body: object.body }
  }

  async head(
    userId: string,
    itemId: string,
    variant: LibraryContentVariant,
  ): Promise<LibraryContentRecord | null> {
    const row = await this.database
      .withSession("first-primary")
      .prepare(
        `SELECT item_id, variant, object_key, sha256_base64, etag,
                byte_length, content_type
           FROM library_content
          WHERE user_id = ? AND item_id = ? AND variant = ?`,
      )
      .bind(userId, itemId, variant)
      .first<ContentRow>()
    if (!row) return null

    const object = await this.media.head(row.object_key)
    if (!object || object.size !== row.byte_length) return null
    return { ...record(row), etag: object.httpEtag }
  }
}
