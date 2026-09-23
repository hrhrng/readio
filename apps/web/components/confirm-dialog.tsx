"use client"

import { useEffect, useId, useRef, type RefObject } from "react"
import { createPortal } from "react-dom"

const FOCUSABLE_SELECTOR = [
  "button:not(:disabled)",
  "a[href]",
  "input:not(:disabled)",
  "textarea:not(:disabled)",
  '[tabindex]:not([tabindex="-1"])',
].join(",")

interface ConfirmDialogProps {
  open: boolean
  title: string
  description?: string
  confirmLabel?: string
  cancelLabel?: string
  destructive?: boolean
  loading?: boolean
  returnFocusRef?: RefObject<HTMLElement | null>
  onConfirm: () => void
  onCancel: () => void
}

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  destructive = false,
  loading = false,
  returnFocusRef,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const titleId = useId()
  const descriptionId = useId()
  const cancelRef = useRef<HTMLButtonElement>(null)
  const dialogRef = useRef<HTMLElement>(null)
  const capturedFocusRef = useRef<HTMLElement | null>(null)
  const loadingRef = useRef(loading)
  const onCancelRef = useRef(onCancel)

  useEffect(() => {
    loadingRef.current = loading
    onCancelRef.current = onCancel
  }, [loading, onCancel])

  useEffect(() => {
    if (!open) return

    const explicitReturnTarget = returnFocusRef?.current ?? null
    capturedFocusRef.current =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !loadingRef.current) {
        event.preventDefault()
        onCancelRef.current()
        return
      }
      if (event.key !== "Tab" || !dialogRef.current) return

      const focusable = Array.from(
        dialogRef.current.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)
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

    document.addEventListener("keydown", handleKeyDown)
    cancelRef.current?.focus()
    return () => {
      document.removeEventListener("keydown", handleKeyDown)
      const returnTarget = explicitReturnTarget ?? capturedFocusRef.current
      if (returnTarget?.isConnected) {
        returnTarget.focus()
        return
      }
      document
        .querySelector<HTMLElement>(
          '[role="dialog"][aria-modal="true"] button:not(:disabled)'
        )
        ?.focus()
    }
  }, [open, returnFocusRef])

  if (!open) return null

  return createPortal(
    <div
      className="ui-overlay"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !loading) onCancel()
      }}
    >
      <section
        ref={dialogRef}
        className="ui-modal ui-modal--sm"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={description ? descriptionId : undefined}
      >
        <div className="ui-modal-body">
          <h2 id={titleId} className="ui-modal-title">
            {title}
          </h2>
          {description && (
            <p id={descriptionId} className="ui-state-description mt-2">
              {description}
            </p>
          )}
          <div className="ui-modal-actions">
            <button
              ref={cancelRef}
              type="button"
              onClick={onCancel}
              disabled={loading}
              className="ui-button ui-button--quiet"
            >
              {cancelLabel}
            </button>
            <button
              type="button"
              onClick={onConfirm}
              disabled={loading}
              className={`ui-button ${
                destructive ? "ui-button--danger" : "ui-button--primary"
              }`}
            >
              {loading ? "Working…" : confirmLabel}
            </button>
          </div>
        </div>
      </section>
    </div>,
    document.body
  )
}
