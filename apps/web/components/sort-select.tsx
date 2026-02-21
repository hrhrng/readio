"use client";

import { useEffect, useRef, useState } from "react";
import { ChevronDown, Check } from "lucide-react";

interface SortSelectProps {
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: string }[];
}

/**
 * Custom dropdown select that replaces the native <select>.
 * Styled consistently with the app's popover/card design language.
 * Closes on outside click or ESC.
 */
export function SortSelect({ value, onChange, options }: SortSelectProps) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const currentLabel = options.find((o) => o.value === value)?.label ?? value;

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  // Close on ESC
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open]);

  return (
    <div ref={containerRef} className="relative">
      {/* Trigger button */}
      <button
        onClick={() => setOpen((v) => !v)}
        className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm transition-colors cursor-pointer ${
          open
            ? "bg-surface-hover text-text-primary"
            : "text-text-secondary hover:text-text-primary hover:bg-surface-hover"
        }`}
      >
        <span>{currentLabel}</span>
        <ChevronDown
          size={14}
          className={`text-text-tertiary transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>

      {/* Dropdown panel */}
      {open && (
        <div className="absolute right-0 top-full mt-1.5 z-50 min-w-[140px] bg-surface-card border border-border rounded-xl shadow-lg py-1">
          {options.map((opt) => (
            <button
              key={opt.value}
              onClick={() => {
                onChange(opt.value);
                setOpen(false);
              }}
              className={`flex items-center justify-between w-full px-3 py-1.5 text-sm transition-colors cursor-pointer ${
                opt.value === value
                  ? "text-text-primary font-medium"
                  : "text-text-secondary hover:text-text-primary hover:bg-surface-hover"
              }`}
            >
              <span>{opt.label}</span>
              {opt.value === value && (
                <Check size={14} className="text-accent" />
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
