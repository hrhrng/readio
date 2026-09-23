"use client"

import { useState } from "react"
import { Search as SearchIcon } from "lucide-react"
import { useSearchItems } from "@/lib/hooks"
import { BookList } from "@/components/book-list"
import { EmptyState } from "@/components/empty-state"
import { LibraryListSkeleton } from "@/components/library-list-skeleton"
import { PageLayout } from "@/components/page-layout"
import { useLocale } from "@/components/locale-provider"

export default function SearchPage() {
  const { t } = useLocale()
  const [query, setQuery] = useState("")
  const { data, isLoading, error } = useSearchItems(query)

  return (
    <PageLayout
      kind="search"
      title={t("search.title")}
      description={t("search.description")}
    >
      <div className="search-field">
        <SearchIcon
          className="ui-icon search-field-icon"
          aria-hidden="true"
        />
        <input
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t("search.placeholder")}
          className="ui-input"
          aria-label={t("search.placeholder")}
          autoFocus
        />
      </div>

      <div className="search-results" aria-live="polite">
        {!query ? (
          <EmptyState
            icon={SearchIcon}
            title={t("search.placeholder")}
            description={t("search.emptyDescription")}
          />
        ) : isLoading ? (
          <LibraryListSkeleton rows={4} layout="list" />
        ) : error ? (
          <EmptyState
            icon={SearchIcon}
            title={t("search.unavailableTitle")}
            description={t("search.unavailableDescription")}
          />
        ) : data && data.items.length > 0 ? (
          <BookList items={data.items} layout="list" />
        ) : (
          <EmptyState
            icon={SearchIcon}
            title={t("search.noResultsTitle")}
            description={t("search.noResultsDescription", { query })}
          />
        )}
      </div>
    </PageLayout>
  )
}
