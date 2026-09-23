import { env } from "cloudflare:test"
import { beforeEach, describe, expect, it } from "vitest"

import { D1LibraryContentService } from "../src/d1-library-content-service"

async function insertUser(id: string) {
  await env.DB.prepare(
    `INSERT INTO user (id, name, email, email_verified, created_at, updated_at)
     VALUES (?, ?, ?, 1, 0, 0)`,
  )
    .bind(id, id, `${id}@example.com`)
    .run()
}

function base64(value: ArrayBuffer): string {
  return btoa(String.fromCharCode(...new Uint8Array(value)))
}

async function uploadInput(itemId: string, bytes: Uint8Array<ArrayBuffer>) {
  const digest = await crypto.subtle.digest("SHA-256", bytes.buffer)
  return {
    itemId,
    variant: "reader" as const,
    byteLength: bytes.byteLength,
    contentType: "text/plain; charset=utf-8",
    sha256: digest,
    sha256Base64: base64(digest),
    body: new Response(bytes.buffer).body!,
  }
}

describe("D1 and R2 library content adapter", () => {
  beforeEach(async () => {
    await env.DB.exec("DELETE FROM library_content; DELETE FROM user;")
    const listed = await env.MEDIA.list({ prefix: "library-content/" })
    if (listed.objects.length > 0) {
      await env.MEDIA.delete(listed.objects.map((object) => object.key))
    }
    await insertUser("reader-a")
    await insertUser("reader-b")
  })

  it("stores a content-addressed private object and isolates downloads by user", async () => {
    const service = new D1LibraryContentService(env.DB, env.MEDIA, () => 1_000)
    const bytes = new TextEncoder().encode("Private reader content")
    const stored = await service.put("reader-a", await uploadInput("book-a", bytes))

    expect(stored).toMatchObject({
      itemId: "book-a",
      variant: "reader",
      byteLength: bytes.byteLength,
      contentType: "text/plain; charset=utf-8",
    })
    const downloaded = await service.get("reader-a", "book-a", "reader")
    expect(downloaded).toMatchObject(stored)
    expect(
      new Uint8Array(await new Response(downloaded!.body).arrayBuffer()),
    ).toEqual(bytes)
    await expect(service.get("reader-b", "book-a", "reader")).resolves.toBeNull()

    const row = await env.DB.prepare(
      `SELECT object_key, sha256_base64
         FROM library_content
        WHERE user_id = ? AND item_id = ? AND variant = ?`,
    )
      .bind("reader-a", "book-a", "reader")
      .first<{ object_key: string; sha256_base64: string }>()
    expect(row?.object_key).toContain("library-content/")
    expect(row?.object_key).not.toContain("reader-a")
    expect(row?.object_key).not.toContain("book-a")
    expect(row?.sha256_base64).toBe(stored.sha256Base64)
  })

  it("rejects a lying byte length without publishing metadata", async () => {
    const service = new D1LibraryContentService(env.DB, env.MEDIA, () => 2_000)
    const bytes = new TextEncoder().encode("longer than declared")
    const input = await uploadInput("book-length", bytes)

    await expect(
      service.put("reader-a", { ...input, byteLength: bytes.byteLength - 1 }),
    ).rejects.toThrow("library_content_length_mismatch")
    expect(
      await env.DB.prepare(
        "SELECT COUNT(*) AS count FROM library_content WHERE user_id = ? AND item_id = ?",
      )
        .bind("reader-a", "book-length")
        .first<number>("count"),
    ).toBe(0)
  })
})
