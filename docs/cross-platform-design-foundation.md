# Readio cross-platform design foundation

Status: normative annex for the selected direction; product implementation remains gated on final brief confirmation  
Selected direction: Quiet Precision (Probe A), confirmed by the user on 2026-08-05  
Companion documents: `cross-platform-parity.md`, `product-direction-options.md`, `rust-core-architecture.md`

## Product promise

Read anything, continue at the exact place on any device, and choose whether the text waits for the reader, advances on its own, or follows a trustworthy spoken clock.

The product is not primarily a TTS dashboard. Reading remains complete when speech is unavailable. TTS is one first-class reading mode, not the dependency underneath the library or reader.

## Users and critical journeys

### Daily reader

Imports EPUB/PDF/web/text, resumes the right place, reads manually or listens, bookmarks a passage, and expects the next device to agree.

Critical journey: import → validate/parse → open → resume → change mode/pace → leave → resume elsewhere.

### Research reader

Uses a large mixed-format library, searches exact occurrences, walks hits, leaves meaningful bookmarks, and needs keyboard efficiency without losing a touch-first path.

Critical journey: filter/open → full-book search → next/previous hit → bookmark with note → return from bookmark after the document parser changes.

### Offline/private reader

Needs previously opened content and position without a network. Speech may use a server, platform voice, or an explicitly installed local engine, but the reading experience cannot be held hostage by it.

Critical journey: pin/download → disconnect → read/change position/bookmark → reconnect → understand and resolve any divergent position.

## Outcomes that exceed the TUI

1. **Exact cross-device continuity:** offset-based position, bookmarks, reading mode, effort, and preferences sync through a versioned conflict-safe contract.
2. **Offline truthfulness:** cached availability is visible before disconnect; queued changes and conflicts are explicit; no optimistic “available offline” fiction.
3. **Native action graph:** every capability has one semantic action ID, then maps to visible controls, Web shortcuts, macOS menus/commands, iPad keyboard commands, and iPhone touch actions.
4. **Inspectable playback:** the active mode, provider attempt, buffer state, actual media cursor, sleep deadline, and recovery action remain coherent without exposing implementation noise by default.

## Information architecture

The domain hierarchy is shared. Navigation presentation adapts by platform.

```text
Readio
├── Home
│   ├── Continue
│   ├── Recently added
│   └── Samples
├── Library
│   ├── All
│   ├── Reading / Want to read / Finished
│   ├── By format
│   └── Collections
├── Search
│   ├── Library search
│   └── Current-book search (inside Reader)
├── Reader
│   ├── Document
│   ├── Contents / plan
│   ├── Search results
│   ├── Bookmarks
│   └── Playback / mode / pace
├── Import
└── Settings
    ├── Reading
    ├── Voice and models
    ├── Audio output
    ├── Offline storage
    ├── Appearance and language
    └── Diagnostics
```

### Web

- Stable desktop shell on Home, Library, Search, Import, and Settings.
- Reader is a separate immersive frame with no product sidebar.
- At narrow widths the sidebar becomes a bottom or sheet navigation; it never squeezes the reader measure.
- URL structure remains deep-linkable. Reader subspaces use URL/query state when sharing or restoration benefits; transient popovers do not.

### iPhone

- Four primary tabs: Home, Library, Import, Settings. Search is a first-class Library destination and system-search pattern, not a fifth persistent tab.
- Reader is pushed full-screen. Contents/search/bookmarks share one inspector sheet with detents.
- Primary playback and mode controls remain thumb reachable; advanced voice/model work stays in a sheet.

### iPad

- NavigationSplitView for top-level navigation.
- Reader uses the document plus one optional inspector column. It does not stack independent TOC/search/bookmark sheets.
- Hardware keyboard commands use the shared action graph.

### macOS

- SwiftUI scene/window composition with AppKit adapters where macOS behavior is otherwise compromised: menu validation, command routing, focus, file panels, window restoration, audio-device observation, and rich table behavior.
- Sidebar + content/inspector split view; Library defaults to a real sortable list/table and supports a cover view.
- Reader can open in the main window or a dedicated restored window without duplicating playback authority.
- Menu commands and toolbar controls are projections of the shared action graph.

## Page frames and slots

### Overview frame

Slots: shell navigation, sticky page header, primary action, Continue shelf, Recent shelf, sample/recovery region, global status.

Alignment authority: the page content left/right edges. Shelf cards align to these edges; text inside cards aligns to each card, not to the page center.

### List frame

Slots: shell navigation, sticky title/actions, query, filter/sort row, column header, rows, pagination/continuation, local feedback, empty/error state.

Alignment authority: column edges. Header cells, skeletons, data cells, inline errors, and row actions consume the same column template tokens.

### Detail frame

Slots: identity/cover, metadata, status/actions, format/import facts, progress, collections, destructive actions.

Alignment authority: metadata edges. A preview can use the reader measure, but the page itself is not centered like a modal.

### Reader frame

Slots: fixed top bar, optional inspector, centered document measure, active-range overlay, persistent playback/mode surface, reader banner, transient selection tools.

Alignment authorities:

- document: x-center on the available viewport after inspector width;
- top/player bars: viewport left/right or safe-area edges;
- active sentence: source text flow, never a separate approximated overlay position;
- loading/empty fatal state: x-center and y-center inside the document viewport;
- recoverable banners: top edge of the document viewport, above content and below the top bar.

### Settings/workspace frame

Slots: section navigation, title/description, labeled controls, preview/test surface, persistence feedback, destructive/reset actions.

Alignment authority: label and control columns. On narrow screens the columns collapse to stacked rows from one breakpoint token.

## State inventory

Every state must have a deterministic slot arrangement, focus target, accessible announcement, allowed actions, and token recipe. “Same screen plus a spinner somewhere” is not a state definition.

### Shell and session

| State | Required visible truth | Primary recovery/action |
| --- | --- | --- |
| bootstrapping | application is loading local/session state | none; skeleton uses final geometry |
| authenticated online | sync is current or actively settling | normal actions |
| authenticated offline | offline status and cached scope | inspect offline items / retry |
| degraded backend | which capabilities remain usable | retry; diagnostics |
| authentication expired | local unsynced work preservation | sign in without discarding queue |

### Library

| State | Layout rule | Action rule |
| --- | --- | --- |
| initial loading | sticky header + column-matched skeleton rows | filters disabled only when their data is unavailable |
| content | stable columns, no row reflow on hover | open row; secondary actions in menu/context menu |
| refreshing | retain rows and selection; show local progress | cancellation when transport supports it |
| empty library | y-centered within content region, not viewport including header | Import and Open sample |
| filtered empty | retain filters/query and column header | Clear relevant filters |
| page/loading more | append row skeletons using column template | retain scroll/focus anchor |
| recoverable error | retain stale rows where safe; inline/banner error | Retry |
| destructive pending | selected row remains visually identified behind dialog | Cancel is initial safe action |

### Import

| State | Required facts | Actions |
| --- | --- | --- |
| source choice | file, URL, paste text; ownership mode where meaningful | choose source |
| validating | filename/URL, size/type checks | cancel |
| duplicate found | existing item and preserved state | open existing; change ownership mode if valid |
| copying/linking/moving/uploading | exact ownership consequence and byte progress when knowable | cancel when safe |
| parsing | format and parser stage, indeterminate only when truly unknown | background/dismiss if durable |
| completed | item identity and any warnings | open; import another |
| partial/warning | readable result plus lost/unsupported features | open; inspect details |
| failed | stable reason code plus human recovery | retry/change source |
| cancelled | whether temporary/owned bytes were cleaned | restart |

### Search

| State | Required facts | Actions |
| --- | --- | --- |
| idle | scope and example/shortcut | focus query |
| querying | query remains editable | cancel/replace query |
| results | total occurrences, paragraphs/documents, current hit | open/walk hits |
| no results | exact query and scope | edit/clear |
| stale after document change | revision mismatch | rerun on new revision |
| failed | search remains distinct from no results | retry |

### Reader and playback

Reader state is the product of independent axes; components render the valid combination rather than creating ad-hoc booleans.

```text
document: unloaded | loading | ready | unreadable | revision_changed
connectivity: online | offline_cached | offline_missing | backend_degraded
mode: manual | auto | read_aloud
transport: idle | fetching | buffering | playing | paused | seeking | completed | failed
position: exact | restoring | conflicted
inspector: closed | contents | search | bookmarks
sleep: unset | active | elapsed
```

Invalid combinations are rejected in Rust, for example `read_aloud + playing` without a ready audio generation, or `sleep=elapsed` while transport remains playing.

Reader recovery rules:

- document failure does not masquerade as TTS failure;
- TTS failure preserves `read_aloud` mode and exact position;
- offline with cached document allows Manual/Auto; Read-aloud depends on cached audio or an explicit local engine;
- a stale audio/search result tagged with a prior document or playback generation is ignored;
- fatal content states occupy the document viewport; recoverable playback failures use the reader banner/player slot and never replace the book.

### Bookmarks

`idle`, `creating`, `created`, `renaming`, `deleting`, `empty`, `content`, `stale-revision-resolved`, `conflicted`, `failed`. The list keeps stable ordering by source offset with an optional creation-time alternate sort.

### Voice/model workspace

`loading catalog`, `available remote`, `available local`, `not installed`, `downloading`, `verifying`, `installed`, `corrupt`, `incompatible`, `testing`, `active global`, `active for book`, `failed`. Download state and selection state are intentionally separate.

## Shared action graph

Actions use stable semantic IDs and declare availability from product state. UI surfaces never infer availability independently.

```text
library.import
library.open
library.forget
library.search
reader.toggle_playback
reader.set_mode.manual
reader.set_mode.auto
reader.set_mode.read_aloud
reader.next_sentence
reader.previous_sentence
reader.next_chapter
reader.previous_chapter
reader.open_contents
reader.open_search
reader.open_bookmarks
reader.add_bookmark
reader.cycle_effort
reader.set_sleep_deadline
reader.cancel_sleep
voice.open_workspace
diagnostics.open
```

Each action definition owns:

- localized label and accessibility phrase;
- availability/disabled reason;
- destructive/reversible metadata;
- default Web/Apple key equivalent where appropriate;
- event sent to Rust or platform effect invoked;
- analytics/diagnostic event name without content payload.

## Durable cross-platform contracts

The concrete wire representation is versioned JSON at Web/FFI boundaries and versioned HTTP schemas at FastAPI. Integers use non-negative 64-bit ranges even where TypeScript needs string encoding beyond safe integer limits.

### Document identity and revision

```text
ContentIdentity {
  algorithm: "blake3-prefix-v1"
  byte_length: u64
  digest: string
}

DocumentRevision {
  content_identity: ContentIdentity
  ir_version: u32
  parser_version: string
  revision_digest: string
  char_count: u64
}
```

Content identity deduplicates source bytes. Document revision identifies a particular semantic projection and invalidates stale block/sentence/search projections without invalidating offset-based user intent.

### Position

```text
ReadingPosition {
  item_id: string
  document_revision: string
  char_offset: u64
  affinity: "forward" | "backward"
  observed_at_unix_ms: i64
  session_id: string
  mutation_id: string
  base_version: u64
}

PositionProjection {
  block_id: string?
  chapter_id: string?
  sentence_index: u32?
  progress_basis_points: u16
}
```

Only `char_offset` plus affinity is authoritative. Projection fields are responses/cache and are never accepted as a reason to overwrite the offset.

### Position synchronization

- FastAPI applies a mutation only when `base_version` matches the current server version, then returns the next version.
- Idempotency is by `mutation_id`.
- Offline mutations retain their base version and order per local session.
- If the remote position changed from the same base, neither offset is discarded. The server returns both candidates and a conflict ID.
- A conflict may auto-resolve only when mutations are on one causal lineage. Divergent sessions require an explicit resume choice, with both chapter/snippet projections shown. Choosing produces a new mutation based on the current version.
- “Take the larger percentage” is forbidden because rereading backward is intentional user behavior.

### Bookmark

```text
Bookmark {
  id: string
  item_id: string
  char_offset: u64
  affinity: "forward" | "backward"
  note: string
  created_at_unix_ms: i64
  updated_at_unix_ms: i64
  version: u64
}
```

One bookmark exists per item/offset. Creating again updates its note. Display chapter/snippet is resolved from the current DocumentRevision.

### Search

```text
SearchRequest {
  document_revision: string
  query: string
  cursor: string?
  limit: u16
}

SearchResultPage {
  folded_query: string
  total_occurrences: u64
  total_blocks: u64
  hits: SearchHit[]
  next_cursor: string?
}

SearchHit {
  hit_id: string
  char_range: [u64, u64]
  block_id: string
  snippet: string
  snippet_match_range: [u32, u32]
}
```

Rust owns case/full-width folding and original-offset mapping. Clients use the same Core implementation for an open cached document. FastAPI library-wide indexing is a separate scope and must return source ranges compatible with the same semantics.

### Reading preferences

```text
ReadingPreferences {
  mode: "manual" | "auto" | "read_aloud"
  base_text_chars_per_second: f32
  effort: "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
  effort_multipliers: map<effort, f32>
  sleep_deadline_unix_ms: i64?
}
```

Effort math lives in Rust. Platforms supply monotonic and wall-clock samples and execute the resulting reveal/playback-rate effects.

## Rust responsibility map

| Core module | Owns | Emits/returns |
| --- | --- | --- |
| `documents` | versioned IR, safe parsing budgets, stable block/media identity | DocumentRevision + blocks/media |
| `identity` | content fingerprints and ownership-independent identity | ContentIdentity |
| `position` | offset clamping, projection, restore across revisions | exact position + projection |
| `search` | fold/index/ranges/cursors over Document IR | SearchResultPage |
| `bookmarks` | validation, default label, same-offset rename semantics | bookmark transitions |
| `reading` | Manual/Auto/Read-aloud mode and continuation policy | reading effects |
| `effort` | six-level rate calculations | text/audio rate |
| `playback` | generations, media-clock decisions, seek/prefetch/sleep | platform effects |
| `sync` | mutation validation and conflict model shared with server tests | apply/conflict result |
| `actions` | state-derived availability/disabled reasons | action snapshot |

Rust does not own fetch, SQLite, DOM/SwiftUI, file pickers, audio decoding/output, platform timers, notifications, credentials, or Web/OS media sessions.

## Backend responsibility map

- authentication/authorization and per-user isolation;
- library item/source ownership, file lifecycle, collections/status;
- content storage, import/download jobs, server-side format extraction where appropriate;
- canonical document revisions and versioned IR delivery/cache;
- position/bookmark/preference mutation versions and conflict records;
- library-wide full-text index;
- TTS provider registry, explicit attempt/fallback trace, jobs, caching, model metadata, and server secrets;
- offline sync batch endpoint with idempotency and partial-result reporting;
- redacted operation diagnostics.

Python remains correct for TTS orchestration. Reusable semantics may be tested against Rust fixtures or exposed through a small native binding later, but FastAPI is not replaced by WASM.

## Optional Web-local TTS research gate

Do not put a speculative browser model on the critical path. A candidate must pass all gates before becoming an offered provider:

- license permits distribution and model download;
- quality is acceptable for at least one declared language/voice;
- real-time factor, first-audio latency, memory, download size, and battery/thermal cost are measured on a representative low-end device;
- cancellation, streaming/chunking, cache keys, and deterministic failure are supported;
- WebGPU/WASM capability detection is explicit and failure returns to the selected mode without silent provider substitution;
- model assets are opt-in and removable from Offline storage settings.

## Token computation and alignment rules

### Token tiers

```text
primitive
  space, size, radius, type, duration, z, color
      ↓
semantic
  surface, text, border, focus, danger, success, readable measure
      ↓
frame
  shell, overview, list, detail, reader, settings geometry
      ↓
slot
  header, query, columns, inspector, document, player, feedback geometry
      ↓
component
  control, row, cover, toast, sheet, banner, skeleton geometry
```

Rules:

1. Primitive spacing uses a 4pt base. Half-step physical corrections are allowed only for 1px borders, optical icon corrections, and platform safe-area/system metrics.
2. Every physical correction is a named token with a reason. Raw component literals are rejected by lint/review.
3. Component height comes from content line-height + block padding + borders, not an unrelated magic height, except platform minimum hit targets.
4. Icon slots have a fixed token; glyphs center inside the slot. Text aligns by baseline, never by the glyph's bounding box.
5. A repeated row's skeleton, empty placeholder, error, selection, hover action, and data state use the same grid/slot tokens.
6. Sticky/fixed surfaces reserve their geometry or apply the corresponding scroll padding. Content may not disappear underneath them.
7. Reader measure, inspector width, and player width are independent tokens calculated against available viewport/safe area.

### Alignment selection

| Situation | Alignment authority |
| --- | --- |
| comparable records | left/right column edges and text baselines |
| long-form document | x-center within available reader viewport |
| fatal empty/loading state | x-center + y-center inside its owning slot |
| toolbar/control row | y-center control slots; labels remain baseline-aligned internally |
| icon + label | fixed icon slot + label baseline |
| cover + metadata row | cover edges + metadata text column |
| toast/banner | viewport/shell edge placement; internal icon/text/action grid |
| sheet/dialog | x/y center only on large viewports; bottom/safe-area edge on touch when appropriate |

### Cross-platform token source

- A platform-neutral token manifest should be the source of values and semantic names.
- Build generation produces CSS custom properties/TypeScript types and Swift constants. Generated files are verified for drift.
- Platform-only tokens are namespaced and must refer to a system constraint, such as Apple minimum hit target or safe-area behavior.
- Rust consumes no visual tokens. It may expose semantic state and action availability that selects a component recipe.

## Global surface policy

| Event class | Surface | Accessibility/focus contract |
| --- | --- | --- |
| transient successful mutation | toast | polite announcement; never moves focus |
| failed mutation with task-local recovery | inline feedback | assertive only when blocking; focus remains at initiating task |
| active reader degradation | status banner/player state | announced once; recovery action keyboard reachable |
| destructive consequence | alert/dialog | focus trapped; safest action first; returns focus to initiator |
| multi-step workspace | sheet/window/inspector | initial focus on title or first invalid field; state survives temporary backgrounding |
| global offline/sync/auth condition | shell banner/status item | persistent, non-repeating announcement; opens details |

Business code emits semantic events such as `library.import_failed` or `playback.provider_unavailable`; platform presentation maps them to the policy. It does not call arbitrary toast/sheet methods directly.

## Accessibility and localization gates

- Every action is reachable without pointer/gesture-only knowledge.
- Focus order follows visual/source order; sticky surfaces do not create focus loops.
- Reader text supports 200% Web zoom and Apple accessibility Dynamic Type without horizontal text scrolling.
- Sentence/word highlighting has a non-color state and meets contrast without replacing the user's text selection semantics.
- Motion is functional and honors reduced motion; auto-scroll uses an immediate mode when reduced motion is active.
- English and Chinese cover UI, errors, counts, shortcuts, accessibility labels, and model/language descriptions.
- Imported book language does not silently switch interface language or voice configuration.

## Offline contract

An item exposes one of: `not_available`, `metadata_only`, `content_ready`, `content_and_audio_ready`, `updating`, `failed`.

- Pinning downloads source/IR/media under a declared budget and verifies the revision.
- Cached audio is optional and keyed by text identity, voice/model, canonical rate, and provider version.
- Local mutations are an inspectable queue with mutation IDs.
- Sign-out or cache removal names whether unsynced mutations exist.
- Storage pressure and eviction never remove user-owned source files or unsynced state.
- Service Worker/Web storage and Apple file/database implementations conform to the same fixture suite at the adapter boundary.

## Quality budgets

Budgets are acceptance targets, not fabricated telemetry:

- warm navigation response: visual acknowledgment in one frame;
- cached reader open: useful text visible within 500ms on representative devices;
- full-book search: first page within 150ms for a 1M-character cached document on reference hardware;
- cached sentence playback: audible start within 200ms;
- uncached TTS: show real queued/running state immediately; no fake percent;
- scroll/word highlight: no sustained frame drops under representative long documents;
- offline mutation replay: idempotent under retry and process interruption;
- accessibility: zero critical automated violations plus manual critical-journey verification.

Hardware/browser reference profiles must be recorded with measurements before a budget is claimed green.

## TDD delivery slices after brief approval

Each slice begins with failing Rust/backend/adapter contract tests, then the minimal behavior, then platform surfaces, then cross-platform fixtures.

1. **Exact position and revision:** Rust types/projection/restore → FastAPI versioned persistence → Web/Apple adapters → resume UI.
2. **Bookmarks:** Rust semantics → backend CRUD/versioning → reader inspector and shortcuts/menu commands.
3. **Open-book search:** Rust fold/index/ranges → WASM/FFI → Web/Apple search inspector and hit navigation.
4. **Reading modes and effort:** Rust reading reducer/rate math → persistence → platform effect executors and mode/pace controls.
5. **Playback takeover:** shadow-compare current Web/Swift engines against Rust reducer → switch authority → delete duplicate state machines.
6. **Sleep and recovery:** deadline effects, background/resume tests, reader banner, explicit provider-attempt trace.
7. **Document expansion:** EPUB/PDF/text/Markdown IR fixtures, stable media/anchor/emphasis behavior, remove duplicate platform parsing.
8. **Offline/sync:** adapter stores, mutation queue, conflict UI, storage settings.
9. **Native macOS:** target/FFI packaging, library/reader windows, action commands, file/audio integration.
10. **Hardening:** localization, accessibility, diagnostics, performance, E2E, matrix audit.

## Design-brief confirmation boundary

Before product code changes, the final brief must confirm:

1. Probe A recommended synthesis, Probe B, or Probe C.
2. Native macOS SwiftUI with selective AppKit adapters.
3. Result-parity rather than terminal-command parity.
4. The four beyond-TUI outcomes and the delivery order above.
5. That AI chat/summary/podcast generation remain outside the parity critical path until the reader foundation is green.

After confirmation, this foundation becomes normative unless an implementation discovery is brought back as an explicit design change.
