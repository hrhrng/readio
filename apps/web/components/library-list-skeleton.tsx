"use client"

import { useLocale } from "./locale-provider"

export function LibraryListSkeleton({
  rows = 8,
  layout = "shelf",
}: {
  rows?: number
  layout?: "shelf" | "list"
}) {
  const { t } = useLocale()
  if (layout === "shelf") {
    return (
      <div
        className="library-shelf-grid library-shelf-skeleton"
        aria-label={t("library.loadingItems")}
      >
        {Array.from({ length: rows }, (_, index) => (
          <div className="book-tile" key={index}>
            <span className="skeleton book-tile-skeleton" />
            <span className="skeleton skeleton-line" />
            <span className="skeleton skeleton-line skeleton-line--short" />
          </div>
        ))}
      </div>
    )
  }

  return (
    <div className="library-list-skeleton" aria-label={t("library.loadingItems")}>
      {Array.from({ length: rows }, (_, index) => (
        <div className="library-list-skeleton-row" key={index}>
          <span className="skeleton library-list-skeleton-cover" />
          <span className="library-list-skeleton-copy">
            <span className="skeleton skeleton-line" />
            <span className="skeleton skeleton-line skeleton-line--short" />
          </span>
        </div>
      ))}
    </div>
  )
}
