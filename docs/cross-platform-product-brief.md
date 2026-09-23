# Readio cross-platform product brief

Status: approved  
Direction: **Quiet Precision — Linear 精度 + Apple Books 阅读舒适度 + Raycast 快捷操作**  
Direction selected by the user: 2026-08-05  
Targets: Web, iPhone, iPad, native macOS, Cloudflare Worker, shared Rust Core; FastAPI is an optional TTS provider

Normative annexes:

- `../taste-saas.contract.json` — machine-checkable Web shell/layout/state/surface/token/alignment composition
- `cross-platform-parity.md` — capability baseline, evidence, and per-row acceptance contract
- `cross-platform-design-foundation.md` — exhaustive state inventory, schemas, layout/token rules, offline/accessibility/performance gates
- `rust-core-architecture.md` — current shared-core boundary and migration state
- `product-direction-options.md` — selected visual/product direction and rejected alternatives

If this brief conflicts with an older Dashboard/Reader MVP PRD, this brief wins. Older PRDs remain historical implementation context, not the target product contract.

## 1. Outcome

Readio becomes one coherent product across Web, iPhone, iPad, and macOS:

- users import EPUB, PDF, Markdown, HTML/web, and text into one library;
- reading always works without requiring TTS;
- Manual, Auto, and Read-aloud are persistent peer modes;
- position and bookmarks survive parser changes because character offset is authoritative;
- search reports real occurrences and lands on the source range;
- playback follows one authoritative media clock and never lets text outrun speech;
- Web and Apple share portable policy through Rust Core while Cloudflare Worker is the real backend and Python/FastAPI is only an optional TTS provider runtime;
- every platform reaches at least TUI user-outcome parity, then exceeds it through exact cross-device sync, offline truthfulness, native OS integration, and accessibility.

## 2. Success definition

The project is not complete when equivalent-looking screens exist. It is complete only when every applicable row in `cross-platform-parity.md` is **Complete** with current test/runtime evidence.

The four beyond-TUI outcomes are mandatory:

1. Exact, conflict-safe cross-device continuity.
2. Previously pinned/opened content remains readable offline with visible queued changes.
3. One semantic action graph maps to touch, visible controls, Web shortcuts, iPad keyboard commands, and macOS menus/commands.
4. Playback/provider/buffer/sleep state is truthful and recoverable without exposing noisy implementation detail by default.

## 3. Product principles

1. **Content first.** The library manages content; the reader reads it. TTS does not gate either.
2. **One authority per fact.** Offset owns position, media owns playback time, Rust reducers own portable transitions, the backend owns durable network state.
3. **No silent semantic fallback.** A Read-aloud failure stays in Read-aloud until retry or explicit mode/provider change.
4. **Real progress only.** Unknown duration, parsing progress, queue position, or sync state is shown as unknown/indeterminate rather than fabricated.
5. **Equivalent outcomes, native interaction.** Do not reproduce terminal commands on touch screens or Web; preserve the capability and invariant.
6. **Dense where comparing, calm where reading.** Lists are compact and aligned; long-form reading is spacious and centered.
7. **Errors preserve work.** Query, position, note, import source, mode, and unsynced mutations survive recoverable failure.

## 4. In scope

### TUI parity foundation

- Library browse/filter/sort/open/delete plus copy/link/move ownership semantics, content-identity deduplication, safe forget, and samples.
- EPUB/PDF/TXT/Markdown/HTML parsing with TOC anchors, non-linear spine handling, covers, headings, quotes, code, emphasis, and illustrations.
- Exact position restore, chapter/TOC navigation, full-book search, next/previous hit, bookmarks with notes.
- Manual/Auto/Read-aloud modes, pause/resume, sentence seek, sentence/word highlight, prefetch, retry, six-level effort, voice/model settings, audio-device allowlist.
- Sleep timer, honest progress/remaining time, English/Chinese UI, diagnostics, keyboard/touch/native commands.

### Cross-platform product completion

- Versioned position/bookmark/preference synchronization and explicit conflict resolution.
- Offline content, media cache policy, mutation queue, storage state, and deterministic reconnect.
- Web Media Session and Apple Now Playing/audio-session lifecycle.
- Native macOS application using SwiftUI plus selective AppKit adapters.
- Production accessibility, localization, performance budgets, support diagnostics, and end-to-end regression coverage.

### Existing valuable product capabilities retained

- Optional authentication for cloud sync and cloud TTS; anonymous local reading remains complete.
- Cloudflare D1/Queue/R2 TTS jobs and server-side secrets, with FastAPI providers available behind the Worker when useful.
- Web Home shelves, library management, settings, theme, and global feedback surfaces.
- iPhone/iPad adaptive navigation and native media controls.
- Chrome extension remains compatible with backend contracts; it is not the primary UI target of this brief.

## 5. Outside the parity critical path

These may follow after the foundation is green and must not delay or distort it:

- AI chat, summary, quiz, or podcast generation;
- social/community/discovery marketplace;
- DRM store integrations;
- collaborative annotations;
- browser-local neural TTS without passing the explicit research gate;
- pixel-identical UI across platforms.

## 6. Selected experience direction

### Visual character

- Precise, calm, content-led, restrained.
- Neutral/sage semantic palette; covers and status meaning provide color.
- No decorative gradients, glassmorphism, floating-card dashboard field, or oversized marketing typography inside the product.
- System sans for interface clarity; a highly readable serif/system reading stack may be selected in Reader.
- Motion explains state and placement, lasts briefly, and respects reduced motion.

### Density

- Home: comfortable, limited cover-forward shelves for Continue, Recent, and Samples.
- Library/Search: compact list-first comparison; cover view is optional.
- Reader: comfortable narrow measure and immersive frame.
- Settings/Voice: compact workspace with aligned labels and controls.

### Interaction character

- Every important action has a visible path.
- Keyboard/menu/command palette paths accelerate the same action rather than hiding exclusive functionality.
- Destructive operations are explicit; reversible operations prefer immediate result plus undo where technically sound.
- Cmd+K is mounted once at shell scope with fixed Recent / Actions / Navigate / Search sections. Shortcuts accelerate behavior but are not printed as persistent interface chrome.

## 7. Information architecture and platform adaptation

Shared destinations are Home, Library, Search, Reader, Import, and Settings.

| Platform | Top-level navigation | Reader adaptation | Advanced work |
| --- | --- | --- | --- |
| Web wide | stable sidebar + sticky page header | separate immersive frame; optional inspector; fixed/floating player | sheet/dialog; command palette accelerator |
| Web narrow | mobile navigation/sheet without squeezing content | full viewport; inspector sheet; safe-area player | full-height sheet |
| iPhone | Home, Library, Import, Settings tabs; search within Library | pushed full screen; combined contents/search/bookmarks sheet | native sheet/navigation stack |
| iPad | NavigationSplitView | document + one optional inspector column | inspector or sheet by task |
| macOS | sidebar/split view and native menus | document + inspector; restorable main/dedicated reader window | window, inspector, sheet, menu commands |

Search has two scopes:

- Library search finds documents/metadata and eventually indexed content across the library.
- Reader search runs exact full-book occurrence search on the current DocumentRevision.

## 8. Core page contracts

### Home

- Continue is authoritative by last reading activity and exact saved position.
- Recent and Samples are secondary shelves.
- Home is not an analytics/card dashboard and contains no fake recommendation or reading-goal telemetry.
- Loading skeletons retain final shelf geometry; offline/degraded status appears once at shell level.

### Library

- Default list columns: identity/cover, title/source, format, progress/status, updated/read time, sync/offline state, actions.
- Column header, rows, skeletons, errors, hover actions, and selection use the same grid tokens.
- Filters and query remain visible in empty/error states.
- Copy/link/move ownership is chosen or explained before import; forget names whether owned bytes will be removed.
- Duplicates open the existing item and preserve position instead of silently producing a second entry.
- The list is a real fixed-layout table with an explicit `colgroup`; the toolbar is its sibling and never joins the horizontal data scroller.

### Search

- Idle, querying, results, no-results, stale-revision, and failed are distinct.
- Full-book search returns real total occurrences, affected blocks, snippets, original ranges, and next/previous navigation.
- Case/full-width folding never corrupts the highlight range in source text.
- A failure never appears as zero results.

### Reader

- Product shell navigation disappears; the book becomes primary.
- Top bar, optional inspector, centered document, active text, reader status banner, and player/mode surface have stable slots.
- Contents, Search, and Bookmarks occupy one inspector region, not simultaneous competing panels.
- Position restore happens before automatic scroll/play; conflict requires choosing between preserved candidates.
- Manual loads/continues only on reader request; Auto advances at text pace; Read-aloud advances from real audio time.
- Pausing never changes mode. TTS failure never changes mode. Opening another book never changes mode.
- Search/bookmark/chapter navigation moves authoritative offset, invalidates stale playback generation, and then derives the new sentence/chapter projection.
- Sentence highlight and word emphasis remain readable under wrap, zoom, Dynamic Type, theme, and text selection.

### Settings and Voice workspace

- Reading contains mode default, base text speed, six effort multipliers, sleep defaults, typography, and language.
- Voice configuration explicitly separates global scope from per-book override.
- Model availability/download/verification is separate from active voice selection.
- Audio Output exposes the allowlist only where platform capability exists and explains Web limitations.
- Offline Storage shows item/model/cache state, size, pending mutations, and safe removal consequence.
- Diagnostics produces redacted operation/clock/provider traces without book text or secrets by default.

## 9. Global surfaces

Business code emits semantic events; platform UI maps them through this policy.

| Meaning | Surface | Lifetime |
| --- | --- | --- |
| transient successful mutation | toast/native transient notice | auto-dismiss; never moves focus |
| task-local recoverable failure | inline feedback | until recovered/dismissed |
| active reader/playback degradation | reader banner/player state | while unresolved |
| destructive or consequential choice | alert/dialog | blocks only that action |
| multi-step import/settings/voice work | sheet/window/inspector | until complete/cancelled |
| global offline/sync/auth condition | shell banner/status item | while condition holds |

No component may arbitrarily choose toast/sheet/alert from a catch block. It emits the typed business event and uses the mapping.

Reversible mutations complete immediately and use one toast with Undo. Background jobs update one stable progress-toast identity instead of emitting a stack. Growable modal/Cmd+K surfaces keep a fixed top anchor and grow downward; they do not vertically re-center. Overlay separation comes from opaque surface, border, shadow, and whitespace rather than decorative header/footer rules or a dimmed backdrop.

## 10. Layout and token contract

The token dependency chain is:

```text
primitive → semantic → frame → slot → component
```

Rules:

- 4pt primitive spacing scale; 1px borders and documented optical/system corrections are named exceptions.
- No unexplained raw geometry in components.
- Repeated list states share one column template.
- Control height derives from line-height, padding, border, and platform minimum hit target.
- Icons occupy a fixed slot and center within it; adjacent text aligns by baseline.
- Sticky/fixed surfaces reserve layout or matching scroll padding.
- Reader measure, inspector, top bar, and player are independent tokens calculated against available viewport and safe area.
- Fatal empty/loading content centers on x and y inside its owning slot; records align to edges; long-form reading x-centers in the available reader viewport.
- A platform-neutral token manifest generates CSS/TypeScript and Swift constants, and CI rejects generated drift.
- `taste-saas.contract.json` is the Web composition source of truth. Route JSX/CSS changes update the contract first and must pass the skill's static contract checker.

The Web shell locks the viewport; the route layout names the only primary body/data scroller. Breadcrumb is shell-owned route metadata. In Linear-like desktop state the sidebar transitions full ↔ hidden through its resize threshold, always retains an external restore path, and uses the same hidden ↔ drawer state model on true mobile. Persistent brand/search/navigation/footer marks keep their slot centers and DOM identity through state changes.

Management chrome uses the Linear compact-with-breathing equation (12.5px interface basis on an 18px line; 28px sidebar rows; 32px data rows; 39px shell top row; 42px list toolbar) through shared basis/derived tokens. Reader typography and chrome remain a distinct comfortable frame and do not inherit compact list density.

The exhaustive frame/slot/alignment recipes are normative in `cross-platform-design-foundation.md`.

## 11. Shared logic and dependency direction

```text
Web UI ── WASM adapter ─┐
Apple UI ─ Swift FFI ───┼──> Rust Core ──> typed effects
TUI ─ direct Rust ──────┘                      │
                                               ├─ platform audio/clock/storage
                                               └─ Cloudflare HTTP/RPC
Cloudflare Worker ─ Better Auth + D1 + Queue + R2 + cloud TTS
Optional FastAPI ─ local/private TTS provider runtime
```

Rust Core owns:

- versioned Document IR and safe parsing semantics;
- content identity, position projection/restore, folded search, bookmark semantics;
- Manual/Auto/Read-aloud state, effort calculations, playback generations, prefetch, seek, media-clock and sleep-deadline decisions;
- portable sync conflict rules and action availability.

Platforms own:

- rendering, focus, accessibility, file pickers, system clocks, audio decode/output, media session, local persistence adapter, HTTP transport, notifications, and safe-area/system metrics.

Cloudflare Worker owns:

- auth, authorization, CRDT change/cursor state, private snapshots/content, TTS job authority, queueing, quotas and server secrets.

Python TTS is retained only as an optional local/private provider. Web does not run or require Python; ordinary reading does not require login or any TTS engine. Browser-local WASM/WebGPU TTS remains an optional provider only after license/quality/latency/memory/download/cancellation tests pass.

## 12. Durable data rules

- Content identity is independent of path/ownership mode.
- DocumentRevision combines content identity, IR/parser version, revision digest, and character count.
- Character offset plus affinity is the only durable reading coordinate.
- Chapter/block/sentence/percentage are projections for a specific revision.
- Position mutations use base version + idempotent mutation ID.
- Divergent offline sessions preserve both candidates; max percentage or silent last-write-wins is forbidden.
- Bookmarks store offset and resolve display context against the current revision.
- Search hits return original character ranges and revision identity.
- Reading preferences persist mode, base pace, six effort multipliers, and sleep deadline where appropriate.

Exact schemas and conflict rules are normative in `cross-platform-design-foundation.md`.

## 13. Offline behavior

Availability states are `not_available`, `metadata_only`, `content_ready`, `content_and_audio_ready`, `updating`, and `failed`.

- Users can see what will work offline before disconnecting.
- Manual/Auto work from cached Document IR; Read-aloud requires cached audio or an explicitly configured local engine.
- Mutations queue with visible IDs/state and replay idempotently.
- Storage pressure never removes user-owned linked files or unsynced state.
- Sign-out/removal names unsynced consequences.
- Reconnect either applies the causal queue or exposes preserved position conflicts for explicit resolution.

## 14. Accessibility and localization

- Critical journeys work with keyboard/switch/VoiceOver/screen-reader paths and visible focus.
- No pointer-, hover-, color-, or gesture-only capability.
- Reader supports 200% Web zoom and Apple accessibility Dynamic Type without horizontal body-text scrolling.
- Highlighting retains sufficient contrast and a non-color cue.
- Focus traps and returns correctly for modal surfaces; toasts never steal focus.
- Reduced motion disables smooth auto-scroll animation while retaining correct placement.
- English and Chinese cover every product string, error, count, shortcut description, accessibility label, and model/language description.

## 15. Performance and truthfulness budgets

Measured against recorded reference hardware/browser profiles:

- user action receives visual acknowledgment within one frame;
- cached reader shows useful text within 500ms;
- 1M-character cached full-book search returns the first page within 150ms;
- cached sentence begins audio within 200ms;
- uncached TTS immediately shows real queued/running/failure state, never fake percent;
- long-document scroll and active highlight avoid sustained frame drops;
- offline replay remains idempotent across retry and interruption;
- automated accessibility has zero critical findings and manual critical journeys pass.

No budget is green without measurement evidence.

## 16. Implementation sequence

All feature work follows strict RED → GREEN → refactor.

1. Exact position/DocumentRevision contracts and backend persistence.
2. Offset-stable bookmarks.
3. Rust full-book search through WASM/FFI.
4. Reading modes and six-level effort persistence/math.
5. Shadow comparison, then takeover of Web and Swift playback by Rust reducer/effect executors.
6. Sleep timer and explicit failure/provider-attempt recovery.
7. Expand shared document/media parsing and remove duplicate platform policy.
8. Offline stores, mutation queue, sync conflict UI.
9. Native macOS target, Rust packaging, windows/menus/file/audio integration.
10. Localization, accessibility, diagnostics, performance, E2E, and final parity audit.

Each slice contains:

- failing Rust/backend protocol and domain tests;
- boundary fixtures shared by WASM/Swift/backend adapters;
- platform behavior tests for all relevant states;
- regression verification of previously green matrix rows;
- no removal of old implementation until shadow/equivalence evidence proves takeover.

## 17. Release gates

A platform is parity-ready only when:

- all applicable parity rows are green with linked evidence;
- no P0/P1 accessibility, data-loss, playback-authority, or offline-sync defect remains;
- production backend migration and rollback are tested on realistic data;
- Web WASM and Apple FFI use the same protocol fixtures and real built artifacts;
- macOS is a native target with signed runnable evidence;
- critical journeys pass online, offline, background/resume, TTS unavailable, parser revision changed, and conflicting-device scenarios;
- existing user worktree changes remain preserved and unrelated known failures are not misreported as new success.

## 18. Approval

Approval of this brief locks:

- Probe A visual/product direction with bounded Apple Books warmth and Raycast action acceleration;
- native macOS SwiftUI with selective AppKit adapters;
- outcome parity rather than terminal UI imitation;
- Rust Core/WASM/Swift FFI/FastAPI dependency boundaries;
- the normative annexes, implementation sequence, and release gates above;
- AI generation features outside the parity critical path.

After explicit approval, implementation begins with the first failing tests for exact position and DocumentRevision. Material discoveries that change scope or behavior return as a brief amendment; ordinary implementation decisions do not reopen discovery.
