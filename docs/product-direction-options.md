# Readio product direction options

Status: Probe A selected by the user on 2026-08-05  
Inputs: current Web token/layout work, the TUI product principles, `docs/image*.png`, Dashboard/Reader PRDs, and the cross-platform parity audit

## Decisions already implied by the product request

- Web = Next.js presentation + Rust Core through WASM + FastAPI as the real backend.
- iPhone/iPad = SwiftUI presentation + Rust Core through Swift FFI + FastAPI.
- Desktop = native macOS SwiftUI application, with AppKit used selectively for mature macOS behaviors such as menus, commands, windows, file panels, focus, and media integration. It is not Electron, Tauri, Catalyst, or a rewritten AppKit-only product.
- Python remains the server TTS runtime. Web reading never depends on TTS. A browser-local WASM/WebGPU TTS engine is an optional privacy/offline capability, not an architectural prerequisite.
- TUI parity is measured by user outcome and invariants, not by copying slash commands or terminal appearance.

## Probe A — Quiet precision (recommended)

**Weighting:** Linear information precision + Apple Books reading comfort + Raycast shortcut quality.

- Library defaults to a compact list because title, type, source, progress, updated time, sync state, and actions compare better in rows than in cover cards.
- Home uses a small number of cover-forward shelves only where visual recognition helps: Continue, Recently added, and Samples.
- Reader removes the persistent product sidebar. It uses a narrow centered measure, optional TOC/marks/search inspector, a quiet top bar, and one bottom transport surface.
- Restrained neutral/sage palette; book art and semantic states provide most color. No gradient decoration, glassmorphism, or dashboard card field.
- Desktop exposes the same action graph through menu commands, toolbar, context menus, and command search. Web uses a command palette only as an accelerator, never as the sole path.
- Density is deliberate: compact management surfaces, comfortable reading surfaces, generous empty states.

**Best for:** a durable daily reading tool that scales from mouse to keyboard and from phone to a large desktop library.

**Risk:** less immediately “bookstore-like” than a full cover grid. Home must supply enough warmth that the product does not feel like file management.

## Probe B — Personal bookroom

**Weighting:** Apple Books warmth and cover recognition first.

- Library and Home are both cover-forward, with shelves, large artwork, editorial typography, and reading-status badges.
- List mode exists but is secondary. Search results are still rows.
- Reader is warm and literary, with more visible typography/theme controls and less operational density.
- Desktop leans into window chrome, sidebar collections, and cover browsing.

**Best for:** leisure reading and small-to-medium personal libraries with good cover metadata.

**Risk:** weak for imported web/TXT content, large libraries, duplicate diagnosis, sync state, and the high-density operational work implied by TUI parity.

## Probe C — Command reader

**Weighting:** Raycast/Linear speed and agent-native semantics first.

- Nearly every screen is list-first and keyboard-first. A global command palette exposes import, open, find, bookmark, mode, effort, voice, and navigation.
- Progress, parser state, TTS provider attempts, offline state, and sync state are highly inspectable.
- Reader retains a visible utility rail for plan/search/marks/voice and uses denser controls.
- Desktop feels exceptionally fast with shortcuts and menu commands; Web closely follows.

**Best for:** expert readers, research workflows, and very large libraries.

**Risk:** can turn reading into operating a tool. Touch and casual use need a second interaction grammar, increasing complexity and risking UI divergence.

## Recommended synthesis

Choose **Probe A** as the product system, then borrow two bounded ideas:

- From B: cover-forward Home shelves and reader typography warmth.
- From C: one global action graph, command palette/menu accelerators, and inspectable diagnostics.

Do not average the three visual systems. The shell, spacing, type scale, color roles, controls, and reader geometry remain Probe A. The borrowed ideas are capabilities placed into its slots.

## Layout grammar for the recommended direction

| Frame | Primary alignment | Persistent regions | Scrolling authority | Typical density |
| --- | --- | --- | --- | --- |
| Home/overview | left and right content edges | desktop sidebar, sticky page header | main content | comfortable |
| Library/search/list | column edges | desktop sidebar, sticky query/filter/header | result list | compact |
| Item detail | metadata column edges; content x-center only when reading preview | desktop sidebar, sticky actions | main content | comfortable |
| Reader | content x-center; top/bottom bars align to viewport edges | fixed top bar, optional inspector, floating/fixed player | document viewport | comfortable |
| Settings/workspace | form label/control columns | sheet on touch/Web; window or inspector on macOS | form content | compact |

The layout token chain is `primitive → semantic spacing → frame geometry → slot geometry → component geometry`. Components may not introduce a raw spacing/size value when an existing tier can express it. Exceptions must be named tokens with a written physical or platform reason.

## Global surface grammar

| Business meaning | Surface | Persistence |
| --- | --- | --- |
| Successful background mutation | toast / native transient notice | transient, logged in local operation history when useful |
| Recoverable failure tied to one task | inline feedback beside that task | until recovered/dismissed |
| Playback/TTS condition affecting the active reader | reader status banner | until resolved or mode changed |
| Destructive or consequential confirmation | alert/dialog | blocks the action only |
| Multi-step import, voice, or settings work | sheet on touch/Web; window/sheet/inspector on macOS | until completed/cancelled |
| Offline/sync state affecting many actions | shell banner or status item | while condition holds |

## Selected direction

The selected product system is Probe A with the bounded B/C additions described above. The final design brief freezes scope, information architecture, platform adaptations, state inventory, token contracts, and acceptance plan; TDD implementation begins after that brief receives explicit confirmation.
