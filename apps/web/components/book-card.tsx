"use client";

import { useState, useCallback } from "react";
import { useRouter } from "next/navigation";
import { MoreHorizontal, Trash2 } from "lucide-react";
import { useSWRConfig } from "swr";
import { LibraryItemSummary, getReadingStatus } from "@/lib/types";
import { deleteItem } from "@/lib/api";
import { ConfirmDialog } from "./confirm-dialog";

const typeGradients: Record<string, string> = {
  web: "from-emerald-900 to-green-800",
  pdf: "from-red-900 to-orange-800",
  epub: "from-emerald-900 to-teal-800",
  txt: "from-gray-700 to-gray-600",
};

const typeLabels: Record<string, string> = {
  web: "WEB",
  pdf: "PDF",
  epub: "EPUB",
  txt: "TXT",
};

function CoverPlaceholder({
  item,
  className,
}: {
  item: LibraryItemSummary;
  className?: string;
}) {
  const gradient = typeGradients[item.type] || typeGradients.txt;
  return (
    <div
      className={`bg-gradient-to-br ${gradient} flex flex-col items-center justify-center p-4 ${className}`}
    >
      <span className="text-white/60 text-[10px] font-semibold uppercase tracking-widest mb-2">
        {typeLabels[item.type]}
      </span>
      <span className="text-white text-sm font-medium text-center line-clamp-3 leading-snug">
        {item.title}
      </span>
    </div>
  );
}

/**
 * Renders the book's actual cover image when available (e.g. EPUB cover),
 * falling back to the gradient-based CoverPlaceholder for non-EPUB items.
 */
function BookCover({
  item,
  className,
}: {
  item: LibraryItemSummary;
  className?: string;
}) {
  if (item.cover_image) {
    return (
      <img
        src={item.cover_image}
        alt={item.title}
        className={`object-cover ${className}`}
      />
    );
  }
  return <CoverPlaceholder item={item} className={className} />;
}

interface BookCardProps {
  item: LibraryItemSummary;
  variant?: "grid" | "list";
}

export function BookCard({ item, variant = "grid" }: BookCardProps) {
  const router = useRouter();
  const { mutate } = useSWRConfig();
  const status = getReadingStatus(item.progress);
  const [menuOpen, setMenuOpen] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const handleClick = () => {
    if (menuOpen || confirmOpen) return;
    router.push(`/player/${item.id}`);
  };

  const handleDelete = useCallback(async () => {
    setDeleting(true);
    try {
      await deleteItem(item.id);
      setConfirmOpen(false);
      // Revalidate all library queries without clearing cached data
      mutate(
        (key: unknown) =>
          Array.isArray(key) &&
          (key[0] === "library-items" || key[0] === "search")
      );
    } catch {
      // Silently fail — item may already be deleted
    } finally {
      setDeleting(false);
    }
  }, [item.id, mutate]);

  if (variant === "list") {
    return (
      <>
        <div className="flex items-center gap-4 w-full rounded-xl p-3 text-left transition-colors hover:bg-surface-hover group relative">
          <button
            onClick={handleClick}
            className="flex items-center gap-4 flex-1 min-w-0 cursor-pointer"
          >
            <BookCover
              item={item}
              className="w-[60px] h-[80px] rounded-lg shrink-0"
            />
            <div className="flex-1 min-w-0">
              <h3 className="text-sm font-medium text-text-primary truncate">
                {item.title}
              </h3>
              <p className="text-xs text-text-secondary mt-1">{item.source}</p>
              <div className="flex items-center gap-2 mt-2">
                <span className="text-[10px] font-medium uppercase px-1.5 py-0.5 rounded bg-surface-hover text-text-secondary">
                  {typeLabels[item.type]}
                </span>
                {status === "reading" && (
                  <span className="text-[10px] text-text-tertiary">
                    {item.progress}%
                  </span>
                )}
                {status === "finished" && (
                  <span className="text-[10px] text-accent">Finished</span>
                )}
              </div>
            </div>
          </button>
          {/* Menu button */}
          <button
            onClick={(e) => {
              e.stopPropagation();
              setMenuOpen(true);
            }}
            className="w-8 h-8 flex items-center justify-center rounded-lg opacity-0 group-hover:opacity-100 hover:bg-surface-active transition-all text-text-tertiary hover:text-text-primary shrink-0 cursor-pointer"
            aria-label="More options"
          >
            <MoreHorizontal size={16} />
          </button>
          {/* Dropdown menu */}
          {menuOpen && (
            <>
              <div
                className="fixed inset-0 z-40"
                onClick={() => setMenuOpen(false)}
              />
              <div className="absolute right-3 top-12 z-40 bg-surface-card border border-border rounded-xl shadow-lg py-1 min-w-[140px]">
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    setMenuOpen(false);
                    setConfirmOpen(true);
                  }}
                  className="flex items-center gap-2 w-full px-3 py-2 text-sm text-red-500 hover:bg-surface-hover transition-colors cursor-pointer"
                >
                  <Trash2 size={14} />
                  Delete
                </button>
              </div>
            </>
          )}
        </div>
        <ConfirmDialog
          open={confirmOpen}
          title="Delete item?"
          description={`"${item.title}" will be permanently removed from your library.`}
          confirmLabel="Delete"
          destructive
          loading={deleting}
          onConfirm={handleDelete}
          onCancel={() => setConfirmOpen(false)}
        />
      </>
    );
  }

  return (
    <>
      <div className="group flex flex-col text-left transition-all relative">
        <button
          onClick={handleClick}
          className="flex flex-col text-left cursor-pointer"
        >
          <div className="relative rounded-xl overflow-hidden shadow-sm dark:shadow-none transition-shadow group-hover:shadow-md dark:group-hover:shadow-none">
            <BookCover item={item} className="w-full aspect-[3/4]" />
            {/* NEW badge */}
            {status === "new" && (
              <span className="absolute top-2 right-2 bg-badge-new text-white text-[10px] font-bold px-1.5 py-0.5 rounded">
                NEW
              </span>
            )}
            {/* Progress bar */}
            {status === "reading" && (
              <div className="absolute bottom-0 left-0 right-0 h-1 bg-black/20">
                <div
                  className="h-full bg-accent transition-all"
                  style={{ width: `${item.progress}%` }}
                />
              </div>
            )}
          </div>
        </button>
        {/* Menu button overlay */}
        <button
          onClick={(e) => {
            e.stopPropagation();
            setMenuOpen(true);
          }}
          className="absolute top-2 left-2 w-7 h-7 bg-black/50 backdrop-blur-sm rounded-full flex items-center justify-center text-white opacity-0 group-hover:opacity-100 hover:bg-black/70 transition-all cursor-pointer z-10"
          aria-label="More options"
        >
          <MoreHorizontal size={14} />
        </button>
        {/* Dropdown menu */}
        {menuOpen && (
          <>
            <div
              className="fixed inset-0 z-40"
              onClick={() => setMenuOpen(false)}
            />
            <div className="absolute left-2 top-10 z-40 bg-surface-card border border-border rounded-xl shadow-lg py-1 min-w-[140px]">
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  setMenuOpen(false);
                  setConfirmOpen(true);
                }}
                className="flex items-center gap-2 w-full px-3 py-2 text-sm text-red-500 hover:bg-surface-hover transition-colors cursor-pointer"
              >
                <Trash2 size={14} />
                Delete
              </button>
            </div>
          </>
        )}
        <button
          onClick={handleClick}
          className="text-left cursor-pointer"
        >
          <h3 className="mt-2 text-sm font-medium text-text-primary truncate leading-snug">
            {item.title}
          </h3>
          <p className="text-xs text-text-secondary mt-0.5">
            {typeLabels[item.type]}
          </p>
        </button>
      </div>
      <ConfirmDialog
        open={confirmOpen}
        title="Delete item?"
        description={`"${item.title}" will be permanently removed from your library.`}
        confirmLabel="Delete"
        destructive
        loading={deleting}
        onConfirm={handleDelete}
        onCancel={() => setConfirmOpen(false)}
      />
    </>
  );
}
