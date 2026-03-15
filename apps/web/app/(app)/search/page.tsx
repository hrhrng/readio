"use client";

import { useState } from "react";
import { Search as SearchIcon } from "lucide-react";
import { useSearchItems } from "@/lib/hooks";
import { BookCard } from "@/components/book-card";
import { EmptyState } from "@/components/empty-state";

export default function SearchPage() {
  const [query, setQuery] = useState("");
  const { data, isLoading } = useSearchItems(query);

  return (
    <div>
      <h1 className="text-3xl font-bold font-serif text-text-primary mb-8">
        Search
      </h1>

      {/* Search Input */}
      <div className="relative max-w-2xl mx-auto mb-10">
        <SearchIcon
          size={20}
          className="absolute left-4 top-1/2 -translate-y-1/2 text-text-tertiary"
        />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search your library..."
          className="w-full rounded-xl bg-surface-card border border-border px-12 py-3.5 text-base text-text-primary placeholder:text-text-tertiary focus:outline-none focus:ring-2 focus:ring-accent/50 shadow-sm dark:shadow-none"
          autoFocus
        />
      </div>

      {/* Results */}
      {!query ? (
        <EmptyState
          icon={SearchIcon}
          title="Search your library"
          description="Find articles, books, and documents by title."
        />
      ) : isLoading ? (
        <div className="max-w-2xl mx-auto space-y-2">
          {[1, 2, 3].map((i) => (
            <div key={i} className="h-20 bg-surface-hover rounded-xl animate-pulse" />
          ))}
        </div>
      ) : data && data.items.length > 0 ? (
        <div className="max-w-2xl mx-auto flex flex-col gap-1">
          {data.items.map((item) => (
            <BookCard key={item.id} item={item} variant="list" />
          ))}
        </div>
      ) : (
        <EmptyState
          icon={SearchIcon}
          title="No results found"
          description={`No items match "${query}". Try a different search term.`}
        />
      )}
    </div>
  );
}
