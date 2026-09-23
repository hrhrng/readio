import type { ParsedEPUBAsset } from "./rust-core.ts"

export interface EpubAssetResolver {
  resolve(assetId: string): Promise<string>
  dispose(): void
}

export interface ObjectURLStore {
  createObjectURL(blob: Blob): string
  revokeObjectURL(url: string): void
}

export function createEpubAssetResolver(options: {
  assets: ParsedEPUBAsset[]
  loadArchive: () => Promise<Uint8Array>
  extractAsset: (
    archive: Uint8Array,
    assetId: string
  ) => Promise<Uint8Array>
  objectURLs?: ObjectURLStore
}): EpubAssetResolver {
  const objectURLs = options.objectURLs ?? URL
  const assets = new Map(options.assets.map((asset) => [asset.id, asset]))
  const pendingURLs = new Map<string, Promise<string>>()
  const ownedURLs = new Set<string>()
  let archive: Promise<Uint8Array> | undefined
  let disposed = false

  return {
    resolve(assetId) {
      if (disposed) {
        return Promise.reject(new Error("EPUB asset resolver is disposed"))
      }
      const metadata = assets.get(assetId)
      if (!metadata) {
        return Promise.reject(new Error(`Unknown EPUB asset: ${assetId}`))
      }
      const cached = pendingURLs.get(assetId)
      if (cached) return cached

      archive ??= options.loadArchive()
      const pending = archive
        .then((bytes) => options.extractAsset(bytes, assetId))
        .then((bytes) => {
          const ownedBytes = new Uint8Array(bytes.byteLength)
          ownedBytes.set(bytes)
          const url = objectURLs.createObjectURL(
            new Blob([ownedBytes], { type: metadata.media_type })
          )
          if (disposed) {
            objectURLs.revokeObjectURL(url)
            throw new Error("EPUB asset resolver is disposed")
          }
          ownedURLs.add(url)
          return url
        })
        .catch((error: unknown) => {
          pendingURLs.delete(assetId)
          throw error
        })
      pendingURLs.set(assetId, pending)
      return pending
    },
    dispose() {
      if (disposed) return
      disposed = true
      for (const url of ownedURLs) objectURLs.revokeObjectURL(url)
      ownedURLs.clear()
      pendingURLs.clear()
    },
  }
}
