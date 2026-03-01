"use client";

import { useRouter } from "next/navigation";
import { ArrowLeft, List, Settings } from "lucide-react";

interface TopBarProps {
  title: string;
  onToggleToc: () => void;
  onToggleSettings: () => void;
  tocOpen: boolean;
}

export function TopBar({
  title,
  onToggleToc,
  onToggleSettings,
  tocOpen,
}: TopBarProps) {
  const router = useRouter();

  return (
    <div className="fixed top-0 left-0 right-0 h-12 bg-surface border-b border-border z-40 flex items-center px-4">
      <button
        onClick={() => router.back()}
        className="flex items-center gap-1.5 text-text-secondary hover:text-text-primary transition-colors duration-200 min-w-[44px] min-h-[44px] justify-center cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent rounded-lg"
        aria-label="Go back"
      >
        <ArrowLeft size={18} />
        <span className="text-sm hidden sm:inline">Back</span>
      </button>

      <h1 className="flex-1 text-sm font-medium text-text-primary text-center truncate mx-4">
        {title}
      </h1>

      <div className="flex items-center gap-1">
        <button
          onClick={onToggleToc}
          className={`min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${
            tocOpen
              ? "text-accent bg-surface-active"
              : "text-text-secondary hover:text-text-primary"
          }`}
          data-toc-toggle
          aria-label="Toggle table of contents"
        >
          <List size={18} />
        </button>
        <button
          onClick={onToggleSettings}
          className="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-text-secondary hover:text-text-primary transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
          aria-label="Settings"
        >
          <Settings size={18} />
        </button>
      </div>
    </div>
  );
}
