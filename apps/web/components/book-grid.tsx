import { LibraryItemSummary } from "@/lib/types";
import { BookCard } from "./book-card";

interface BookGridProps {
  items: LibraryItemSummary[];
}

export function BookGrid({ items }: BookGridProps) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-6">
      {items.map((item) => (
        <BookCard key={item.id} item={item} variant="grid" />
      ))}
    </div>
  );
}
