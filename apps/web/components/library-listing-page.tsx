"use client"

import { useState } from "react"
import { LucideIcon } from "lucide-react"
import { useLibraryItems } from "@/lib/hooks"
import type { LibraryItemType } from "@/lib/types"
import { BookList } from "./book-list"
import { EmptyState } from "./empty-state"
import { LibraryListSkeleton } from "./library-list-skeleton"
import {
  LibraryViewMenu,
  type LibraryLayout,
} from "./library-view-menu"
import { PageLayout } from "./page-layout"
import { BuiltinSampleLink } from "./builtin-sample-link"
import { shouldOfferBuiltinSample } from "@/lib/builtin-sample-entry"
import { useLocale } from "./locale-provider"

interface LibraryListingPageProps {
  title: string
  description: string
  icon: LucideIcon
  type?: LibraryItemType
  progressMin?: number
  progressMax?: number
}

export function LibraryListingPage({
  title,
  description,
  icon,
  type,
  progressMin,
  progressMax,
}: LibraryListingPageProps) {
  const { t } = useLocale()
  const sortOptions = [
    { value: "created_at", label: t("library.sortDateAdded") },
    { value: "title", label: t("library.sortTitle") },
    { value: "progress", label: t("library.sortProgress") },
  ]
  const [sortBy, setSortBy] = useState("created_at")
  const [layout, setLayout] = useState<LibraryLayout>("shelf")
  const { data, isLoading, error } = useLibraryItems({
    type,
    progress_min: progressMin,
    progress_max: progressMax,
    sort_by: sortBy,
    sort_order: sortBy === "title" ? "asc" : "desc",
    page_size: 100,
  })

  const items = data?.items ?? []
  const offersBuiltinSample = shouldOfferBuiltinSample({
    itemCount: items.length,
    type,
    progressMin,
    progressMax,
  })

  return (
    <PageLayout
      kind="library"
      title={title}
      description={description}
      actions={
        <LibraryViewMenu
          layout={layout}
          sortBy={sortBy}
          sortOptions={sortOptions}
          onLayoutChange={setLayout}
          onSortChange={setSortBy}
        />
      }
    >
      {isLoading ? (
        <LibraryListSkeleton layout={layout} />
      ) : error ? (
        <EmptyState
          icon={icon}
          title={t("library.loadErrorTitle", { title })}
          description={t("library.loadErrorDescription")}
        />
      ) : items.length > 0 ? (
        <BookList items={items} layout={layout} />
      ) : (
        <EmptyState
          icon={icon}
          title={t("library.emptyTitle", { title })}
          description={t("library.emptyDescription")}
          action={offersBuiltinSample ? <BuiltinSampleLink /> : undefined}
        />
      )}
    </PageLayout>
  )
}
