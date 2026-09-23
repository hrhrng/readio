import { readFile } from 'node:fs/promises'
import { describe, expect, it } from 'vitest'

describe('extension manifest', () => {
  it('allows only the WebAssembly capability required by the local Rust SDK', async () => {
    const manifest = JSON.parse(
      await readFile(new URL('../manifest.json', import.meta.url), 'utf8'),
    ) as {
      content_security_policy?: { extension_pages?: string }
    }

    expect(manifest.content_security_policy?.extension_pages).toBe(
      "script-src 'self' 'wasm-unsafe-eval'; object-src 'self'",
    )
  })
})
