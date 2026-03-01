/**
 * Shared utility functions for applying persisted settings to the DOM.
 * Used by both SettingsDialog (on user interaction) and SettingsProvider
 * (on initial page load) to keep behavior consistent.
 */

import { ACCENT_PRESETS } from "@/components/settings-dialog";

/**
 * Apply accent color CSS vars based on the preset index.
 *
 * next-themes uses `attribute="class"` → `.dark` is toggled on <html>.
 * We detect the current mode and apply the matching color variant.
 *
 * Highlight colors (sentence/word) are fixed warm-yellow / cool-blue tints
 * independent of accent — they ensure readability across all accent presets.
 */
export function applyAccentColor(presetIndex: number) {
  const preset = ACCENT_PRESETS[presetIndex];
  if (!preset) return;

  const root = document.documentElement;
  const isDark = root.classList.contains("dark");
  const color = isDark ? preset.dark : preset.light;

  root.style.setProperty("--accent", color);

  // Sentence = light accent tint, Word = deeper accent tint, same color family.
  const bgBase = isDark ? "#1C1C1E" : "#FFFFFF";
  const sentencePct = isDark ? 20 : 15;
  const wordPct = isDark ? 45 : 40;
  root.style.setProperty("--highlight-sentence", `color-mix(in srgb, ${color} ${sentencePct}%, ${bgBase})`);
  root.style.setProperty("--highlight-word-bg", `color-mix(in srgb, ${color} ${wordPct}%, ${bgBase})`);
  root.style.setProperty("--highlight-word", "inherit");
}

/**
 * Apply font size as a CSS custom property and dispatch the custom event
 * so reader components can react immediately.
 */
export function applyFontSize(size: number) {
  document.documentElement.style.setProperty(
    "--reader-font-size",
    `${size}px`
  );
  window.dispatchEvent(new CustomEvent("readio-font-change"));
}
