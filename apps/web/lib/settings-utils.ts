/**
 * Shared utility functions for applying persisted settings to the DOM.
 * Used by both SettingsDialog (on user interaction) and SettingsProvider
 * (on initial page load) to keep behavior consistent.
 */

import { ACCENT_PRESETS } from "./design-tokens";
import { webRustCore } from "./readio-core-runtime";
import type {
  EffortLadder,
  EffortLevel,
  ReadingMode,
  ReadingPace,
  RustCoreClient,
} from "./rust-core";

const READING_MODES: ReadingMode[] = ["manual", "auto", "read_aloud"];
const EFFORT_LEVELS: EffortLevel[] = [
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
];

export async function resolveStoredReadingPace(
  settings: Record<string, string>,
  core: RustCoreClient = webRustCore
): Promise<ReadingPace> {
  const mode = READING_MODES.includes(settings.reading_mode as ReadingMode)
    ? (settings.reading_mode as ReadingMode)
    : "manual";
  const effort = EFFORT_LEVELS.includes(
    settings.reading_effort as EffortLevel
  )
    ? (settings.reading_effort as EffortLevel)
    : "high";
  const storedBaseRate = Number(settings.reading_base_cps);
  const multipliers = Object.fromEntries(
    EFFORT_LEVELS.flatMap((level) => {
      const value = Number(settings[`reading_effort_${level}`]);
      return Number.isFinite(value) ? [[level, value]] : [];
    })
  ) as Partial<EffortLadder>;

  return core.resolveReadingPace({
    mode,
    effort,
    base_text_characters_per_second: Number.isFinite(storedBaseRate)
      ? storedBaseRate
      : 46,
    ...(Object.keys(multipliers).length > 0 ? { multipliers } : {}),
  });
}

export async function resolveInitialPlaybackRate(
  itemSpeed: number | null,
  settings: Record<string, string>,
  core: RustCoreClient = webRustCore
): Promise<number> {
  if (itemSpeed !== null) {
    const pace = await core.resolveReadingPace({
      mode: "read_aloud",
      effort: "high",
      base_text_characters_per_second: 46,
      multipliers: { high: itemSpeed },
    });
    return pace.audio_playback_rate;
  }
  return (await resolveStoredReadingPace(settings, core)).audio_playback_rate;
}

/**
 * Apply accent color CSS vars based on the preset index.
 *
 * next-themes uses `attribute="class"` → `.dark` is toggled on <html>.
 * We detect the current mode and apply the matching color variant.
 *
 * Reader highlight colors follow the chosen accent as a paired semantic token.
 * The default Clover preset uses the exact brand highlight from DESIGN.md;
 * dark mode derives a quieter surface from the preset's dark accent.
 */
export function applyAccentColor(presetIndex: number) {
  const preset = ACCENT_PRESETS[presetIndex];
  if (!preset) return;

  const root = document.documentElement;
  const isDark = root.classList.contains("dark");
  const color = isDark ? preset.dark : preset.light;
  const highlight = isDark
    ? `color-mix(in srgb, ${preset.dark} 42%, var(--surface))`
    : preset.lightHighlight;

  root.style.setProperty("--accent", color);
  root.style.setProperty("--accent-highlight", highlight);
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
