"use client"

import { ChevronLeft, ChevronRight } from "lucide-react"
import { useLocale } from "../locale-provider"

export interface TOCItem {
  id: string
  title: string
  depth?: number
  sentenceIndex?: number | null
}

interface TOCPanelProps {
  items: TOCItem[]
  currentSentenceIndex: number
  onNavigate: (sentenceIndex: number) => void
  onItemClick?: (index: number) => void
  activeIndex?: number
}

export function TOCPanel({
  items,
  currentSentenceIndex,
  onNavigate,
  onItemClick,
  activeIndex,
}: TOCPanelProps) {
  const { t } = useLocale()
  if (items.length === 0) {
    return <p className="reader-toc-empty">{t("reader.noToc")}</p>
  }

  const projectedActiveIndex = activeIndex ?? items.reduce(
    (selected, item, index) =>
      item.sentenceIndex != null && item.sentenceIndex <= currentSentenceIndex
        ? index
        : selected,
    -1
  )
  const chapterTargets = items.filter(
    (item) => (item.depth ?? 0) === 0 && item.sentenceIndex != null
  )
  const activeChapterIndex = chapterTargets.reduce(
    (selected, item, index) =>
      item.sentenceIndex! <= currentSentenceIndex ? index : selected,
    -1
  )
  const previousChapter =
    activeChapterIndex > 0 ? chapterTargets[activeChapterIndex - 1] : null
  const nextChapter =
    activeChapterIndex < chapterTargets.length - 1
      ? chapterTargets[activeChapterIndex + 1]
      : null

  return (
    <nav aria-label={t("reader.toc")}>
      {chapterTargets.length > 1 && (
        <div
          className="reader-toc-chapter-navigation"
          aria-label={t("reader.chapterNavigation")}
        >
          <button
            type="button"
            className="ui-button ui-button--quiet reader-toc-step"
            aria-label={t("reader.previousChapter")}
            disabled={!previousChapter}
            onClick={() => {
              if (previousChapter?.sentenceIndex != null) {
                onNavigate(previousChapter.sentenceIndex)
              }
            }}
          >
            <ChevronLeft className="ui-icon" aria-hidden="true" />
            <span>{t("reader.previous")}</span>
          </button>
          <span
            className="reader-toc-chapter-position"
            role="status"
            aria-live="polite"
          >
            {t("reader.chapterPosition", {
              current: Math.max(1, activeChapterIndex + 1),
              total: chapterTargets.length,
            })}
          </span>
          <button
            type="button"
            className="ui-button ui-button--quiet reader-toc-step"
            aria-label={t("reader.nextChapter")}
            disabled={!nextChapter}
            onClick={() => {
              if (nextChapter?.sentenceIndex != null) {
                onNavigate(nextChapter.sentenceIndex)
              }
            }}
          >
            <span>{t("reader.next")}</span>
            <ChevronRight className="ui-icon" aria-hidden="true" />
          </button>
        </div>
      )}
      <ul className="reader-toc">
        {items.map((item, index) => {
          const isActive = index === projectedActiveIndex
          const depth = Math.min(item.depth ?? 0, 4)

          return (
            <li key={`${item.id}-${index}`}>
              <button
                type="button"
                onClick={() => {
                  if (item.sentenceIndex != null) {
                    onNavigate(item.sentenceIndex)
                  } else {
                    onItemClick?.(index)
                  }
                }}
                className="reader-toc-item"
                data-active={isActive}
                data-depth={depth}
              >
                <span className="line-clamp-2">{item.title}</span>
              </button>
            </li>
          )
        })}
      </ul>
    </nav>
  )
}
