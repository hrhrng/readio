"use client"

import { FormEvent, useCallback, useEffect, useRef, useState } from "react"
import {
  ArrowDown,
  ArrowUp,
  BookmarkPlus,
  Check,
  LoaderCircle,
  Pencil,
  Search,
  Trash2,
  X,
} from "lucide-react"
import { InlineFeedback } from "@/components/feedback"
import { ConfirmDialog } from "@/components/confirm-dialog"
import { useToast } from "@/components/toast-provider"
import { useLocale } from "@/components/locale-provider"
import {
  deleteBookmark,
  fetchBookmarks,
  saveBookmark,
} from "@/lib/bookmark-api"
import {
  sentenceBlockCursor,
  sentenceIndexForBlock,
} from "@/lib/reader-document"
import { webReadioSdk } from "@/lib/readio-core-runtime"
import {
  resolveSearchWalk,
  type ReaderTool,
} from "@/lib/reader-tools-navigation"
import type {
  Bookmark,
  DocumentIR,
  DocumentTextIndex,
  SearchHit,
  SearchResultPage,
} from "@/lib/rust-core"

interface ReaderToolsInspectorProps {
  activeTool: ReaderTool
  onToolChange: (tool: ReaderTool) => void
  itemId: string
  document: DocumentIR | null
  textIndex: DocumentTextIndex | null
  documentError: string | null
  currentSentenceIndex: number
  onJumpToSentence: (index: number) => void
}

const SEARCH_PAGE_SIZE = 100

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback
}

function splitSnippet(hit: SearchHit) {
  const scalars = Array.from(hit.snippet)
  const [start, end] = hit.snippet_match_range
  return {
    before: scalars.slice(0, start).join(""),
    match: scalars.slice(start, end).join(""),
    after: scalars.slice(end).join(""),
  }
}

function mutationId(prefix: string): string {
  return `${prefix}-${globalThis.crypto.randomUUID()}`
}

export function ReaderToolsInspector({
  activeTool,
  onToolChange,
  itemId,
  document,
  textIndex,
  documentError,
  currentSentenceIndex,
  onJumpToSentence,
}: ReaderToolsInspectorProps) {
  const { t } = useLocale()
  return (
    <div className="reader-tools">
      <div className="reader-tools-tabs" role="tablist" aria-label={t("reader.tools")}>
        <button
          type="button"
          role="tab"
          aria-selected={activeTool === "search"}
          onClick={() => onToolChange("search")}
        >
          {t("reader.searchTab")}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeTool === "bookmarks"}
          onClick={() => onToolChange("bookmarks")}
        >
          {t("reader.bookmarksTab")}
        </button>
      </div>

      {documentError ? (
        <InlineFeedback tone="danger">{documentError}</InlineFeedback>
      ) : activeTool === "search" ? (
        <ReaderSearch
          document={document}
          textIndex={textIndex}
          onJumpToSentence={onJumpToSentence}
        />
      ) : (
        <ReaderBookmarks
          itemId={itemId}
          document={document}
          textIndex={textIndex}
          currentSentenceIndex={currentSentenceIndex}
          onJumpToSentence={onJumpToSentence}
        />
      )}
    </div>
  )
}

function ReaderSearch({
  document,
  textIndex,
  onJumpToSentence,
}: {
  document: DocumentIR | null
  textIndex: DocumentTextIndex | null
  onJumpToSentence: (index: number) => void
}) {
  const { t } = useLocale()
  const [query, setQuery] = useState("")
  const [page, setPage] = useState<SearchResultPage | null>(null)
  const [hits, setHits] = useState<SearchHit[]>([])
  const [activeHit, setActiveHit] = useState<number | null>(null)
  const [isSearching, setIsSearching] = useState(false)
  const [searchError, setSearchError] = useState<string | null>(null)
  const operation = useRef(0)

  const jumpToHit = useCallback(
    (hit: SearchHit, index: number) => {
      if (!textIndex) return
      const sentenceIndex = sentenceIndexForBlock(textIndex, hit.block_id)
      if (sentenceIndex === null) return
      setActiveHit(index)
      onJumpToSentence(sentenceIndex)
    },
    [onJumpToSentence, textIndex]
  )

  const search = useCallback(
    async (cursor: string | null, append: boolean) => {
      const trimmed = query.trim()
      if (!document?.revision || !trimmed) {
        setPage(null)
        setHits([])
        setActiveHit(null)
        return []
      }

      const operationId = ++operation.current
      setIsSearching(true)
      setSearchError(null)
      try {
        const nextPage = await webReadioSdk.searchDocument(document, {
          document_revision: document.revision.revision_digest,
          query: trimmed,
          cursor,
          limit: SEARCH_PAGE_SIZE,
        })
        if (operation.current !== operationId) return []
        const merged = append ? [...hits, ...nextPage.hits] : nextPage.hits
        setPage(nextPage)
        setHits(merged)
        if (!append) setActiveHit(null)
        return merged
      } catch (error) {
        if (operation.current === operationId) {
          setSearchError(errorMessage(error, t("reader.somethingWentWrong")))
        }
        return []
      } finally {
        if (operation.current === operationId) setIsSearching(false)
      }
    },
    [document, hits, query, t]
  )

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault()
    void search(null, false)
  }

  const walk = async (direction: -1 | 1) => {
    if (hits.length === 0) return
    const resolution = resolveSearchWalk({
      hitCount: hits.length,
      activeIndex: activeHit,
      direction,
      nextCursor: page?.next_cursor ?? null,
    })
    const availableHits =
      resolution.kind === "load"
        ? await search(resolution.cursor, true)
        : hits
    const hit = availableHits[resolution.targetIndex]
    if (hit) jumpToHit(hit, resolution.targetIndex)
  }

  return (
    <section className="reader-tool-panel" role="tabpanel">
      <form className="reader-tool-search" onSubmit={handleSubmit}>
        <div className="reader-tool-search-field">
          <Search className="ui-icon" aria-hidden="true" />
          <input
            className="ui-input"
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("reader.find")}
            aria-label={t("reader.find")}
            autoFocus
          />
        </div>
        <button
          type="submit"
          className="ui-button ui-button--primary"
          disabled={!document || !query.trim() || isSearching}
        >
          {isSearching ? (
            <LoaderCircle className="ui-icon reader-tool-spinner" />
          ) : (
            t("reader.find")
          )}
        </button>
      </form>

      {!document ? (
        <p className="reader-tool-empty">{t("reader.indexing")}</p>
      ) : searchError ? (
        <InlineFeedback tone="danger">{searchError}</InlineFeedback>
      ) : page && page.total_occurrences === 0 ? (
        <p className="reader-tool-empty">{t("reader.noMatches")}</p>
      ) : page ? (
        <>
          <div className="reader-tool-summary">
            <span>
              {t("reader.searchSummary", {
                matches: page.total_occurrences,
                passages: page.total_blocks,
              })}
            </span>
            <div className="reader-tool-walk" aria-label={t("reader.walkResults")}>
              <button
                type="button"
                className="ui-icon-button"
                onClick={() => void walk(-1)}
                aria-label={t("reader.previousMatch")}
              >
                <ArrowUp className="ui-icon" aria-hidden="true" />
              </button>
              <button
                type="button"
                className="ui-icon-button"
                onClick={() => void walk(1)}
                aria-label={t("reader.nextMatch")}
              >
                <ArrowDown className="ui-icon" aria-hidden="true" />
              </button>
            </div>
          </div>
          <ol className="reader-tool-results">
            {hits.map((hit, index) => {
              const snippet = splitSnippet(hit)
              return (
                <li key={hit.hit_id}>
                  <button
                    type="button"
                    className="reader-tool-result"
                    data-active={activeHit === index}
                    onClick={() => jumpToHit(hit, index)}
                  >
                    <span className="reader-tool-result-index">{index + 1}</span>
                    <span>
                      {snippet.before}
                      <mark>{snippet.match}</mark>
                      {snippet.after}
                    </span>
                  </button>
                </li>
              )
            })}
          </ol>
          {page.next_cursor ? (
            <button
              type="button"
              className="ui-button ui-button--secondary reader-tool-load-more"
              onClick={() => void search(page.next_cursor, true)}
              disabled={isSearching}
            >
              {t("reader.loadMore")}
            </button>
          ) : null}
        </>
      ) : (
        <p className="reader-tool-empty">
          {t("reader.searchHelp")}
        </p>
      )}
    </section>
  )
}

function ReaderBookmarks({
  itemId,
  document,
  textIndex,
  currentSentenceIndex,
  onJumpToSentence,
}: {
  itemId: string
  document: DocumentIR | null
  textIndex: DocumentTextIndex | null
  currentSentenceIndex: number
  onJumpToSentence: (index: number) => void
}) {
  const { showToast } = useToast()
  const { t } = useLocale()
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([])
  const [isLoading, setIsLoading] = useState(true)
  const [busyId, setBusyId] = useState<string | null>(null)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [draftNote, setDraftNote] = useState("")
  const [bookmarkError, setBookmarkError] = useState<string | null>(null)
  const [pendingDelete, setPendingDelete] = useState<Bookmark | null>(null)

  const refresh = useCallback(async () => {
    setIsLoading(true)
    try {
      setBookmarks(await fetchBookmarks(itemId))
      setBookmarkError(null)
    } catch (error) {
      setBookmarkError(errorMessage(error, t("reader.somethingWentWrong")))
    } finally {
      setIsLoading(false)
    }
  }, [itemId, t])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const recoverMutation = async (error: unknown) => {
    await refresh()
    setBookmarkError(errorMessage(error, t("reader.somethingWentWrong")))
  }

  const markCurrentPosition = async () => {
    if (!document || !textIndex) return
    const cursor = sentenceBlockCursor(textIndex, currentSentenceIndex)
    if (!cursor) return
    const now = Date.now()
    setBusyId("new")
    setBookmarkError(null)
    try {
      const projection = await webReadioSdk.resolveBlockOffset(
        document,
        cursor.blockId,
        cursor.offsetInBlock,
        "forward"
      )
      const existing = bookmarks.find(
        (bookmark) => bookmark.char_offset === projection.char_offset
      )
      if (existing) {
        setEditingId(existing.id)
        setDraftNote(existing.note)
        return
      }

      const transition = await webReadioSdk.upsertBookmark(document, bookmarks, {
        bookmark_id: mutationId("bookmark"),
        item_id: itemId,
        char_offset: projection.char_offset,
        affinity: "forward",
        note: "",
        created_at_unix_ms: now,
        updated_at_unix_ms: now,
        base_version: 0,
      })
      const saved = await saveBookmark(transition.bookmark)
      setBookmarks((current) => [...current, saved])
      showToast("bookmark.saved", t("bookmark.added"))
    } catch (error) {
      await recoverMutation(error)
    } finally {
      setBusyId(null)
    }
  }

  const saveRename = async (bookmark: Bookmark) => {
    if (!document) return
    const now = Date.now()
    setBusyId(bookmark.id)
    setBookmarkError(null)
    try {
      const transition = await webReadioSdk.upsertBookmark(document, bookmarks, {
        bookmark_id: bookmark.id,
        item_id: itemId,
        char_offset: bookmark.char_offset,
        affinity: bookmark.affinity,
        note: draftNote,
        created_at_unix_ms: bookmark.created_at_unix_ms,
        updated_at_unix_ms: now,
        base_version: bookmark.version,
      })
      const saved = await saveBookmark(transition.bookmark)
      setBookmarks((current) =>
        current.map((value) => (value.id === saved.id ? saved : value))
      )
      setEditingId(null)
      showToast("bookmark.saved", t("bookmark.renamed"))
    } catch (error) {
      await recoverMutation(error)
    } finally {
      setBusyId(null)
    }
  }

  const remove = async (bookmark: Bookmark) => {
    setBusyId(bookmark.id)
    setBookmarkError(null)
    try {
      await deleteBookmark(bookmark.id)
      setBookmarks((current) =>
        current.filter((value) => value.id !== bookmark.id)
      )
      showToast("bookmark.deleted", t("bookmark.removed"))
    } catch (error) {
      await recoverMutation(error)
    } finally {
      setBusyId(null)
      setPendingDelete(null)
    }
  }

  const jump = async (bookmark: Bookmark) => {
    if (!document) return
    try {
      const projection = await webReadioSdk.projectOffset(
        document,
        bookmark.char_offset,
        bookmark.affinity
      )
      if (!projection.block_id) return
      if (!textIndex) return
      const sentenceIndex = sentenceIndexForBlock(textIndex, projection.block_id)
      if (sentenceIndex !== null) onJumpToSentence(sentenceIndex)
    } catch (error) {
      setBookmarkError(errorMessage(error, t("reader.somethingWentWrong")))
    }
  }

  return (
    <>
      <section className="reader-tool-panel" role="tabpanel">
      <button
        type="button"
        className="ui-button ui-button--primary reader-tool-mark"
        onClick={() => void markCurrentPosition()}
        disabled={!document || !textIndex || busyId !== null}
      >
        {busyId === "new" ? (
          <LoaderCircle className="ui-icon reader-tool-spinner" aria-hidden="true" />
        ) : (
          <BookmarkPlus className="ui-icon" aria-hidden="true" />
        )}
        {t("bookmark.markPassage")}
      </button>

      {bookmarkError ? (
        <InlineFeedback tone="danger">{bookmarkError}</InlineFeedback>
      ) : null}

      {isLoading ? (
        <p className="reader-tool-empty">{t("bookmark.loading")}</p>
      ) : bookmarks.length === 0 ? (
        <p className="reader-tool-empty">
          {t("bookmark.empty")}
        </p>
      ) : (
        <ol className="reader-bookmark-list">
          {bookmarks.map((bookmark, index) => (
            <li key={bookmark.id} className="reader-bookmark-row">
              {editingId === bookmark.id ? (
                <form
                  className="reader-bookmark-edit"
                  onSubmit={(event) => {
                    event.preventDefault()
                    void saveRename(bookmark)
                  }}
                >
                  <input
                    className="ui-input"
                    value={draftNote}
                    onChange={(event) => setDraftNote(event.target.value)}
                    aria-label={t("bookmark.renameNumber", { number: index + 1 })}
                    autoFocus
                  />
                  <button
                    type="submit"
                    className="ui-icon-button"
                    aria-label={t("bookmark.saveName")}
                    disabled={busyId === bookmark.id}
                  >
                    <Check className="ui-icon" aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className="ui-icon-button"
                    aria-label={t("bookmark.cancelRename")}
                    onClick={() => setEditingId(null)}
                  >
                    <X className="ui-icon" aria-hidden="true" />
                  </button>
                </form>
              ) : (
                <>
                  <button
                    type="button"
                    className="reader-bookmark-jump"
                    onClick={() => void jump(bookmark)}
                  >
                    <span className="reader-bookmark-index">{index + 1}</span>
                    <span>
                      <strong>{bookmark.note || t("bookmark.untitled")}</strong>
                      <small>{t("bookmark.character", { offset: bookmark.char_offset })}</small>
                    </span>
                  </button>
                  <div className="reader-bookmark-actions">
                    <button
                      type="button"
                      className="ui-icon-button"
                      aria-label={t("bookmark.renameNumber", { number: index + 1 })}
                      onClick={() => {
                        setEditingId(bookmark.id)
                        setDraftNote(bookmark.note)
                      }}
                    >
                      <Pencil className="ui-icon-sm" aria-hidden="true" />
                    </button>
                    <button
                      type="button"
                      className="ui-icon-button"
                      aria-label={t("bookmark.deleteNumber", { number: index + 1 })}
                      onClick={() => setPendingDelete(bookmark)}
                      disabled={busyId === bookmark.id}
                    >
                      <Trash2 className="ui-icon-sm" aria-hidden="true" />
                    </button>
                  </div>
                </>
              )}
            </li>
          ))}
        </ol>
      )}
      </section>
      <ConfirmDialog
        open={pendingDelete !== null}
        title={t("bookmark.deleteTitle")}
        description={t("bookmark.deleteDescription")}
        confirmLabel={t("bookmark.deleteAction")}
        cancelLabel={t("common.cancel")}
        destructive
        loading={pendingDelete !== null && busyId === pendingDelete.id}
        onConfirm={() => {
          if (pendingDelete) void remove(pendingDelete)
        }}
        onCancel={() => setPendingDelete(null)}
      />
    </>
  )
}
