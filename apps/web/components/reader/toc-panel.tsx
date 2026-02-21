"use client";

import { X } from "lucide-react";

interface TOCItem {
  id: string;
  title: string;
  depth?: number; // 0 = top-level, 1+ = indented sub-items
}

interface TOCPanelProps {
  items: TOCItem[];
  onClose: () => void;
  onItemClick: (index: number) => void;
  activeIndex?: number;
}

export function TOCPanel({
  items,
  onClose,
  onItemClick,
  activeIndex,
}: TOCPanelProps) {
  return (
    <aside className="w-64 max-h-full flex flex-col bg-surface-card/80 backdrop-blur-xl border-r border-border shadow-xl">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <h2 className="text-xs font-semibold uppercase tracking-wider text-text-tertiary">
          Contents
        </h2>
        <button
          onClick={onClose}
          className="w-7 h-7 flex items-center justify-center rounded-full text-text-tertiary hover:text-text-primary hover:bg-surface-hover transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
          aria-label="Close table of contents"
        >
          <X size={14} />
        </button>
      </div>

      {/* Item list */}
      {items.length === 0 ? (
        <div className="px-4 py-8 text-center text-sm text-text-tertiary">
          No table of contents available.
        </div>
      ) : (
        <nav className="flex-1 overflow-y-auto py-2 px-2">
          <ul>
            {items.map((item, idx) => {
              const isActive = idx === activeIndex;
              const depth = item.depth ?? 0;

              return (
                <li key={`${item.id}-${idx}`}>
                  <button
                    onClick={() => onItemClick(idx)}
                    className={`
                      group relative w-full text-left py-1.5 pr-3 rounded-lg
                      transition-all duration-200 cursor-pointer
                      focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent
                      ${isActive
                        ? "bg-accent/10 text-accent"
                        : "text-text-secondary hover:text-text-primary hover:bg-surface-hover"
                      }
                    `}
                    style={{ paddingLeft: `${10 + depth * 14}px` }}
                  >
                    {/* Active indicator bar */}
                    {isActive && (
                      <span className="absolute left-1 top-1/2 -translate-y-1/2 w-[3px] h-4 rounded-full bg-accent" />
                    )}

                    <span
                      className={`
                        block text-[13px] leading-snug line-clamp-2
                        ${isActive ? "font-medium" : ""}
                        ${depth > 0 && !isActive ? "text-text-tertiary group-hover:text-text-secondary" : ""}
                      `}
                    >
                      {item.title}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        </nav>
      )}
    </aside>
  );
}
