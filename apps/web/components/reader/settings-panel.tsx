"use client";

import { useEffect, useState } from "react";
import { ThemeToggle } from "@/components/theme-toggle";
import { X } from "lucide-react";

interface SettingsPanelProps {
  onClose: () => void;
}

type FontSize = "small" | "medium" | "large";

const FONT_SIZE_MAP: Record<FontSize, string> = {
  small: "16px",
  medium: "18px",
  large: "22px",
};

export function SettingsPanel({ onClose }: SettingsPanelProps) {
  const [fontSize, setFontSize] = useState<FontSize>("medium");

  useEffect(() => {
    const saved = localStorage.getItem("readio-font-size") as FontSize | null;
    if (saved && saved in FONT_SIZE_MAP) {
      setFontSize(saved);
    }
  }, []);

  const handleFontSizeChange = (size: FontSize) => {
    setFontSize(size);
    localStorage.setItem("readio-font-size", size);
    document.documentElement.style.setProperty(
      "--reader-font-size",
      FONT_SIZE_MAP[size]
    );
    // Notify same-tab listeners (storage event only fires cross-tab)
    window.dispatchEvent(new CustomEvent("readio-font-change"));
  };

  return (
    <div className="absolute top-12 right-4 w-64 bg-surface-card border border-border rounded-xl shadow-lg z-50 overflow-hidden">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <h3 className="text-sm font-semibold text-text-primary">Settings</h3>
        <button
          onClick={onClose}
          className="min-w-[32px] min-h-[32px] flex items-center justify-center rounded-md text-text-secondary hover:text-text-primary transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
          aria-label="Close settings"
        >
          <X size={16} />
        </button>
      </div>

      <div className="p-4 space-y-4">
        {/* Font Size */}
        <div>
          <label className="text-xs font-medium text-text-secondary uppercase tracking-wider mb-2 block">
            Font Size
          </label>
          <div className="flex items-center gap-1 rounded-lg bg-surface-hover p-1">
            {(["small", "medium", "large"] as const).map((size) => (
              <button
                key={size}
                onClick={() => handleFontSizeChange(size)}
                className={`flex-1 rounded-md px-2.5 py-1.5 text-xs capitalize transition-colors duration-200 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${
                  fontSize === size
                    ? "bg-surface-card text-text-primary shadow-sm"
                    : "text-text-secondary hover:text-text-primary"
                }`}
              >
                {size}
              </button>
            ))}
          </div>
        </div>

        {/* Theme */}
        <div>
          <label className="text-xs font-medium text-text-secondary uppercase tracking-wider mb-2 block">
            Theme
          </label>
          <ThemeToggle />
        </div>
      </div>
    </div>
  );
}
