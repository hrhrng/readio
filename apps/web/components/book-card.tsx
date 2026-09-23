"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { useRouter } from "next/navigation"
import { MoreHorizontal, Trash2 } from "lucide-react"
import { useSWRConfig } from "swr"
import { LibraryItemSummary, getReadingStatus } from "@/lib/types"
import { deleteItem, fetchItemFile } from "@/lib/api"
import { resolveLibraryForgetPresentation } from "@/lib/library-forget-presentation"
import { webReadioSdk } from "@/lib/readio-core-runtime"
import { ConfirmDialog } from "./confirm-dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "./dropdown-menu"
import { useToast } from "./toast-provider"
import { useLocale } from "./locale-provider"

const typeLabels: Record<string, string> = {
  web: "WEB",
  pdf: "PDF",
  epub: "EPUB",
  txt: "TXT",
}

function CoverPlaceholder({
  item,
  className,
}: {
  item: LibraryItemSummary
  className?: string
}) {
  return (
    <div className={`book-cover ${className ?? ""}`.trim()}>
      <div
        className="book-cover-placeholder"
        data-type={item.type}
        aria-hidden="true"
      >
        <span className="book-cover-type">{typeLabels[item.type]}</span>
        <span className="book-cover-title">{item.title}</span>
      </div>
    </div>
  )
}

export interface BookCoverAssetIO {
  loadArchive(itemId: string): Promise<Uint8Array>
  extractAsset(archive: Uint8Array, assetId: string): Promise<Uint8Array>
  objectURLs: Pick<typeof URL, "createObjectURL" | "revokeObjectURL">
}

const defaultCoverAssetIO: BookCoverAssetIO = {
  async loadArchive(itemId) {
    return new Uint8Array(await fetchItemFile(itemId))
  },
  extractAsset(archive, assetId) {
    return webReadioSdk.extractEPUBAsset(archive, assetId)
  },
  objectURLs: {
    createObjectURL: (blob) => URL.createObjectURL(blob),
    revokeObjectURL: (url) => URL.revokeObjectURL(url),
  },
}

export function BookCover({
  item,
  className,
  assetIO = defaultCoverAssetIO,
}: {
  item: LibraryItemSummary
  className?: string
  assetIO?: BookCoverAssetIO
}) {
  if (!item.cover_image) {
    return <CoverPlaceholder item={item} className={className} />
  }

  const assetId = localEpubAssetId(item.cover_image)
  if (assetId) {
    return (
      <LocalEpubCover
        item={item}
        assetId={assetId}
        className={className}
        assetIO={assetIO}
      />
    )
  }

  return (
    <div className={`book-cover ${className ?? ""}`.trim()}>
      {/* Covers come from the library API and can be data URLs or local file routes. */}
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img src={item.cover_image} alt="" />
    </div>
  )
}

function LocalEpubCover({
  item,
  assetId,
  className,
  assetIO,
}: {
  item: LibraryItemSummary
  assetId: string
  className?: string
  assetIO: BookCoverAssetIO
}) {
  const [source, setSource] = useState<string | null>(null)

  useEffect(() => {
    let active = true
    let ownedURL: string | null = null
    void assetIO
      .loadArchive(item.id)
      .then((archive) => assetIO.extractAsset(archive, assetId))
      .then((bytes) => {
        const ownedBytes = new Uint8Array(bytes.byteLength)
        ownedBytes.set(bytes)
        ownedURL = assetIO.objectURLs.createObjectURL(new Blob([ownedBytes]))
        if (active) setSource(ownedURL)
        else assetIO.objectURLs.revokeObjectURL(ownedURL)
      })
      .catch(() => {
        // The deterministic cover placeholder remains available offline when
        // an old or corrupt archive cannot produce its declared cover.
      })
    return () => {
      active = false
      if (ownedURL) assetIO.objectURLs.revokeObjectURL(ownedURL)
    }
  }, [assetIO, assetId, item.id])

  if (!source) return <CoverPlaceholder item={item} className={className} />
  return (
    <div className={`book-cover ${className ?? ""}`.trim()}>
      {/* Local EPUB covers are object URLs owned by this mounted card. */}
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img src={source} alt="" />
    </div>
  )
}

function localEpubAssetId(reference: string): string | null {
  const prefix = "readio-epub-asset:"
  if (!reference.startsWith(prefix)) return null
  const assetId = reference.slice(prefix.length)
  return assetId ? assetId : null
}

interface BookCardProps {
  item: LibraryItemSummary
  variant?: "grid" | "list"
}

export function BookCard({ item, variant = "grid" }: BookCardProps) {
  const { locale, t } = useLocale()
  const router = useRouter()
  const { mutate } = useSWRConfig()
  const { showToast } = useToast()
  const status = getReadingStatus(item.progress)
  const [menuOpen, setMenuOpen] = useState(false)
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const menuTriggerRef = useRef<HTMLButtonElement>(null)
  const forgetPresentation = resolveLibraryForgetPresentation(
    item.title,
    item.import_record?.ownership,
    locale
  )

  const openBook = () => {
    if (!menuOpen && !confirmOpen) router.push(`/player/${item.id}`)
  }

  const handleDelete = useCallback(async () => {
    setDeleting(true)
    try {
      await deleteItem(item.id)
      setConfirmOpen(false)
      mutate(
        (key: unknown) =>
          Array.isArray(key) &&
          (key[0] === "library-items" || key[0] === "search")
      )
      showToast("library.deleted", forgetPresentation.completion)
    } catch {
      showToast("library.delete_failed", t("library.deleteFailed", { title: item.title }))
    } finally {
      setDeleting(false)
    }
  }, [forgetPresentation.completion, item.id, item.title, mutate, showToast, t])

  const itemMenu = (triggerClassName: string) => (
    <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
      <DropdownMenuTrigger asChild>
        <button
          ref={menuTriggerRef}
          type="button"
          className={triggerClassName}
          aria-label={t("library.moreOptions", { title: item.title })}
        >
          <MoreHorizontal className="ui-icon" aria-hidden="true" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent className="book-menu" align="end">
        <DropdownMenuItem
          className="popover-item--danger"
          onSelect={() => setConfirmOpen(true)}
        >
          <Trash2 className="ui-icon-sm" aria-hidden="true" />
          {forgetPresentation.actionLabel}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )

  const confirmDialog = (
    <ConfirmDialog
      open={confirmOpen}
      title={forgetPresentation.title}
      description={forgetPresentation.description}
      confirmLabel={forgetPresentation.actionLabel}
      destructive
      loading={deleting}
      cancelLabel={t("common.cancel")}
      returnFocusRef={menuTriggerRef}
      onConfirm={handleDelete}
      onCancel={() => setConfirmOpen(false)}
    />
  )

  if (variant === "list") {
    return (
      <>
        <article className="book-row" role="listitem">
          <button type="button" onClick={openBook} className="book-row-main">
            <BookCover item={item} className="book-row-cover" />
            <span className="book-row-copy">
              <span className="book-title block">{item.title}</span>
              <span className="book-row-source block">
                {item.source || t("library.localFile")}
              </span>
              <span className="book-row-state" data-state={status}>
                <span>
                  {status === "new"
                    ? t("library.new")
                    : status === "finished"
                      ? t("library.finished")
                      : t("library.percentRead", { progress: item.progress })}
                </span>
                <span aria-hidden="true"> · </span>
                <span>{typeLabels[item.type]}</span>
              </span>
            </span>
          </button>
          <div className="book-row-actions">
            {itemMenu("ui-icon-button book-row-action")}
          </div>
        </article>
        {confirmDialog}
      </>
    )
  }

  return (
    <>
      <article className="book-tile" role="listitem">
        <button type="button" onClick={openBook} className="book-tile-main">
          <span className="book-cover-frame">
            <BookCover item={item} />
          </span>
          <span className="book-tile-copy">
            <span className="book-title block">{item.title}</span>
          </span>
        </button>

        <div className="book-tile-footer">
          <span className="book-meta">
            <span className="book-reading-state" data-state={status}>
              {status === "new"
                ? t("library.new")
                : status === "finished"
                  ? t("library.finished")
                  : `${item.progress}%`}
            </span>
            <span aria-hidden="true">·</span>
            <span>{typeLabels[item.type]}</span>
          </span>
          {itemMenu("ui-icon-button book-menu-trigger")}
        </div>
      </article>
      {confirmDialog}
    </>
  )
}
