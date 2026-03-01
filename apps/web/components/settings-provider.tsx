"use client";

import { useEffect } from "react";
import { useTheme } from "next-themes";
import { useSettings } from "@/lib/hooks";
import { applyAccentColor, applyFontSize } from "@/lib/settings-utils";

/**
 * Thin provider that applies backend-persisted settings on app load.
 * Renders no UI — only runs side-effects to sync CSS custom properties
 * (accent color, font size) from the settings API.
 *
 * Also re-applies accent color when the theme changes (light ↔ dark),
 * since highlight colors derive from the theme-specific accent variant.
 */
export function SettingsProvider({ children }: { children: React.ReactNode }) {
  const { settings, isLoading } = useSettings();
  const { resolvedTheme } = useTheme();

  // Apply persisted values once settings are fetched,
  // and re-apply accent when theme toggles (light/dark use different color variants)
  useEffect(() => {
    if (isLoading) return;

    // Accent color (+ derived highlight vars)
    const accentIndex = parseInt(settings.accent_color ?? "0", 10);
    applyAccentColor(accentIndex);

    // Reader font size
    const fontSize = parseInt(settings.font_size ?? "18", 10);
    applyFontSize(fontSize);
  }, [settings, isLoading, resolvedTheme]);

  return <>{children}</>;
}
