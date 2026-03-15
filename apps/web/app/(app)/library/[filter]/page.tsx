"use client";

import { use, useState } from "react";
import { useLibraryItems } from "@/lib/hooks";
import { BookGrid } from "@/components/book-grid";
import { EmptyState } from "@/components/empty-state";
import { BookOpen, Clock, CheckCircle } from "lucide-react";
import { SortSelect } from "@/components/sort-select";

const filterConfig: Record<
  string,
  {
    title: string;
    icon: typeof BookOpen;
    progress_min?: number;
    progress_max?: number;
  }
> = {
  all: { title: "All", icon: BookOpen },
  "want-to-read": {
    title: "Want to Read",
    icon: Clock,
    progress_min: 0,
    progress_max: 0,
  },
  finished: {
    title: "Finished",
    icon: CheckCircle,
    progress_min: 100,
    progress_max: 100,
  },
};

const sortOptions = [
  { value: "created_at", label: "Date Added" },
  { value: "title", label: "Title" },
  { value: "progress", label: "Progress" },
];

export default function LibraryFilterPage({
  params,
}: {
  params: Promise<{ filter: string }>;
}) {
  const { filter } = use(params);
  const config = filterConfig[filter] ?? filterConfig.all;
  const [sortBy, setSortBy] = useState("created_at");

  const { data, isLoading } = useLibraryItems({
    progress_min: config.progress_min,
    progress_max: config.progress_max,
    sort_by: sortBy,
    sort_order: sortBy === "title" ? "asc" : "desc",
    page_size: 100,
  });

  const items = data?.items ?? [];
  const Icon = config.icon;

  return (
    <div>
      <div className="flex items-center justify-between mb-8">
        <h1 className="text-3xl font-bold font-serif text-text-primary">
          {config.title}
        </h1>
        <SortSelect value={sortBy} onChange={setSortBy} options={sortOptions} />
      </div>

      {isLoading ? (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-6">
          {[1, 2, 3, 4, 5, 6].map((i) => (
            <div
              key={i}
              className="aspect-[3/4] bg-surface-hover rounded-xl animate-pulse"
            />
          ))}
        </div>
      ) : items.length > 0 ? (
        <BookGrid items={items} />
      ) : (
        <EmptyState
          icon={Icon}
          title={`No items in "${config.title}"`}
          description="Import content using the browser extension to build your library."
        />
      )}
    </div>
  );
}
