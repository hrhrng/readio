"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { createPortal } from "react-dom"
import { FileText, Link, Loader2, Upload, X } from "lucide-react"
import { useSWRConfig } from "swr"
import { importFile, importUrl } from "@/lib/api"
import { InlineFeedback } from "./feedback"
import { useToast } from "./toast-provider"
import { useLocale } from "./locale-provider"

interface ImportDialogProps {
  open: boolean
  onClose: () => void
}

const ACCEPTED_EXTENSIONS = ".txt,.pdf,.epub,.docx"

export function ImportDialog({ open, onClose }: ImportDialogProps) {
  const { t } = useLocale()
  const { mutate } = useSWRConfig()
  const { showToast } = useToast()
  const [tab, setTab] = useState<"file" | "url">("file")
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [dragActive, setDragActive] = useState(false)
  const [url, setUrl] = useState("")
  const fileInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!open) return
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !loading) onClose()
    }
    document.addEventListener("keydown", handleKeyDown)
    return () => document.removeEventListener("keydown", handleKeyDown)
  }, [loading, onClose, open])

  const revalidate = useCallback(() => {
    mutate(
      (key: unknown) =>
        Array.isArray(key) &&
        (key[0] === "library-items" || key[0] === "search")
    )
  }, [mutate])

  const handleFileImport = useCallback(
    async (file: File) => {
      setLoading(true)
      setError(null)
      try {
        await importFile(file)
        revalidate()
        onClose()
        showToast("library.imported", t("import.fileAdded", { name: file.name }))
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : t("import.failed"))
      } finally {
        setLoading(false)
      }
    },
    [onClose, revalidate, showToast, t]
  )

  const handleUrlImport = useCallback(async () => {
    const trimmed = url.trim()
    if (!trimmed) return

    setLoading(true)
    setError(null)
    try {
      await importUrl(trimmed)
      revalidate()
      setUrl("")
      onClose()
      showToast("library.imported", t("import.urlAdded"))
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : t("import.failed"))
    } finally {
      setLoading(false)
    }
  }, [onClose, revalidate, showToast, t, url])

  const handleDrop = useCallback(
    (event: React.DragEvent) => {
      event.preventDefault()
      setDragActive(false)
      const file = event.dataTransfer.files[0]
      if (file) handleFileImport(file)
    },
    [handleFileImport]
  )

  if (!open) return null

  return createPortal(
    <div
      className="ui-overlay"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !loading) onClose()
      }}
    >
      <section
        className="ui-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-dialog-title"
      >
        <header className="ui-modal-header">
          <h2 id="import-dialog-title" className="ui-modal-title">
            {t("import.dialogTitle")}
          </h2>
          <button
            type="button"
            onClick={onClose}
            disabled={loading}
            className="ui-icon-button"
            aria-label={t("import.closeDialog")}
          >
            <X className="ui-icon" aria-hidden="true" />
          </button>
        </header>

        <div className="ui-modal-body form-stack">
          <div className="import-tabs" role="tablist" aria-label={t("import.method")}>
            <button
              type="button"
              role="tab"
              aria-selected={tab === "file"}
              onClick={() => {
                setTab("file")
                setError(null)
              }}
              className="import-tab"
            >
              <Upload className="ui-icon-sm" aria-hidden="true" />
              {t("import.fileTab")}
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={tab === "url"}
              onClick={() => {
                setTab("url")
                setError(null)
              }}
              className="import-tab"
            >
              <Link className="ui-icon-sm" aria-hidden="true" />
              {t("import.urlTab")}
            </button>
          </div>

          {tab === "file" ? (
            <div
              className="import-dropzone"
              data-active={dragActive}
              data-loading={loading}
              onDragOver={(event) => {
                event.preventDefault()
                setDragActive(true)
              }}
              onDragLeave={() => setDragActive(false)}
              onDrop={handleDrop}
              onClick={() => !loading && fileInputRef.current?.click()}
              role="button"
              tabIndex={loading ? -1 : 0}
              onKeyDown={(event) => {
                if ((event.key === "Enter" || event.key === " ") && !loading) {
                  fileInputRef.current?.click()
                }
              }}
            >
              {loading ? (
                <Loader2 className="ui-icon-lg animate-spin" aria-hidden="true" />
              ) : (
                <FileText className="ui-icon-lg" aria-hidden="true" />
              )}
              <strong>{loading ? t("import.inProgress") : t("import.dropFile")}</strong>
              <span>TXT, PDF, EPUB, or DOCX</span>
              <input
                ref={fileInputRef}
                type="file"
                className="sr-only"
                accept={ACCEPTED_EXTENSIONS}
                onChange={(event) => {
                  const file = event.target.files?.[0]
                  if (file) handleFileImport(file)
                  event.target.value = ""
                }}
              />
            </div>
          ) : (
            <div className="form-stack">
              <label className="form-field">
                <span className="form-label">{t("import.articleUrl")}</span>
                <input
                  type="url"
                  value={url}
                  onChange={(event) => setUrl(event.target.value)}
                  placeholder="https://example.com/article"
                  disabled={loading}
                  className="ui-input"
                  onKeyDown={(event) => {
                    if (event.key === "Enter") handleUrlImport()
                  }}
                />
              </label>
              <button
                type="button"
                onClick={handleUrlImport}
                disabled={loading || !url.trim()}
                className="ui-button ui-button--primary"
              >
                {loading && (
                  <Loader2 className="ui-icon-sm animate-spin" aria-hidden="true" />
                )}
                {loading ? t("import.inProgress") : t("import.importUrl")}
              </button>
            </div>
          )}

          {error && <InlineFeedback tone="danger">{error}</InlineFeedback>}
        </div>
      </section>
    </div>,
    document.body
  )
}
