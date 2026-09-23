import { getBrowserLocalReplicaStore } from "./browser-local-replica.ts"
import type { LocalReplicaStore } from "./local-replica-store.ts"
import type { Bookmark } from "./rust-core.ts"

async function resolveStore(
  store: LocalReplicaStore | undefined
): Promise<LocalReplicaStore> {
  return store ?? getBrowserLocalReplicaStore()
}

export async function fetchBookmarks(
  itemId: string,
  store?: LocalReplicaStore
): Promise<Bookmark[]> {
  return (await resolveStore(store)).bookmarks(itemId)
}

export async function saveBookmark(
  bookmark: Bookmark,
  store?: LocalReplicaStore
): Promise<Bookmark> {
  await (await resolveStore(store)).upsert([
    {
      entityKind: "bookmark",
      entityId: bookmark.id,
      payload: bookmark,
    },
  ])
  return bookmark
}

export async function deleteBookmark(
  bookmarkId: string,
  store?: LocalReplicaStore
): Promise<void> {
  await (await resolveStore(store)).delete([
    { entityKind: "bookmark", entityId: bookmarkId },
  ])
}
