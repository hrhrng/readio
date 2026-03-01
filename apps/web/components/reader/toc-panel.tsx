"use client";

interface TOCItem {
  id: string;
  title: string;
  depth?: number; // 0 = top-level, 1+ = indented sub-items
}

interface TOCPanelProps {
  items: TOCItem[];
  onItemClick: (index: number) => void;
  activeIndex?: number;
}

export function TOCPanel({ items, onItemClick, activeIndex }: TOCPanelProps) {
  return (
    <nav className="w-64 max-h-[calc(100vh-3.5rem-96px)] overflow-y-auto bg-surface-card backdrop-blur-xl rounded-xl shadow-xl py-2 px-2">
      {items.length === 0 ? (
        <p className="px-3 py-4 text-sm text-text-tertiary">
          No table of contents available.
        </p>
      ) : (
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
                      : "text-text-primary hover:bg-surface-hover"
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
                      ${depth > 0 && !isActive ? "text-text-secondary group-hover:text-text-primary" : ""}
                    `}
                  >
                    {item.title}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </nav>
  );
}
