"use client";

import { X } from "lucide-react";

interface TOCItem {
  id: string;
  title: string;
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
  if (items.length === 0) {
    return (
      <aside className="w-60 h-full bg-surface-sidebar border-r border-border overflow-y-auto shadow-lg">
        <div className="flex items-center justify-between p-4 border-b border-border">
          <h2 className="text-sm font-semibold text-text-primary">Contents</h2>
          <button
            onClick={onClose}
            className="min-w-[32px] min-h-[32px] flex items-center justify-center rounded-md text-text-secondary hover:text-text-primary transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
            aria-label="Close table of contents"
          >
            <X size={16} />
          </button>
        </div>
        <div className="p-4 text-sm text-text-tertiary">
          No table of contents available.
        </div>
      </aside>
    );
  }

  return (
    <aside className="w-60 h-full bg-surface-sidebar border-r border-border overflow-y-auto shadow-lg">
      <div className="flex items-center justify-between p-4 border-b border-border">
        <h2 className="text-sm font-semibold text-text-primary">Contents</h2>
        <button
          onClick={onClose}
          className="min-w-[32px] min-h-[32px] flex items-center justify-center rounded-md text-text-secondary hover:text-text-primary transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
          aria-label="Close table of contents"
        >
          <X size={16} />
        </button>
      </div>
      <nav className="p-2">
        <ul className="space-y-0.5">
          {items.map((item, idx) => (
            <li key={item.id}>
              <button
                onClick={() => onItemClick(idx)}
                className={`w-full text-left px-3 py-2 text-sm rounded-lg transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${
                  idx === activeIndex
                    ? "bg-surface-active text-accent font-medium"
                    : "text-text-secondary hover:text-text-primary hover:bg-surface-hover"
                }`}
              >
                <span className="line-clamp-2">{item.title}</span>
              </button>
            </li>
          ))}
        </ul>
      </nav>
    </aside>
  );
}
