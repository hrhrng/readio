"use client";

import { useTheme } from "next-themes";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Sun, Moon, Monitor, Check, X } from "lucide-react";

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

const NAV_ITEMS = [
  { id: "appearance", label: "Appearance" },
  { id: "reading", label: "Reading" },
  { id: "about", label: "About" },
] as const;

type SectionId = (typeof NAV_ITEMS)[number]["id"];

const ACCENT_PRESETS = [
  { name: "Sage", light: "#6B8F71", dark: "#7EC8A0" },
  { name: "Blue", light: "#4A7FBF", dark: "#6EB5FF" },
  { name: "Amber", light: "#B8860B", dark: "#E8B84B" },
  { name: "Rose", light: "#C26B7E", dark: "#E8909F" },
  { name: "Violet", light: "#7C6BAF", dark: "#A594D8" },
] as const;

/**
 * Full-screen modal overlay for settings.
 * Follows the same left-nav + right-content layout as Claude's settings dialog.
 * Closes on ESC or clicking the backdrop.
 */
export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  const overlayRef = useRef<HTMLDivElement>(null);
  const { theme, setTheme } = useTheme();
  const [mounted, setMounted] = useState(false);
  const [activeSection, setActiveSection] = useState<SectionId>("appearance");
  const [accentIndex, setAccentIndex] = useState(0);

  useEffect(() => setMounted(true), []);

  // Close on ESC
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open || !mounted) return null;

  return createPortal(
    <div
      ref={overlayRef}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      onClick={(e) => {
        if (e.target === overlayRef.current) onClose();
      }}
    >
      {/* Dialog card */}
      <div className="bg-surface rounded-2xl shadow-2xl w-[780px] h-[560px] overflow-hidden flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-border shrink-0">
          <h2 className="text-lg font-semibold text-text-primary">Settings</h2>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-text-tertiary hover:text-text-primary hover:bg-surface-hover transition-colors cursor-pointer"
          >
            <X size={18} />
          </button>
        </div>

        {/* Body: left nav + right content */}
        <div className="flex flex-1 min-h-0">
          {/* Left nav */}
          <nav className="w-40 shrink-0 border-r border-border py-3 px-2">
            <ul className="flex flex-col gap-0.5">
              {NAV_ITEMS.map(({ id, label }) => (
                <li key={id}>
                  <button
                    onClick={() => setActiveSection(id)}
                    className={`w-full text-left px-3 py-1.5 rounded-lg text-sm transition-colors cursor-pointer ${
                      activeSection === id
                        ? "bg-surface-active text-text-primary font-medium"
                        : "text-text-secondary hover:bg-surface-hover hover:text-text-primary"
                    }`}
                  >
                    {label}
                  </button>
                </li>
              ))}
            </ul>
          </nav>

          {/* Right content — scrollable */}
          <div className="flex-1 overflow-y-auto p-6">
            {activeSection === "appearance" && (
              <AppearanceSection
                theme={theme}
                setTheme={setTheme}
                accentIndex={accentIndex}
                setAccentIndex={setAccentIndex}
              />
            )}
            {activeSection === "reading" && <ReadingSection />}
            {activeSection === "about" && <AboutSection />}
          </div>
        </div>
      </div>
    </div>,
    document.body
  );
}

/* ─────────────── Section: Appearance ─────────────── */

function AppearanceSection({
  theme,
  setTheme,
  accentIndex,
  setAccentIndex,
}: {
  theme: string | undefined;
  setTheme: (t: string) => void;
  accentIndex: number;
  setAccentIndex: (i: number) => void;
}) {
  const modes = [
    { value: "light", icon: Sun, label: "Light" },
    { value: "dark", icon: Moon, label: "Dark" },
    { value: "system", icon: Monitor, label: "System" },
  ] as const;

  return (
    <div className="space-y-8">
      <div>
        <SectionHeading>Color mode</SectionHeading>
        <div className="flex gap-3">
          {modes.map(({ value, icon: Icon, label }) => (
            <button
              key={value}
              onClick={() => setTheme(value)}
              className={`flex flex-col items-center gap-2 rounded-xl border-2 px-6 py-4 transition-all cursor-pointer ${
                theme === value
                  ? "border-accent bg-surface-active"
                  : "border-border hover:border-text-tertiary bg-surface-card"
              }`}
            >
              <Icon
                size={22}
                strokeWidth={1.5}
                className={theme === value ? "text-accent" : "text-text-secondary"}
              />
              <span
                className={`text-xs font-medium ${theme === value ? "text-accent" : "text-text-secondary"}`}
              >
                {label}
              </span>
            </button>
          ))}
        </div>
      </div>

      <Divider />

      <div>
        <SectionHeading>Accent color</SectionHeading>
        <div className="flex gap-3">
          {ACCENT_PRESETS.map((preset, i) => (
            <button
              key={preset.name}
              onClick={() => {
                setAccentIndex(i);
                document.documentElement.style.setProperty("--accent", preset.light);
                const dark = document.querySelector(".dark");
                if (dark) {
                  (dark as HTMLElement).style.setProperty("--accent", preset.dark);
                }
              }}
              title={preset.name}
              className="relative w-9 h-9 rounded-full border-2 transition-all cursor-pointer flex items-center justify-center"
              style={{
                backgroundColor: preset.light,
                borderColor: accentIndex === i ? preset.light : "transparent",
                boxShadow:
                  accentIndex === i
                    ? `0 0 0 2px var(--surface-card), 0 0 0 4px ${preset.light}`
                    : "none",
              }}
            >
              {accentIndex === i && (
                <Check size={16} strokeWidth={2.5} className="text-white" />
              )}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

/* ─────────────── Section: Reading ─────────────── */

function ReadingSection() {
  return (
    <div className="space-y-8">
      <div>
        <SectionHeading>Default font size</SectionHeading>
        <p className="text-sm text-text-tertiary mb-3">
          Adjust the default reading font size across all books.
        </p>
        <div className="flex items-center gap-4">
          <input
            type="range"
            min={14}
            max={24}
            defaultValue={18}
            className="flex-1 accent-accent"
          />
          <span className="text-sm text-text-secondary font-medium tabular-nums w-10 text-right">
            18px
          </span>
        </div>
      </div>

      <Divider />

      <div>
        <SectionHeading>TTS speed</SectionHeading>
        <p className="text-sm text-text-tertiary mb-3">
          Default playback speed for text-to-speech.
        </p>
        <div className="flex items-center gap-4">
          <input
            type="range"
            min={0.5}
            max={2}
            step={0.1}
            defaultValue={1}
            className="flex-1 accent-accent"
          />
          <span className="text-sm text-text-secondary font-medium tabular-nums w-10 text-right">
            1.0x
          </span>
        </div>
      </div>
    </div>
  );
}

/* ─────────────── Section: About ─────────────── */

function AboutSection() {
  return (
    <div className="space-y-4">
      <SectionHeading>About</SectionHeading>
      <div className="bg-surface-card border border-border rounded-xl p-5 space-y-3">
        <div className="flex items-baseline gap-3">
          <span className="text-lg font-serif font-bold text-text-primary">
            Readio
          </span>
          <span className="text-xs text-text-tertiary">v0.1.0</span>
        </div>
        <p className="text-sm text-text-secondary leading-relaxed">
          Open-source audiobook &amp; TTS reading platform. Turn any text, URL,
          EPUB, or PDF into a listenable experience.
        </p>
        <a
          href="https://github.com/nicepkg/readio"
          target="_blank"
          rel="noopener noreferrer"
          className="inline-block text-sm text-accent hover:underline"
        >
          View on GitHub
        </a>
      </div>
    </div>
  );
}

/* ─── Shared helpers ─── */

function SectionHeading({ children }: { children: React.ReactNode }) {
  return <h3 className="text-sm font-medium text-text-primary mb-2">{children}</h3>;
}

function Divider() {
  return <div className="border-t border-border" />;
}
