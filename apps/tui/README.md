# readio — implementation guide

<sub>For what readio is and how to use it, read the [repository README](https://github.com/hrhrng/readio#readme) ([中文](https://github.com/hrhrng/readio/blob/main/README.zh-CN.md)) — absolute links, because this file also ships inside every release archive. This document is for people changing the code: what the modules are, which invariants they hold, and how the thing is tested and shipped.</sub>

readio is a terminal ebook reader whose interface borrows the grammar of a coding agent. Press enter and it thinks, issues a tool call, and streams the next passage of the book. Only the vocabulary is costume: every number on screen is a real paragraph offset, a real line range, a real search count.

## Contents

- [Module map](#module-map)
- [Four decisions](#four-decisions)
- [Books, positions and progress](#books-positions-and-progress)
- [Search](#search)
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
    mod.rs        Book, Chapter, Para, and search
    epub.rs       container → OPF → spine → XHTML
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
| EPUB | `container.xml` → OPF → spine order → each XHTML document is a chapter; ToC labels name them |
| PDF | `pdf-extract` per page, then broken lines rejoined; the outline names chapters when it lines up |
| Markdown, text | split on `#`/`##` headings, or on blank-line runs and chapter-like lines |

A position is `(chapter, paragraph)`. Progress is cumulative characters over the whole book, which is the model most readers use and the one `epy` uses. A chapter with no ToC entry and no heading of its own is numbered (`Section 4`), never named after its file: `index_split_003` is a fact about the publisher's toolchain.

Positions are written to `state.json` through a temp file and a rename, so an interrupted write cannot corrupt the previous one, and the write is shown on screen as a `Write` tool call.

## Search

Anything typed at the prompt that is not a command runs a real full-text search, reported in the shape of a `Grep` call. Behaviour was checked against `epy` and matches what a reader expects:

- **Every occurrence counts, not one per paragraph.** `Book::search` returns `Hits { total, paragraphs, shown }`, so a header reading `pattern: memory · 45 matches · 30 lines · showing 12` is arithmetic rather than decoration.
- **Matching happens on a folded copy.** `fold()` lowercases and maps full-width forms down to ASCII (U+FF01..FF5E minus 0xFEE0, U+3000 to a space) while recording the original byte index for every folded byte, so highlight ranges land correctly back in the source text. A term typed on an English keyboard finds text typeset in Chinese.
- **Hits are reachable.** Type a number to jump, `^g` and `^b` to walk the list; wrapping says so. Arriving lights the term deeply inside its lightly washed sentence, sharing the two-level highlight with read-aloud — the sentence comes from `tts::sentence::split`.
- **A question is narrowed before it is searched.** A whole sentence is almost never a term. `candidates()` strips interrogative tails, then tries progressively shorter windows, widest first, and answers for the longest phrase that actually occurs in the book.
- **No regular expressions.** Deliberately: `/find ^Chapter .$` would tear the coding-agent skin off in one keystroke.

## Read-aloud

readio ships no model. It invokes an engine you installed through a command template, which is why the binary is 4 MB and why a better model next month is a config edit rather than a release. Four presets ship, measured in [tts-bench](https://github.com/5uck1ess/tts-bench):

| Engine | Size · licence | Why it is here |
| --- | --- | --- |
| `kokoro` | 82M · Apache-2.0 | default; multilingual, ~13.8× realtime on an M4, best on long passages |
| `piper` | ~15M · GPL-3.0 | fastest to first sound (62 ms, 33.5× realtime); text on stdin |
| `supertonic` | 99M · MIT | pure ONNX, no torch, 31 languages |
| `openai` | — | any OpenAI-compatible `/v1/audio/speech` endpoint |

Templates are argument lists with the placeholders `{text} {out} {voice} {rate} {model} {json} {file}`, so an engine readio has never heard of still works. A sentence is always passed as **one argument** and never through a shell: `$(whoami)` in the text is just eight characters.

Two things change when speech is on. Reveal speed follows the audio — each clip reports its own duration and the pacer runs at `chars / clip_seconds`, so text finishes exactly when sound does and `/speed` steps aside. And highlighting becomes two-level: the sentence being spoken takes a light wash, the word or character being sounded a deep one. Chinese advances by character (there is nothing to break on, and the character is the unit the eye moves in), Latin by word, with punctuation lit alongside the character it follows.

### Speed and prefetch

`^r` cycles 0.75×, 1×, 1.25×, 1.5×, 2× — the ladder the web player uses — and `/rate` takes any value from 0.5 to 3.0. While audio plays the multiplier sits next to the engine in the status line: `♪ kokoro · zf_xiaobei 1.5×`.

Speed applies at synthesis, not at playback: resampling would change the voice along with the tempo. The cost is that a speed change invalidates everything already prefetched, so `set_speed` flushes the pipeline, notes where the sentence in progress began, and re-queues from there. A change is heard within a sentence rather than at the next passage.

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
  speed: 46               # characters revealed per second; read-aloud overrides it
  auto: false             # keep going without pressing enter
images:
  enabled: true
  max_rows: 16            # tallest an illustration may be drawn
tts:
  enabled: false
  engine: kokoro
  voice: ""
  rate: 1.0               # 0.5–3.0, the ^r ladder writes here
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
| `⏎` | read on; with input, ask; in the library, open the last book |
| `⏎` while streaming | rush this turn to its end |
| `esc` | interrupt |
| `↑ ↓`, wheel, `pgup` `pgdn`, `home` `end` | scroll |
| `^t` · `^o` | fold reasoning · tool calls |
| `^s` · `^r` | read-aloud on or off · cycle speed |
| `^g` · `^b` | next · previous search hit |
| `^p` `^n` · `^l` · `^c` `^d` | input history · clear · quit |

| Area | Commands |
| --- | --- |
| Library | `/lib` `/open <n>` `/import <path> [-c\|-l\|-m]` `/forget <n>` `/sample` |
| Reading | `/toc` `/goto <n>` `/next` `/prev` `/find <term>` `/auto` `/plan` `/context` `/progress` `/speed <n>` |
| Read-aloud | `/tts [on\|off\|<engine>\|test\|config]` `/voice <name>` `/rate <0.5-3.0>` `/device` |
| Interface | `/lang en\|zh` `/help` `/quit` |

`/forget` removes only the library's own copy. A file imported with `-l` stays where it is, and one imported with `-m` is not deleted from the library either.

## Testing

```sh
cargo test          # 212: wrapping, pacing, parsing, import modes, reading, whole frames,
                    # illustrations, read-aloud, the device allowlist, search and jumps
```

- `tests/render.rs` draws a real `App` through ratatui's `TestBackend` and asserts on the screen, so "is the library listed", "did typing 2 open the second book" and "did esc actually interrupt" all have regression cover.
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

`install.sh` is POSIX `sh`. It detects the platform, downloads the archive, **verifies it against the release's `SHA256SUMS`**, unpacks, and replaces the binary with a `mv` so an upgrade cannot disturb a running readio. It uses no sudo, writes nothing outside the install directory, never edits a shell profile, and leaves no half-installed binary behind on failure; a checksum mismatch prints both hashes and refuses.

```sh
sh scripts/install.sh --version tui-v0.2.0-beta.1   # a specific release
sh scripts/install.sh --dir /usr/local/bin          # elsewhere (bring write access)
```

`READIO_BASE_URL` and `READIO_API_URL` point the script at a different host, which is how it is tested against a local server standing in for GitHub. They affect the script only; readio itself still reads no environment variables.
