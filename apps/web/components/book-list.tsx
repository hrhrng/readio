import React from "react"
import { LibraryItemSummary } from "@/lib/types"
import { BookCard } from "./book-card"
import { useLocale } from "./locale-provider"

interface BookListProps {
  items: LibraryItemSummary[]
  layout?: "shelf" | "list"
}

export function BookList({ items, layout = "shelf" }: BookListProps) {
  const { t } = useLocale()
  if (layout === "shelf") {
    return (
      <section className="library-shelf" aria-label={t("library.shelf")}>
        <div className="library-shelf-grid" role="list">
          {items.map((item) => (
            <BookCard key={item.id} item={item} variant="grid" />
          ))}
        </div>
      </section>
    )
  }

  return (
    <section className="library-rows" aria-label={t("library.listAria")}>
      <div className="library-rows-list" role="list">
        {items.map((item) => (
          <BookCard key={item.id} item={item} variant="list" />
        ))}
      </div>
    </section>
  )
}
