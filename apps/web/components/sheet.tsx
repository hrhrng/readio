"use client"

import { useEffect, useId, useRef } from "react"
import { createPortal } from "react-dom"
import { X } from "lucide-react"
import { useLocale } from "./locale-provider"

const FOCUSABLE_SELECTOR = [
  "button:not(:disabled)",
  "a[href]",
  "input:not(:disabled)",
  "select:not(:disabled)",
  "textarea:not(:disabled)",
  '[tabindex]:not([tabindex="-1"])',
].join(",")

interface SheetProps {
  open: boolean
  title: string
  onClose: () => void
  children: React.ReactNode
}

export function Sheet({ open, title, onClose, children }: SheetProps) {
  const { t } = useLocale()
  const titleId = useId()
  const closeRef = useRef<HTMLButtonElement>(null)
  const panelRef = useRef<HTMLElement>(null)
  const returnFocusRef = useRef<HTMLElement | null>(null)
  const onCloseRef = useRef(onClose)

  useEffect(() => {
    onCloseRef.current = onClose
  }, [onClose])

  useEffect(() => {
    if (!open) return

    const previousOverflow = document.body.style.overflow
    returnFocusRef.current =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null
    const handleKeyDown = (event: KeyboardEvent) => {
      const blockingDialog = document.querySelector<HTMLElement>(
        '[role="alertdialog"][aria-modal="true"]'
      )
      if (blockingDialog && !panelRef.current?.contains(blockingDialog)) return

      if (event.key === "Escape") {
        onCloseRef.current()
        return
      }
      if (event.key !== "Tab" || !panelRef.current) return

      const focusable = Array.from(
        panelRef.current.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }

    document.body.style.overflow = "hidden"
    document.addEventListener("keydown", handleKeyDown)
    closeRef.current?.focus()

    return () => {
      document.body.style.overflow = previousOverflow
      document.removeEventListener("keydown", handleKeyDown)
      returnFocusRef.current?.focus()
    }
  }, [open])

  if (!open) return null

  return createPortal(
    <div
      className="ui-overlay ui-sheet-overlay"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <aside
        ref={panelRef}
        className="ui-sheet"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <header className="ui-sheet-header">
          <h2 id={titleId} className="ui-sheet-title">
            {title}
          </h2>
          <button
            ref={closeRef}
            type="button"
            className="ui-icon-button"
            onClick={onClose}
            aria-label={t("reader.closePanel")}
          >
            <X className="ui-icon" aria-hidden="true" />
          </button>
        </header>
        <div className="ui-sheet-body">{children}</div>
      </aside>
    </div>,
    document.body
  )
}
