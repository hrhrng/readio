"use client"

import { useRouter } from "next/navigation"
import { ArrowLeft, Bookmark, List, Search, Settings } from "lucide-react"
import type { ReaderTool } from "@/lib/reader-tools-navigation"
import { useLocale } from "../locale-provider"

interface TopBarProps {
  title: string
  onToggleToc: () => void
  onToggleSettings: () => void
  onOpenTool: (tool: ReaderTool) => void
  activeTool: ReaderTool | null
  tocOpen: boolean
}

interface ReaderToolActionsProps {
  activeTool: ReaderTool | null
  tocOpen: boolean
  onOpenTool: (tool: ReaderTool) => void
  onToggleToc: () => void
  onToggleSettings: () => void
}

export function ReaderToolActions({
  activeTool,
  tocOpen,
  onOpenTool,
  onToggleToc,
  onToggleSettings,
}: ReaderToolActionsProps) {
  const { t } = useLocale()
  return (
    <div className="reader-topbar-end">
      <button
        type="button"
        onClick={() => onOpenTool("search")}
        className="ui-icon-button"
        aria-pressed={activeTool === "search"}
        aria-label={t("reader.find")}
      >
        <Search className="ui-icon" aria-hidden="true" />
      </button>
      <button
        type="button"
        onClick={() => onOpenTool("bookmarks")}
        className="ui-icon-button"
        aria-pressed={activeTool === "bookmarks"}
        aria-label={t("reader.openBookmarks")}
      >
        <Bookmark className="ui-icon" aria-hidden="true" />
      </button>
      <button
        type="button"
        onClick={onToggleToc}
        className="ui-icon-button"
        aria-pressed={tocOpen}
        aria-label={t("reader.openToc")}
      >
        <List className="ui-icon" aria-hidden="true" />
      </button>
      <button
        type="button"
        onClick={onToggleSettings}
        className="ui-icon-button"
        aria-label={t("reader.openSettings")}
      >
        <Settings className="ui-icon" aria-hidden="true" />
      </button>
    </div>
  )
}

export function TopBar({
  title,
  onToggleToc,
  onToggleSettings,
  onOpenTool,
  activeTool,
  tocOpen,
}: TopBarProps) {
  const router = useRouter()
  const { t } = useLocale()

  return (
    <header className="reader-topbar">
      <div className="reader-topbar-start">
        <button
          type="button"
          onClick={() => router.back()}
          className="ui-button ui-button--quiet px-2"
          aria-label={t("reader.back")}
        >
          <ArrowLeft className="ui-icon" aria-hidden="true" />
          <span className="hidden sm:inline">{t("reader.back")}</span>
        </button>
      </div>

      <h1 className="reader-topbar-title">{title}</h1>

      <ReaderToolActions
        activeTool={activeTool}
        tocOpen={tocOpen}
        onOpenTool={onOpenTool}
        onToggleToc={onToggleToc}
        onToggleSettings={onToggleSettings}
      />
    </header>
  )
}
