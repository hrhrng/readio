/**
 * Brand accent presets consumed by both the settings UI and the DOM token
 * applier. Keeping them outside either component prevents visual state drift.
 */
export const READIO_COLOR_TOKENS = {
  primary: "#397548",
  highlight: "#BFD9C4",
  canvas: "#FCFCF8",
  sidebar: "#F7F7F2",
  selected: "#E0EBE1",
  ink: "#292929",
} as const;

export const ACCENT_PRESETS = [
  {
    name: "Clover",
    light: READIO_COLOR_TOKENS.primary,
    dark: "#8AD6A9",
    lightHighlight: READIO_COLOR_TOKENS.highlight,
  },
  {
    name: "Blue",
    light: "#356DA8",
    dark: "#86C5FF",
    lightHighlight: "#B0C6DA",
  },
  {
    name: "Amber",
    light: "#8A5B16",
    dark: "#EFC477",
    lightHighlight: "#D1BFA2",
  },
  {
    name: "Rose",
    light: "#A2445C",
    dark: "#F2A0B1",
    lightHighlight: "#DAB6BD",
  },
  {
    name: "Violet",
    light: "#6854A3",
    dark: "#B9A8F0",
    lightHighlight: "#C4BCD8",
  },
] as const
