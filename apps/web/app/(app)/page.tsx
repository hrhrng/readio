"use client"

import { BookOpen, CircleAlert } from "lucide-react"
import { useLibraryItems } from "@/lib/hooks"
import { SectionCarousel } from "@/components/section-carousel"
import { BookCard } from "@/components/book-card"
import { EmptyState } from "@/components/empty-state"
import { PageLayout } from "@/components/page-layout"
import { BuiltinSampleLink } from "@/components/builtin-sample-link"
import { useLocale } from "@/components/locale-provider"

function OverviewSkeleton() {
  const { t } = useLocale()
  return (
    <div className="section-stack" aria-label={t("home.loading")}>
      {Array.from({ length: 2 }, (_, section) => (
        <section className="content-section" key={section}>
          <div className="skeleton skeleton-line overview-skeleton-heading" />
          <div className="carousel-track">
            {Array.from({ length: 4 }, (_, item) => (
              <div className="carousel-item" key={item}>
                <div className="skeleton book-tile-skeleton" />
                <div className="skeleton skeleton-line overview-skeleton-copy" />
              </div>
            ))}
          </div>
        </section>
      ))}
    </div>
  )
}

export default function HomePage() {
  const { t } = useLocale()
  const continueQuery = useLibraryItems({
    progress_min: 1,
    progress_max: 99,
    sort_by: "created_at",
    sort_order: "desc",
    page_size: 10,
  })
  const wantToReadQuery = useLibraryItems({
    progress_min: 0,
    progress_max: 0,
    sort_by: "created_at",
    sort_order: "desc",
    page_size: 10,
  })
  const recentQuery = useLibraryItems({
    sort_by: "created_at",
    sort_order: "desc",
    page_size: 10,
  })

  const continueItems = continueQuery.data?.items ?? []
  const wantToReadItems = wantToReadQuery.data?.items ?? []
  const recentItems = recentQuery.data?.items ?? []
  const isLoading =
    continueQuery.isLoading &&
    wantToReadQuery.isLoading &&
    recentQuery.isLoading
  const hasError =
    continueQuery.error || wantToReadQuery.error || recentQuery.error
  const hasContent =
    continueItems.length > 0 ||
    wantToReadItems.length > 0 ||
    recentItems.length > 0

  return (
    <PageLayout
      kind="home"
      title={t("home.title")}
      description={t("home.description")}
    >
      {isLoading ? (
        <OverviewSkeleton />
      ) : hasError && !hasContent ? (
        <EmptyState
          icon={CircleAlert}
          title={t("home.loadErrorTitle")}
          description={t("home.loadErrorDescription")}
        />
      ) : hasContent ? (
        <div className="section-stack">
          {continueItems.length > 0 && (
            <SectionCarousel
              title={t("home.continue")}
              subtitle={t("home.continueSubtitle")}
            >
              {continueItems.map((item) => (
                <div key={item.id} className="carousel-item">
                  <BookCard item={item} variant="grid" />
                </div>
              ))}
            </SectionCarousel>
          )}

          {wantToReadItems.length > 0 && (
            <SectionCarousel
              title={t("home.wantToRead")}
              href="/library/want-to-read"
            >
              {wantToReadItems.map((item) => (
                <div key={item.id} className="carousel-item">
                  <BookCard item={item} variant="grid" />
                </div>
              ))}
            </SectionCarousel>
          )}

          {recentItems.length > 0 && (
            <SectionCarousel
              title={t("home.recent")}
              href="/library/all"
              subtitle={t("home.recentSubtitle")}
            >
              {recentItems.map((item) => (
                <div key={item.id} className="carousel-item">
                  <BookCard item={item} variant="grid" />
                </div>
              ))}
            </SectionCarousel>
          )}
        </div>
      ) : (
        <EmptyState
          icon={BookOpen}
          title={t("home.emptyTitle")}
          description={t("home.emptyDescription")}
          action={<BuiltinSampleLink />}
        />
      )}
    </PageLayout>
  )
}
