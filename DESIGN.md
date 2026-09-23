---
version: "alpha"
name: Quiet Library
description: A calm, cover-led reading workspace with Apple Books clarity and Readio precision.
colors:
  primary: "#397548"
  on-primary: "#FFFFFF"
  highlight: "#BFD9C4"
  canvas: "#FCFCF8"
  sidebar: "#F7F7F2"
  raised: "#FFFFFF"
  ink: "#292929"
  secondary-ink: "#5F655F"
  hover: "#E9EDE8"
  selected: "#E0EBE1"
  border: "#DDE3DD"
  danger: "#B42318"
typography:
  page-title:
    fontFamily: Inter
    fontSize: 2.25rem
    fontWeight: 750
    lineHeight: 2.8125rem
    letterSpacing: -0.04em
  section-title:
    fontFamily: Inter
    fontSize: 1.25rem
    fontWeight: 700
    lineHeight: 1.5625rem
    letterSpacing: -0.015em
  body-md:
    fontFamily: Inter
    fontSize: 0.875rem
    fontWeight: 400
    lineHeight: 1.3125rem
  label:
    fontFamily: Inter
    fontSize: 0.875rem
    fontWeight: 600
    lineHeight: 1.125rem
  metadata:
    fontFamily: Inter
    fontSize: 0.75rem
    fontWeight: 400
    lineHeight: 1.125rem
  reading:
    fontFamily: Playfair Display
    fontSize: 1rem
    fontWeight: 400
    lineHeight: 1.75rem
rounded:
  sm: 6px
  md: 8px
  lg: 12px
  pill: 999px
spacing:
  xs: 4px
  sm: 8px
  md: 16px
  lg: 24px
  xl: 32px
  2xl: 48px
control-sizes:
  compact: 32px
  standard: 40px
  prominent: 44px
components:
  page-canvas:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.ink}"
    typography: "{typography.body-md}"
  app-sidebar:
    backgroundColor: "{colors.sidebar}"
    textColor: "{colors.ink}"
    width: 256px
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    typography: "{typography.label}"
    rounded: "{rounded.md}"
    height: "{control-sizes.standard}"
    padding: 0 16px
  button-quiet:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.secondary-ink}"
    typography: "{typography.label}"
    rounded: "{rounded.md}"
    height: "{control-sizes.standard}"
  button-quiet-hover:
    backgroundColor: "{colors.hover}"
    textColor: "{colors.ink}"
  navigation-active:
    backgroundColor: "{colors.selected}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    height: "{control-sizes.standard}"
  input-standard:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink}"
    typography: "{typography.body-md}"
    rounded: "{rounded.md}"
    height: "{control-sizes.standard}"
  menu-trigger:
    backgroundColor: transparent
    textColor: "{colors.secondary-ink}"
    typography: "{typography.label}"
    rounded: "{rounded.md}"
    height: "{control-sizes.standard}"
  popover:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    padding: 4px
  book-cover:
    backgroundColor: "{colors.selected}"
    rounded: "{rounded.sm}"
    width: 200px
  book-metadata:
    textColor: "{colors.secondary-ink}"
    typography: "{typography.metadata}"
  book-list-row:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.ink}"
    height: 128px
  destructive-menu-item:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.danger}"
    typography: "{typography.body-md}"
  divider:
    backgroundColor: "{colors.border}"
    height: 1px
---

## Overview

Readio is a calm, local-first reading workspace. Collection surfaces borrow the clarity of Apple Books: covers lead, navigation is quiet, and controls recede until needed. Readio remains recognizable through the `❯_ readio` mark, a restrained clover-green accent, and precise reader/TUI behavior.

This is inspiration, not imitation. Do not use Apple trademarks, proprietary icons, window chrome, sample covers, or copied assets.

The design follows six principles:

1. **The book is the interface.** Cover art and reading state outrank metadata and chrome.
2. **Quiet precision.** Use whitespace, alignment, typography, and one clover-green accent. Avoid decorative cards and competing colors.
3. **One state, multiple views.** Grid and list render the same query, sort, progress, and mutation state.
4. **Progressive controls.** Navigation stays visible; view, sort, and item actions live in anchored menus.
5. **Stable geometry.** Loading, ready, empty, error, grid, and list states keep the header and collection origin fixed.
6. **Local first through behavior.** The interface should feel immediate without filling the UI with sync terminology.

The brand lockup is `❯_ readio`. The prompt glyph uses primary clover green, while the lowercase wordmark uses ink. Never place it inside a colored tile or add a second pictorial mark.

## Colors

The light palette uses Granola-like warm whites without changing their temperature between accent choices. Canvas is `#FCFCF8`, Sidebar is `#F7F7F2`, and Raised white is reserved for overlays. Ink `#292929` avoids absolute black.

Primary clover green `#397548` is the only brand hue. Highlight `#BFD9C4` supports progress tracks and word-level emphasis; Selected `#E0EBE1` marks current navigation and chosen rows. Danger and other semantic colors must always be reinforced by text or iconography.

Dark mode maps the same roles to Canvas `#1C1F1D`, Sidebar `#151816`, Raised `#252925`, Ink `#F4F7F4`, Secondary Ink `#C0C8C1`, and Primary `#8AD6A9`. These are implementation aliases until the format supports mode-specific token groups.

## Typography

Inter owns navigation, labels, metadata, and page hierarchy. Page titles are large but compact, matching the confidence of a native library. Book titles use the Label scale with one-line truncation on shelves and two-line truncation in list rows.

Playfair Display is restricted to fallback reading prose. It never appears in navigation, menus, or book metadata. Imported documents may preserve their own typography when safe and readable.

Avoid all-caps except short file-type labels. Use weight for hierarchy before adding size. Metadata remains quiet but must still meet WCAG AA contrast.

## Layout

The viewport is locked; the main pane is the only vertical scroller. Desktop uses a fixed 256 px sidebar. Mobile replaces it with a five-item bottom navigation. The page header is sticky and keeps the title, description, and trailing action on a stable baseline.

Collection pages default to a cover shelf. Desktop columns auto-fill between 172 and 200 px with 32 px horizontal and 48 px vertical rhythm. Mobile uses two equal columns with a 16 px page gutter. Covers use a 2:3 ratio.

List mode is a media list, not a data table. Each row contains a 2:3 cover thumbnail, title, source, reading state, and trailing action. Grid/list changes reuse the same data hook and sort state.

The right-side `…` menu owns view and sort controls. It remains at the same header coordinate across loading, ready, empty, and error states.

## Elevation & Depth

The application is primarily flat. Surface contrast and whitespace separate the sidebar, content, and rows.

Book covers are the exception: they use a restrained two-layer shadow to suggest a physical object. Hover may lift a cover by 2 px without scaling it. Popovers and dialogs use a tighter shadow plus an opaque Raised surface and hairline border. Do not add shadows to navigation rows or collection containers.

Motion uses 150 ms for hover/focus and 200 ms for menus and layout transitions. All motion derives from shared duration tokens and collapses under `prefers-reduced-motion`.

## Shapes

Controls use 8 px radii, overlays use 12 px, book covers use 6 px, and true status chips use the pill radius. Avoid mixing arbitrary radii.

Books never sit inside rounded card containers; the cover itself is the object. List rows use whitespace and a bottom hairline rather than outlined cards. Borders are structural hairlines, not decoration.

## Components

Tailwind CSS v4 and semantic CSS variables own visuals. Radix UI owns behavioral primitives: Dropdown Menu, Popover, Dialog, Alert Dialog, Tooltip, and Tabs. shadcn/ui may scaffold local Radix wrappers, but its neutral defaults are not a second theme; every wrapper consumes these tokens. Lucide is the only icon family.

The collection menu and every book action menu use Radix Dropdown Menu. The page menu groups Grid/List separately from sorting choices and uses radio semantics with a trailing check. Item menus place destructive actions last. Menus close on activation, outside click, and Escape, support typeahead and arrow navigation, collision-shift inside the viewport, and return focus to their trigger.

Primary buttons use clover-green fill and white text. Secondary buttons use Hover. Quiet and icon buttons are transparent at rest. The closed control scale is Compact `32 px`, Standard `40 px`, and Prominent `44 px`. Sidebar navigation rows, standard buttons, standard inputs, icon buttons, and menu triggers all consume the Standard token; this preserves the comfortable `40 px` highlight rhythm validated in the palette POC. Prominent is reserved for mobile touch targets and genuinely emphasized controls. Palette changes never alter geometry: the standard product button remains `40 px` high with an `8 px` radius and `14 px` label.

Feedback follows intent: reversible completion uses a toast with Undo; destructive decisions use an alert dialog; page failures stay inline; an empty library is an invitation with a concrete Import action; search/filter empty is a compact explanation.

Implementation sources:

- `apps/web/app/globals.css` — CSS variables and component classes.
- `apps/web/lib/design-tokens.ts` — brand accent presets.
- `apps/web/lib/ui-contracts.ts` — page geometry and feedback contracts.
- `taste-saas.contract.json` — product states, slots, and alignment groups.
- `apps/web/components/dropdown-menu.tsx` — Radix design-system wrapper.
- `apps/web/components/book-list.tsx` and `book-card.tsx` — collection renderers.

## Do's and Don'ts

Do:

- Let real book covers and reading state carry visual interest.
- Reuse existing tokens, Radix wrappers, feedback surfaces, and geometry slots.
- Keep grid/list as render functions over one collection state.
- Keep loading skeletons on the same tracks as loaded content.
- Reinforce color states with copy and semantics.
- Add a token only when it has at least two consumers.

Don't:

- Copy Apple logos, proprietary assets, traffic-light chrome, or sample covers.
- Add gradients, glass effects, oversized pills, or nested cards.
- Hand-roll dropdown keyboard/focus behavior.
- Use a data table for the book list alternative.
- Add a second icon family, accent hue, or visual theme from a component library.
- Hide important actions behind hover on touch devices.
