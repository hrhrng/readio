import { readFile } from 'node:fs/promises'
import { describe, expect, it } from 'vitest'

import { createExtensionReadioSdkLoader } from '../src/lib/readio-runtime'
import { createReadioSdkClient } from '@readio/sdk'

describe('extension Rust/WASM runtime', () => {
  it('loads the same SDK bundle and document semantics as Web', async () => {
    const binary = await readFile(
      new URL('../../web/wasm/readio_wasm_bg.wasm', import.meta.url),
    )
    const sdk = createReadioSdkClient(createExtensionReadioSdkLoader(binary))

    const document = await sdk.parseHTML(
      '<main><h1>Shared extension</h1><p>Portable body.</p><script>bad()</script></main>',
    )

    expect(document.blocks.map((block) => block.text)).toEqual([
      'Shared extension',
      'Portable body.',
    ])
  })
})
