# Readio Design

## Clover Green / 三叶草绿

Readio uses **Clover Green** as its core light-theme palette. The visual direction is quiet, warm, and reading-first: green carries brand/action emphasis, while the reading surfaces stay close to paper rather than pure white.

| Token | Hex | Usage |
| --- | --- | --- |
| `Primary` | `#397548` | Brand color, primary actions, active icons and high-emphasis accents |
| `Highlight` | `#BFD9C4` | Reading highlights and emphasized passages |
| `Canvas` | `#FCFCF8` | Main reading surface and page background |
| `Sidebar` | `#F7F7F2` | Navigation/sidebar background |
| `Selected` | `#E0EBE1` | Selected rows, active items and low-emphasis state fills |
| `Ink` | `#292929` | Primary text and icons |

### Canonical frontend tokens

```css
:root {
  --readio-primary: #397548;
  --readio-highlight: #BFD9C4;
  --readio-canvas: #FCFCF8;
  --readio-sidebar: #F7F7F2;
  --readio-selected: #E0EBE1;
  --readio-ink: #292929;
}
```

The runtime source of truth for the web app is `apps/web/app/globals.css`. Components should consume these tokens, or the semantic tokens mapped from them, instead of hard-coding the palette values.

### Usage principles

- Keep `Canvas` dominant in the reading area; avoid large blocks of saturated green behind long-form text.
- Use `Primary` for actions and identity, not as a general-purpose surface color.
- Use `Highlight` for passages and transient reading emphasis.
- Use `Selected` for navigation and selection state fills.
- Use `Ink` for primary copy and iconography on `Canvas`, `Sidebar`, `Selected`, and `Highlight` surfaces.
- The existing dark theme remains a separate semantic theme until a dedicated dark Clover Green palette is defined.
