"use client";

import { useLibraryItems } from "@/lib/hooks";
import { SectionCarousel } from "@/components/section-carousel";
import { BookCard } from "@/components/book-card";
import { EmptyState } from "@/components/empty-state";
import { BookOpen } from "lucide-react";

export default function HomePage() {
  const { data: continueData } = useLibraryItems({
    progress_min: 1,
    progress_max: 99,
    sort_by: "created_at",
    sort_order: "desc",
    page_size: 10,
  });

  const { data: wantToReadData } = useLibraryItems({
    progress_min: 0,
    progress_max: 0,
    sort_by: "created_at",
    sort_order: "desc",
    page_size: 10,
  });

  const { data: recentData } = useLibraryItems({
    sort_by: "created_at",
    sort_order: "desc",
    page_size: 10,
  });

  const continueItems = continueData?.items ?? [];
  const wantToReadItems = wantToReadData?.items ?? [];
  const recentItems = recentData?.items ?? [];

  const hasContent =
    continueItems.length > 0 ||
    wantToReadItems.length > 0 ||
    recentItems.length > 0;

  if (!hasContent && !continueData && !wantToReadData && !recentData) {
    // Still loading
    return (
      <div className="animate-pulse space-y-8">
        <div className="h-8 w-32 bg-surface-hover rounded" />
        <div className="flex gap-4">
          {[1, 2, 3].map((i) => (
            <div
              key={i}
              className="w-[160px] h-[240px] bg-surface-hover rounded-xl shrink-0"
            />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div>
      <h1 className="text-3xl font-bold font-serif text-text-primary mb-8">
        Home
      </h1>

      {continueItems.length > 0 && (
        <SectionCarousel title="Continue" subtitle="Pick up where you left off">
          {continueItems.map((item) => (
            <div key={item.id} className="w-[160px] shrink-0">
              <BookCard item={item} variant="grid" />
            </div>
          ))}
        </SectionCarousel>
      )}

      {wantToReadItems.length > 0 && (
        <SectionCarousel
          title="Want to Read"
          href="/library/want-to-read"
        >
          {wantToReadItems.map((item) => (
            <div key={item.id} className="w-[160px] shrink-0">
              <BookCard item={item} variant="grid" />
            </div>
          ))}
        </SectionCarousel>
      )}

      {recentItems.length > 0 && (
        <SectionCarousel
          title="Recently Added"
          href="/library/all"
          subtitle="New additions to your library"
        >
          {recentItems.map((item) => (
            <div key={item.id} className="w-[160px] shrink-0">
              <BookCard item={item} variant="grid" />
            </div>
          ))}
        </SectionCarousel>
      )}

      {hasContent ||
        (!continueData && !wantToReadData && !recentData ? null : (
          <EmptyState
            icon={BookOpen}
            title="Your library is empty"
            description="Import articles, PDFs, or ebooks using the browser extension to get started."
          />
        ))}
    </div>
  );
}
