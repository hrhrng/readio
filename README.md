# readio

**A terminal reader with the interaction grammar of a coding agent.** Press enter and it thinks, issues a tool call, and streams the next passage of your book.

[![tui-ci](https://github.com/hrhrng/readio/actions/workflows/tui-ci.yml/badge.svg?branch=main)](https://github.com/hrhrng/readio/actions/workflows/tui-ci.yml)
[![release](https://img.shields.io/github/v/release/hrhrng/readio?include_prereleases&filter=tui-v*&label=release&color=6f5ec7)](https://github.com/hrhrng/readio/releases)
[![license](https://img.shields.io/badge/license-MIT-6f5ec7)](LICENSE)
[![rust](https://img.shields.io/badge/rust-1.85%2B-6f5ec7)](https://www.rust-lang.org)
![platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-6f5ec7)

[中文文档](README.zh-CN.md)

EPUB, text-layer PDF, Markdown and plain text. Chapters as the book's own table of contents defines them, italics kept, covers and illustrations drawn in the terminal. Read-aloud through a local model of your choice, with the spoken sentence and the sounded character highlighted. One 4 MB binary with no bundled model, no assets, no runtime dependencies, and no environment variables.

Every number on screen is a real reading — real paragraph offsets, real line ranges, real full-text search hits. Only the vocabulary is costume.

```
 readio   The Shape of Attention  readio sample                   ch 1/4  ·  ctx 0.0%
   ○  3. Three  The feel of a tool  783 tok
   ○  4. Four  One continuous piece of work  485 tok

 ❙ Thought for 1.3s

 ● Read readio://sample/attention.md  L1-9  ·  0.3s
   # One  Attention in eighty columns
   The first time I noticed that attention has a shape, I was watching a cursor…
   I was waiting on a slow build. There was nothing on the screen but that one …

   One  Attention in eighty columns

   The first time I noticed that attention has a shap▌
╭────────────────────────────────────────────────────────────────────────────────────╮
│❯ enter to keep reading, or ask a question / type a command                         │
╰────────────────────────────────────────────────────────────────────────────────────╯
  ⠼ One  Attention in eighty co…  ·  esc to stop  ·  ↑↓ … ⸬ readio-1  64 tok  ·  0:06
```

<sub>Captured from a real pty by `apps/tui/scripts/pty_probe.py`, not typed by hand.</sub>

## Install

> **readio is in beta.** Releases are tagged `tui-v0.Y.0-beta.N` and flagged as prereleases on GitHub; the installer takes the newest one. Things settle as they get used — read-aloud in particular has been exercised against command templates and tests, not against every engine in the table.

```sh
curl -fsSL https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.sh | sh
```

The installer detects your platform, downloads the release archive, verifies it against the release's `SHA256SUMS`, and installs a single file to `~/.local/bin/readio`. It needs no sudo, no compiler and no Rust toolchain; it writes nothing outside the install directory. To uninstall, delete that file and `~/.readio`.

From source, with Rust 1.85 or newer:

```sh
cargo install --git https://github.com/hrhrng/readio readio
```

Prebuilt archives are published for `aarch64`/`x86_64` macOS and `aarch64`/`x86_64` Linux (musl, statically
linked). They are a convenience, not the only path: anything else — Windows, a BSD, an architecture nobody
packages — builds from source with the command above, since the dependency tree is pure Rust and needs no C
toolchain. Windows in particular is untested rather than unsupported.

## Building on Windows

Windows has no prebuilt archive and no installer script — it is untested rather than unsupported. The dependency tree is pure Rust, so a build needs Rust and a linker, nothing else.

1. **Install Rust** with [rustup](https://rustup.rs). Keep the default `x86_64-pc-windows-msvc` host and let it install the Visual Studio Build Tools it asks for (*Desktop development with C++*). readio contains no C, but the MSVC linker is still what `rustc` invokes. If you would rather not install Visual Studio, `rustup default stable-x86_64-pc-windows-gnu` works with MinGW-w64 instead.

2. **Build it.** In PowerShell:

   ```powershell
   git clone https://github.com/hrhrng/readio
   cd readio\apps\tui
   cargo build --release
   .\target\release\readio.exe
   ```

3. **Put it on PATH.** `cargo install --path .` places `readio.exe` in `%USERPROFILE%\.cargo\bin`, which rustup already added to your PATH.

4. **Use a VT-capable terminal.** readio needs truecolour, the alternate screen, and half-block characters for illustrations: Windows Terminal handles all three. In the legacy `conhost` console, run `chcp 65001` first or the box drawing and any CJK text will come out as mojibake.

5. **Where things live.** `%USERPROFILE%\.readio` holds `config.yaml`, `books\` and `state.json`. `readio --home D:\readio` moves the lot somewhere else.

6. **Read-aloud** needs no extra player: the default `play` command is a PowerShell one-liner using `Media.SoundPlayer`. You still supply the speech engine yourself and point `tts.engines.<name>.synth` at its Windows command line.

Two things do not come along. `scripts/install.sh` is POSIX sh, and `scripts/pty_probe.py` needs a POSIX pty, so neither runs here — build and run the binary directly. The audio-output whitelist also has no built-in device probe on Windows: set `tts.output.query` to a command that prints the current output device (PowerShell with the `AudioDeviceCmdlets` module, for instance), and until you do, `/device` will say it cannot read the list and that the whitelist keeps speech muted.

`cargo test` should work — the frame tests render through ratatui's `TestBackend` rather than a real terminal — but nobody has run the suite on Windows, so treat a failure there as a bug worth reporting rather than a surprise.

## Usage

```sh
readio                 # open the library
readio book.epub       # import and start reading (-c copy, the default)
readio book.pdf -l     # link: record the path, do not copy
readio book.md -m      # move: relocate the file into the library
```

Copies land in `~/.readio/books`. Use `readio --home <dir>` to keep a separate library. With no book to hand, `/sample` opens a short built-in text.

**Enter reads the next passage. Anything you type is treated as a question and runs a full-text search.**

| Key | Action |
| --- | --- |
| `enter` | keep reading; again mid-passage to rush it to the end |
| `esc` | interrupt |
| `↑` `↓` · wheel · `pgup` `pgdn` · `home` `end` | scroll |
| `^t` · `^o` | fold or unfold reasoning · tool calls |
| `^s` · `^r` | toggle read-aloud · cycle its speed (0.75× → 2×) |
| `^g` · `^b` | jump to the next · previous search hit |
| `^p` `^n` · `^l` · `^c` `^d` | input history · clear · quit |

| Command | Purpose |
| --- | --- |
| `/lib` `/open <n>` `/import <path>` `/forget <n>` | manage the library |
| `/toc` `/goto <n>` `/next` `/prev` | move between chapters |
| `/find <term>` | search the whole book; type a number to jump to that hit |
| `/mark [note]` `/marks [n]` `/unmark <n>` | keep a place, list places, drop one |
| `/auto` `/speed <n>` | keep reading unattended · reveal speed |
| `/context` `/progress` `/plan` | where you are |
| `/tts` `/voice <name>` `/rate <0.5-3>` `/device` | read-aloud and audio output |
| `/lang en\|zh` `/help` `/quit` | interface language · help · exit |

## What the interface pretends to be

| Reading concept | Shown as |
| --- | --- |
| How far through the book you are | `ctx 23.3%`, context-window usage |
| Characters in a passage | `735 tok`, a token count |
| Time since you opened the book | `0:15`, a session clock |
| Fetching the next passage | a tool call: `● Read book.epub#ch1  L1-9  ·  0.3s` |
| Full-text search | the question you asked, with real hit counts |

## Reading a book as the book is written

readio follows the file rather than the filesystem.

**Chapters come from the table of contents.** A conversion tool will happily put a dozen chapters in one XHTML file and point the ToC at anchors inside it; readio cuts there, so a book whose contents page lists 71 sections has 71 chapters and not 13. Documents the spine marks `linear="no"` — copyright pages, adverts — are not part of the read. A section the ToC never names and that carries no heading is numbered rather than named after its file, because `index_split_003` says something about the publisher's toolchain and nothing about the book.

**Italics survive.** Emphasis is kept as ranges over the text and drawn as a terminal modifier, whether the book spelled it `<em>` or — as converted EPUBs almost always do — as a CSS class with `font-style: italic`. It composes with read-aloud: an italic phrase being spoken is italic and washed at once.

**The cover is shown when you open a book**, and not when you resume one.

**Places you keep are kept properly.** `/mark` remembers where you are, `/marks` lists what you kept, `/marks <n>` goes back. Bookmarks and your reading position are stored as character offsets, so they still point at the same sentence after an update changes how the book is divided — the update that introduced ToC chapters moved every chapter number, and nobody lost their place.

## Search

`/find <term>` — or simply a question typed at the prompt — searches the whole book and counts **every** occurrence, not one per paragraph. The header reports what a reader wants to know before deciding whether to look: `pattern: memory · 45 matches · 30 lines · showing 12`. Matching ignores case and treats full-width punctuation and Latin letters as their ASCII equivalents, so a term typed on an English keyboard still finds text typeset in Chinese.

Type the number of a hit to jump there; `^g` and `^b` walk forward and back through the list and say so when they wrap. Arriving at a hit lights the term deeply inside its lightly washed sentence — the same two-level highlight read-aloud uses — so the eye lands on the word rather than on the paragraph.

A question in Chinese rarely reads as a search term, so readio narrows it before searching: interrogative tails such as 是什么样子 or 怎么 are stripped, then progressively shorter windows of the remaining text are tried, widest first. What comes back is the answer to the longest phrase that actually occurs in the book.

## Read-aloud

readio ships no speech model. It drives whichever engine you have installed through command templates in the config file, so changing models is an edit rather than a new release.

| Engine | Size · licence | Notes |
| --- | --- | --- |
| `kokoro` | 82M · Apache-2.0 | default; multilingual, best on long passages |
| `piper` | ~15M · GPL-3.0 | fastest to first sound; text on stdin |
| `supertonic` | 99M · MIT | pure ONNX, no torch, 31 languages |
| `openai` | — | any OpenAI-compatible `/v1/audio/speech` endpoint |

While a passage is spoken, its sentence is washed lightly and the word or character being sounded is washed deeply, and the reveal speed follows each clip's real duration rather than a guess.

Speed works the way an audiobook app's does. `^r` cycles 0.75×, 1×, 1.25×, 1.5×, 2× — the same ladder the web player offers — and `/rate` takes any value from 0.5 to 3. The multiplier sits in the status line next to the engine while audio is playing. Because clips are *rendered* at a speed rather than resampled on playback, a change throws away everything already prefetched and re-queues from the start of the sentence you are hearing, so the new speed arrives within a sentence instead of at the next passage.

Sentences are rendered ahead of playback — `tts.prefetch`, two by default — on a thread of their own, so a sentence boundary is not a hole the length of your engine's synthesis time.

`/device` restricts playback to named audio outputs. When headphones disconnect and the system quietly falls back to the speakers, readio mutes, names the device it found, and offers the way out on screen. An output it cannot identify counts as not allowed.

## Configuration

One file, `~/.readio/config.yaml`, written with comments on first run. readio reads no environment variables. The interface is English by default; set `language: zh` for Chinese. Commands like `/speed`, `/voice`, `/rate` and `/device` write their changes back to the same file.

## Development

```sh
cd apps/tui
cargo test                                     # 240 tests
python3 scripts/pty_probe.py 96 24 "wait:0.6,type:/sample,key:enter,wait:2"
```

The probe drives the binary in a real pty and prints the screen it produced, including a map of which cells were highlighted — the failures that matter here are raw mode, the alternate screen and whether the terminal is restored on exit, none of which a unit test can see. CI runs the suite on macOS and Linux and the probe on both.

## Repository layout

`apps/tui` holds this reader; [`apps/tui/README.md`](apps/tui/README.md) is its implementation guide — module map, the invariants each layer holds, and how the thing is tested and shipped.

`apps/web`, `apps/api` and `apps/extension` are the Speechify-like web stack this repository started as; their setup lives in [`docs/web-api-extension.md`](docs/web-api-extension.md).

## Licence

MIT, for everything in this repository. See [`LICENSE`](LICENSE).
