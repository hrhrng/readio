import { mkdirSync, rmSync, copyFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { build, context } from 'esbuild'

const isWatch = process.argv.includes('--watch')
const rootDir = dirname(dirname(fileURLToPath(import.meta.url)))
const distDir = resolve(rootDir, 'dist')

const entries = [
  { in: resolve(rootDir, 'src/popup.ts'), out: 'popup.js' },
  { in: resolve(rootDir, 'src/background.ts'), out: 'background.js' },
]

function copyStaticFiles() {
  copyFileSync(resolve(rootDir, 'manifest.json'), resolve(distDir, 'manifest.json'))
  copyFileSync(resolve(rootDir, 'src/popup.html'), resolve(distDir, 'popup.html'))
}

function ensureDist() {
  rmSync(distDir, { recursive: true, force: true })
  mkdirSync(distDir, { recursive: true })
}

async function runBuild() {
  ensureDist()
  copyStaticFiles()

  if (isWatch) {
    const contexts = await Promise.all(
      entries.map((entry) =>
        context({
          entryPoints: [entry.in],
          bundle: true,
          format: 'esm',
          sourcemap: true,
          outfile: resolve(distDir, entry.out),
          target: ['chrome120'],
          logLevel: 'info',
        })
      )
    )

    await Promise.all(contexts.map((ctx) => ctx.watch()))
    console.log('[readio-extension] watching for changes...')
    return
  }

  await Promise.all(
    entries.map((entry) =>
      build({
        entryPoints: [entry.in],
        bundle: true,
        format: 'esm',
        sourcemap: false,
        outfile: resolve(distDir, entry.out),
        target: ['chrome120'],
        logLevel: 'info',
      })
    )
  )
}

runBuild().catch((error) => {
  console.error(error)
  process.exit(1)
})
