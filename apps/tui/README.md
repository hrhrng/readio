# readio — implementation guide

<sub>For what readio is and how to use it, read the [repository README](https://github.com/hrhrng/readio#readme) ([中文](https://github.com/hrhrng/readio/blob/main/README.zh-CN.md)) — absolute links, because this file also ships inside every release archive. This document is for people changing the code: what the modules are, which invariants they hold, and how the thing is tested and shipped.</sub>

[`docs/interaction.md`](docs/interaction.md) is the companion to this file: the interaction design, including the full key-by-state grammar and what is disguised as what. Read it before changing a binding.

readio is a terminal ebook reader whose interface borrows the grammar of a coding agent. Press enter and it thinks, issues a tool call, and streams the next passage of the book. Only the vocabulary is costume: every number on screen is a real paragraph offset, a real line range, a real search count.

## Contents

- [Module map](#module-map)
- [Four decisions](#four-decisions)
- [Books, positions and progress](#books-positions-and-progress)
- [Three ways to read](#three-ways-to-read)
- [Pace, as reasoning effort](#pace-as-reasoning-effort)
- [The command menu](#the-command-menu)
- [Emphasis](#emphasis)
- [Search](#search)
- [Bookmarks](#bookmarks)
- [Read-aloud](#read-aloud)
- [Configuration](#configuration)
- [Keys and commands](#keys-and-commands)
- [Testing](#testing)
- [Build and release](#build-and-release)

## Module map

```
src/
  cli.rs          readio [file] [-c|-l|-m] [--home <dir>]
  paths.rs        host directory, atomic writes
  config.rs       the single config file, and the engine table inside it
  i18n.rs         every visible string, English and Chinese side by side
  theme.rs        colours, including the two highlight washes
  metrics.rs      the disguise: characters → tokens, progress → context, words and lengths
  library.rs      import (three modes), the index, identity by content
  store.rs        reading positions
  wrap.rs         width-aware wrapping: CJK breaks anywhere, Latin keeps words whole
  stream.rs       phrase splitting and the characters-per-second pacer
  book/
    mod.rs        Book, Chapter, Para, Rich, and search
    epub.rs       container → OPF → spine → XHTML, cut at ToC anchors
    css.rs        which CSS classes mean italic or bold
    html.rs       forgiving XHTML extraction (entities, unclosed tags, img)
    pdf.rs        text-layer PDF: per-page text, rejoined lines
    media.rs      illustrations out of an EPUB (double-checked, zip-slip safe)
    text.rs       chapter splitting for Markdown and plain text
    sample.rs     the built-in sample, one per interface language
  ui/
    block.rs      ten block kinds, from user input to library listing
    image.rs      half-block rendering: two pixels per cell, aspect corrected
    scrollback.rs entries, per-entry line cache, tail-following viewport, pixel layer
    prompt.rs     single-line editor with a grapheme-level cursor
    chrome.rs     top bar, status line, help panel
  tts/
    config.rs     engine templates and presets
    device.rs     the output allowlist: probing, matching, the mute decision
    sentence.rs   sentence splitting, and the word or character unit highlighting walks
    command.rs    invoking an engine, playing, cancelling
    wav.rs        clip duration read from the audio itself
    mod.rs        the render and playback threads, and the pipeline between them
  app/
    turn.rs       the turn state machine: thinking → tool → streaming → images → events
    flow.rs       reading semantics: a position in a book becomes a queue of steps
    mod.rs        state, input dispatch, layout
```

## Four decisions

**A turn is a queue of steps.** `Turn` owns timing only — when characters appear, how long a tool call spins, when a turn closes — and knows nothing about books. `flow` translates a reading position into steps. Pointing readio at a different kind of content (a feed, a paper, a log) means writing a new `flow`, not touching the renderer.

**The pacer spends a character budget.** At low speeds it releases part of a long phrase rather than saving up and emitting it in one burst. That is the line between looking like work in progress and looking like a screen dump.

**Identity comes from content.** A book is identified by its length plus a hash of its first 128 KiB, not by its path. Importing the same file twice does not create a second entry, and copying or moving it into the library keeps the position that was already stored. This is what makes `-c`, `-l` and `-m` interchangeable after the fact.

**Illustrations are a pixel layer, not character art.** The block reserves rows and draws a caption; pixels are composited after the text, so an image takes part in ratatui's diffing, can be covered by text, and clips correctly when half of it scrolls off. The kitty and iTerm2 protocols are detected but unused: half-blocks hold in every terminal and never leave an image stranded after a scroll.

## Books, positions and progress

Three parsers produce the same shape — `Book { chapters: Vec<Chapter> }`, each chapter a `Vec<Para>` of headings, body text, quotes, code and images.

| Format | Path through the code |
| --- | --- |
| EPUB | `container.xml` → OPF → spine order → cut at the anchors the ToC points to |
| PDF | `pdf-extract` per page, then broken lines rejoined; the outline names chapters when it lines up |
| Markdown, text | split on `#`/`##` headings, or on blank-line runs and chapter-like lines |

**Chapters are what the table of contents says they are**, not what the file layout says. A ToC entry may point inside a document — `part1.xhtml#ch3` — and conversion tools routinely put a dozen chapters in one file, so each entry becomes a chapter and the text before the first of them stays with the document's own name. In one real book this is the difference between 13 chapters and 72. `html::to_document` therefore reports anchor positions alongside paragraphs, and deduplication renumbers them as it merges. An anchor the document does not contain is not a cut — except when nothing has claimed the start yet, where it is almost always a ToC line naming the whole file.

Spine items marked `linear="no"` are out of the reading order and are not read: copyright pages, adverts, a duplicate cover. Fabricated line numbers continue across a document that holds several chapters, because two `Read` calls on one file should not both start at line 1. A chapter with no ToC entry and no heading of its own is numbered (`Section 4`), never named after its file: `index_split_003` is a fact about the publisher's toolchain.

A position is `(chapter, paragraph)`, but the coordinate that is *stored* is the character offset, and it is the one that wins. Chapter indices are relative to how the book was cut, and that changes: the release that started honouring tables of contents turned index 3 into a different page. `restore()` keeps a stored `(chapter, para)` only while it agrees with the offset beside it, and otherwise asks `Book::locate` where that offset is now. Bookmarks work the same way, which is why one made last month still lands on its sentence.

Progress is cumulative characters over the whole book, the model most readers use and the one `epy` uses. Positions are written to `state.json` through a temp file and a rename, so an interrupted write cannot corrupt the previous one, and the write is shown on screen as a `Write` tool call.

The cover, when a book declares one — EPUB 3's `properties="cover-image"` or EPUB 2's `<meta name="cover">` — is shown when the book is opened, not when it is resumed. A picture that reappears every time the reader presses enter on their history is furniture.

## Three ways to read

A reader is doing one of three things, so readio has one setting with three values rather than two switches with four combinations — one of which, "listening while the passages have stopped coming", never made sense.

| Mode | Chip | What moves the text |
| --- | --- | --- |
| Manual | `⏵ step` | `⏎`, or `↓` `pgdn` and the wheel once you are at the bottom |
| Auto-scroll | `⏵⏵ auto` | nothing to press: passages follow one another |
| Read-aloud | `⏵⏵ voice` | the voice, which brings its own scrolling |

`shift+tab` cycles them, which is the gesture a coding agent uses for exactly this kind of switch, and `/mode [manual|auto|tts]` says it in words. The chip carries no musical note and never will: a `♪` in the corner of the screen announces a media player, which is the one thing this interface must not look like.

`mode::Mode` is not stored. It is read back from the two flags that already existed — `reading.auto` and `tts.enabled` — through `Mode::of`, and `App::set_mode` is the only thing that writes them, which is what keeps them from disagreeing. Entering read-aloud with no working engine is refused rather than half-applied: `start_speaker` says why, the flags stay where they were, and a `shift+tab` cycle skips the mode instead of trapping the reader in it.

Three smaller rules came out of using it:

- **A mode is a setting, not a consequence.** `esc` pauses and a second `esc` stops the turn, but neither demotes auto-scroll to manual. Pausing used to do exactly that, which is how a reader could pause, press enter, and watch the same paragraph arrive twice.
- **In manual mode the bottom of the page loads more.** `↓`, `pgdn` and the wheel scroll as usual until there is nothing below, and then they fetch the next passage, the way reaching the end of a list loads the next page.
- **A mode explains itself once.** The first switch prints a full line — what moves, how to change its speed, how to pause. After that a switch is a flash in the status row, because someone cycling with `shift+tab` does not want the paragraph three times.

## Pace, as reasoning effort

Reading pace is presented as the model's reasoning effort, and the honest consequence of asking for more is kept: **more effort reads more slowly.** Six levels, the six a coding agent offers, shown beside the model name — `readio-1 (high)`.

| Level | Default multiplier | What it feels like |
| --- | --- | --- |
| `minimal` | 2.5× | skim, barely pausing |
| `low` | 2× | quick read |
| `medium` | 1.5× | brisk |
| `high` | 1× | normal reading — the default, and the pace readio has always had |
| `xhigh` | 0.85× | slow, words stay with you |
| `max` | 0.7× | close reading, sentence by sentence |

One multiplier drives both worlds: text appears at `reading.speed × multiplier`, and read-aloud plays at the multiplier itself, so a level means the same thing whether the book is being typed out or spoken. That is also why `^r` is a single key — it always means "change how fast I am reading" — and why `crate::effort` owns the arithmetic while `App::apply_pace` is the only thing that hands a number to the pacer.

The numbers belong to the reader, not to readio. All six live under `effort.multipliers` in the config file; `/rate <0.5-3.0>` retunes the level in force and writes it back, so `/effort xhigh` then `/rate 0.9` is how someone makes their slow gear their own; `/speed <n>` moves the base the multipliers scale. `/effort` with no argument prints the ladder with the active level marked, which is the one place the costume and the honest numbers sit side by side.

Clips are rendered at a speed rather than resampled at playback, so a level change throws away everything prefetched and re-queues from the start of the sentence that was playing: the reader hears the new pace within a sentence instead of at the next passage.

## The command menu

The menu has two levels, because a value is as hard to remember as a command.

1. `/` and a partial name offers commands: name, argument shape, and a one-line description.
2. `/name ` for a command with a fixed set of answers — `/effort`, `/mode`, `/lang`, `/tts` — offers those answers instead, each with what it means and `(active)` on the one in force. Nobody should have to know that `xhigh` is spelled without a hyphen, or which of their engines the config calls `piper`.

`↑` `↓` move, `tab` completes the highlighted row, `⏎` runs it — or completes it, when the command cannot run without an argument. The bracket in `args` is what decides: `<n>` means `⏎` completes, `[n]` means the bare form does something worth seeing. Value rows are built per frame rather than declared, because half of what they say — which level is active, what multiplier it stands for, which engines are installed — is a fact about the reader's config.

The wide description under the list exists because a menu row is the only documentation most readers will ever read. "List the library" does not say what the listing contains; the panel does, and it is where `--copy` versus `--link` versus `--move` gets settled. It wraps to the width available, keeps the list from eating a short terminal, and clips with an ellipsis rather than stopping mid-sentence.

Text that is not a command does nothing. It used to be treated as a question and searched for, which turned a mistyped `2` into a `Grep` across eighty-five paragraphs; now the line stays in the prompt and the status row says commands start with a slash.

## Emphasis

A book that italicises a title is saying something about the title. Emphasis is kept as byte ranges beside the paragraph's text — `Rich { text, emphasis }` — so the text stays a plain string and wrapping, pacing, search and read-aloud all keep treating a paragraph as characters. The renderer lays the ranges over the top, exactly as it already does for the speech highlight, and the two compose: an italic phrase being read aloud is italic *and* washed.

Two sources feed it. Semantic tags (`em`, `i`, `cite`, `strong`, `b`) are obvious. The other is the one that matters in practice: converted EPUBs rarely contain a single `<em>`, and say `<span class="calibre14">` with `font-style: italic` in a stylesheet instead. `book::css` scans the archive's CSS for rules that lean or bolden text and collects their class names — not a CSS engine, and everything it fails to understand it ignores, which is the right failure. Inline `style="font-style: italic"` is honoured too.

Offsets are the fragile part, and there are two conversions: collapsing whitespace moves every byte after the first run of it, so `collapse_ws_indexed` returns a map from input byte to output byte; and the passage adds its own markers (`## `, `> `), so `render_markdown` shifts each paragraph's ranges by where its text landed.

## Search

`/find <term>` runs a real full-text search, reported in the shape of a `Grep` call. Behaviour was checked against `epy` and matches what a reader expects:

- **Every occurrence counts, not one per paragraph.** `Book::search` returns `Hits { total, paragraphs, shown }`, so a header reading `pattern: memory · 45 matches · 30 lines · showing 12` is arithmetic rather than decoration.
- **Matching happens on a folded copy.** `fold()` lowercases and maps full-width forms down to ASCII (U+FF01..FF5E minus 0xFEE0, U+3000 to a space) while recording the original byte index for every folded byte, so highlight ranges land correctly back in the source text. A term typed on an English keyboard finds text typeset in Chinese.
- **Hits are reachable.** Type a number to jump, `^g` and `^b` to walk the list; wrapping says so. Arriving lights the term deeply inside its lightly washed sentence, sharing the two-level highlight with read-aloud — the sentence comes from `tts::sentence::split`.
- **A question is narrowed before it is searched.** A whole sentence is almost never a term. `candidates()` strips interrogative tails, then tries progressively shorter windows, widest first, and answers for the longest phrase that actually occurs in the book.
- **No regular expressions.** Deliberately: `/find ^Chapter .$` would tear the coding-agent skin off in one keystroke.

## Bookmarks

`/mark [note]` keeps the current place, `/marks` lists them, `/marks <n>` goes back to one and `/unmark <n>` drops it. A mark with no note is named after the opening words of what is there, because a list of bookmarks that all say "bookmark" is a list nobody can read. Marking the same place twice renames one mark rather than making two.

They live in `state.json` beside the reading position, stored as character offsets, and the listing resolves each one through `Book::locate` — so a mark keeps pointing at its sentence even after the chapter numbering under it changes. Setting one is shown as the `Write` it really is.

## Read-aloud

readio ships no model. It invokes an engine you installed through a command template, which is why the binary is 4 MB and why a better model next month is a config edit rather than a release. Four presets ship, measured in [tts-bench](https://github.com/5uck1ess/tts-bench):

| Engine | Size · licence | Why it is here |
| --- | --- | --- |
| `kokoro` | 82M · Apache-2.0 | default; multilingual, ~13.8× realtime on an M4, best on long passages |
| `piper` | ~15M · GPL-3.0 | fastest to first sound (62 ms, 33.5× realtime); text on stdin |
| `supertonic` | 99M · MIT | pure ONNX, no torch, 31 languages |
| `openai` | — | any OpenAI-compatible `/v1/audio/speech` endpoint |

Templates are argument lists with the placeholders `{text} {out} {voice} {rate} {model} {json} {file}`, so an engine readio has never heard of still works. A sentence is always passed as **one argument** and never through a shell: `$(whoami)` in the text is just eight characters.

Two things change when speech is on. Reveal speed follows the audio — each clip reports its own duration and the pacer runs at `chars / clip_seconds`, so text finishes exactly when sound does and the configured pace steps aside. And highlighting becomes two-level: the sentence being spoken takes a light wash, the word or character being sounded a deep one. Chinese advances by character (there is nothing to break on, and the character is the unit the eye moves in), Latin by word, with punctuation lit alongside the character it follows.

### Speed and prefetch

Playback speed is the effort multiplier — one control for both worlds, described under [Pace, as reasoning effort](#pace-as-reasoning-effort). While audio plays it sits next to the engine in the status line whenever it is not 1×: `⏵ kokoro · zf_xiaobei 1.5×`.

Speed applies at synthesis, not at playback: resampling would change the voice along with the tempo. The cost is that a speed change invalidates everything already prefetched, so `set_effort` flushes the pipeline, notes where the sentence in progress began, and re-queues from there. A change is heard within a sentence rather than at the next passage.

Sentences are rendered ahead of playback on a thread of their own, `tts.prefetch` of them (two by default), so a sentence boundary is not a hole the length of the engine's synthesis time:

```
render thread ──push──▶ queue (capacity = prefetch) ──pop──▶ playback thread
                          ▲                                      │
                          └── flush(): drop clips, delete wavs ───┘
```

The queue is a `Mutex<VecDeque>` with two condition variables rather than a bounded channel, because a bounded channel cannot be emptied by a third party — the render thread would block sending a clip nobody wants. A locked queue lets `stop()` take the lock, delete the scratch wavs, and wake both threads. Each job carries an `era`; `flush()` bumps it and both threads discard work from an older era, so cancelling never means killing a thread.

### Output allowlist

The failure being prevented is specific: headphones disconnect or go to sleep, the system quietly falls back to the built-in speakers, and the next sentence of the book is read to the whole office. The allowlist makes remembering to check the machine's job — sound only on devices you named, and elsewhere keep reading silently.

```yaml
tts:
  output:
    allow: ["AirPods", "bluetooth"]   # substring of a name, or a transport type
    poll: 5                           # seconds between checks
    on_mismatch: silence              # silence (default) | play, which reads on with a warning
```

Muting is always explained, because silence without explanation is indistinguishable from a broken engine. The notice names the current device, names the allowlist, and offers the three ways out — `/device allow <name>` to add it, `/device any` to stop restricting, `/device` to list everything. Changes take effect at once and are written back to the config file; speech resumes by itself when an allowed device returns.

Two rules are worth stating because they are easy to get backwards. A device that cannot be identified counts as not allowed: better silent than audible in the wrong room. And probing never happens on the UI thread — asking macOS for the current output device takes about 200 ms, six frames' worth, so it runs in the background and the interface reads a cache.

## Configuration

One file, `~/.readio/config.yaml`, written with bilingual comments on first run. **readio reads no environment variables.** `/speed`, `/voice`, `/rate`, `/lang`, `/tts` and `/device` write their changes back to it. The only setting that cannot live there is `--home`, which decides where it is.

```yaml
language: en              # en or zh
reading:
  speed: 46               # characters per second at effort high; the multiplier scales it
  auto: false             # auto-scroll: the mode shift+tab cycles into
effort:
  level: high             # minimal | low | medium | high | xhigh | max
  multipliers:            # more effort reads more slowly; /rate retunes one
    minimal: 2.5
    low: 2
    medium: 1.5
    high: 1
    xhigh: 0.85
    max: 0.7
images:
  enabled: true
  max_rows: 16            # tallest an illustration may be drawn
tts:
  enabled: false
  engine: kokoro
  voice: ""
  prefetch: 2             # sentences rendered ahead of the one playing
# input_log: /tmp/keys.log   # every terminal event appended, for debugging input
```

The host directory:

```
~/.readio/
  config.yaml     settings
  library.json    imported books, in import order
  state.json      reading positions
  books/          the files themselves, for -c and -m imports
  images/         illustrations extracted from EPUBs, filed per book
  speech/         scratch audio, deleted as it finishes playing
```

`readio --home <dir>` uses a different one. The interface is English by default and the built-in sample follows it — an English interface around Chinese prose teaches nobody anything — while a real book always stays in the language it was written in.

## Keys and commands

| Key | Action |
| --- | --- |
| `⏎` | load the next passage; in the library, open the last book |
| `⏎` while streaming | rush this turn to its end |
| `esc` · `esc` again | pause, shown as thinking · stop the turn |
| `shift+tab` | cycle manual → auto-scroll → read-aloud |
| `/` | the command menu: `↑ ↓` to choose, `tab` completes, `⏎` runs |
| `↑ ↓`, wheel, `pgup` `pgdn`, `home` `end` | scroll; at the bottom in manual mode, load more |
| `^t` · `^o` | fold reasoning · tool calls |
| `^s` · `^r` | read-aloud on or off · next reasoning effort |
| `^g` · `^b` | next · previous search hit |
| `^p` `^n` · `^l` · `^c` `^d` | input history · clear · quit |

| Area | Commands |
| --- | --- |
| Library | `/lib` `/open <n>` `/import <path> [--copy\|--link\|--move]` `/forget <n>` `/sample` |
| Reading | `/mode [manual\|auto\|tts]` `/effort [level]` `/toc` `/goto <n>` `/next` `/prev` `/find <term>` `/auto` `/plan` `/context` `/progress` `/speed <n>` |
| Bookmarks | `/mark [note]` `/marks [n]` `/unmark <n>` |
| Read-aloud | `/tts [on\|off\|<engine>\|test\|config]` `/voice <name>` `/rate <0.5-3.0>` `/device` |
| Interface | `/lang en\|zh` `/help` `/quit` |

`/forget` removes only the library's own copy. A file imported with `-l` stays where it is, and one imported with `-m` is not deleted from the library either.

## Testing

```sh
cargo test          # 276: wrapping, pacing, parsing, import modes, reading, whole frames,
                    # illustrations, read-aloud, the device allowlist, search and jumps,
                    # chapter shape, emphasis, bookmarks, the three modes, effort levels
```

- `tests/render.rs` draws a real `App` through ratatui's `TestBackend` and asserts on the screen, so "is the library listed", "did typing 2 open the second book" and "did esc actually interrupt" all have regression cover.
- `tests/effort.rs` holds the pace honest: `^r` walks the ladder and the pacer slows with it, a level names what it is worth, `/rate` retunes only the level in force and writes it back, and `/speed` moves the base the multiplier scales.
- `tests/mode.rs` drives the modes the way a reader does: `shift+tab` into auto-scroll and the text starts moving, two passages arrive with nobody pressing anything, manual mode then sits still, `↓` at the bottom fetches more, and a pause does not silently change the mode.
- `tests/find.rs` walks search → jump by number → the term deeply washed, asserts the counts are true counts, that `^g` says so when it wraps, and that `^g` with no search points back at `/find`.
- `tests/library.rs` covers the filesystem consequences of each import mode, that a second import of the same book adds no second entry, and that `/forget` never deletes a file the reader owns.
- `tests/audio_device.rs` runs the allowlist end to end — blocked, warned, `/device allow`, restored — while `src/tts/device.rs` simulates sleeping headphones through an injected probe and asserts the decision flips once rather than every frame.
- `tests/illustration.rs` generates a PNG and an EPUB on the spot and checks that coloured half-blocks reach the screen, so there is no fixture to go stale.
- Every test runs against a temporary host directory (`paths::set_home`) and cannot touch a real library.

Some things only a terminal knows. `scripts/pty_probe.py` runs the binary in a real pty with a small terminal emulator behind it, and prints the screen as text:

```sh
python3 scripts/pty_probe.py 96 24 "wait:0.4,type:1,key:enter,wait:3" --home /tmp/readio-demo
```

It parses SGR backgrounds and ends with a highlight map — `-` for the sentence being spoken, `#` for the character being sounded — followed by `[exit: clean]` and `[alt-screen restored: True]`, which is how raw mode, the alternate screen and terminal restoration get tested at all:

```
[highlight]  - sentence   # word
  13:    - - - - - - # # - - - - - - - - - - - - -
         An interface is never neutral; it decides for you what deserves attention.
```

CI (`.github/workflows/tui-ci.yml`, at the repository root) runs fmt, clippy with `-D warnings`, the suite, and the probe, on macOS and Linux.

## Build and release

```sh
cargo build --release     # 4.0 MB, thin LTO, one codegen unit, symbols stripped
```

Nothing in the dependency tree compiles C. `zip` is reduced to `default-features = false, features = ["deflate"]`, which drops `zstd-sys` and bzip2 — EPUB only needs store and deflate — and leaves the tree pure Rust. Static musl builds are then a target away rather than a cross toolchain, and one Linux archive runs on any distribution.

Four archives are published: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`. Pushing a `tui-v*` tag builds all four, writes `SHA256SUMS`, and creates the release (`.github/workflows/tui-release.yml`). Windows and anything else builds from source; see the repository README.

Releases are on a beta channel: tags look like `tui-v0.Y.0-beta.N` and anything with `-beta` or `-rc` is flagged as a prerelease. `install.sh` therefore reads the releases *list* rather than `/releases/latest`, which skips prereleases and would find nothing while the newest release is a beta.

It asks twice, because the first way has a quota. The unauthenticated API answers 403 once a machine has made around sixty requests in an hour — which a developer with `gh` open manages easily, and which used to surface as "no release found", blaming the repository for the caller's rate limit. When the API says nothing useful the script reads `releases.atom` instead: the same list, on github.com, with no quota. Only if both come back empty does it give up, and then it says what actually happened and how to name a version by hand.

`install.sh` is POSIX `sh`. It detects the platform, downloads the archive, **verifies it against the release's `SHA256SUMS`**, unpacks, and replaces the binary with a `mv` so an upgrade cannot disturb a running readio. It uses no sudo, writes nothing outside the install directory, never edits a shell profile, and leaves no half-installed binary behind on failure; a checksum mismatch prints both hashes and refuses.

```sh
sh scripts/install.sh --version tui-v0.2.0-beta.2   # a specific release
sh scripts/install.sh --dir /usr/local/bin          # elsewhere (bring write access)
```

`READIO_BASE_URL` and `READIO_API_URL` point the script at a different host, which is how it is tested against a local server standing in for GitHub. They affect the script only; readio itself still reads no environment variables.
