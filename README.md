# readio

**A terminal reader with the interaction grammar of a coding agent.** Press enter and it thinks, issues a tool call, and streams the next passage of your book.

[![tui-ci](https://github.com/hrhrng/readio/actions/workflows/tui-ci.yml/badge.svg?branch=main)](https://github.com/hrhrng/readio/actions/workflows/tui-ci.yml)
[![release](https://img.shields.io/github/v/release/hrhrng/readio?filter=tui-v*&label=release&color=6f5ec7)](https://github.com/hrhrng/readio/releases)
[![license](https://img.shields.io/badge/license-MIT-6f5ec7)](apps/tui/LICENSE)
[![rust](https://img.shields.io/badge/rust-1.85%2B-6f5ec7)](https://www.rust-lang.org)
![platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-6f5ec7)

[中文文档](README.zh-CN.md)

EPUB, text-layer PDF, Markdown and plain text. Read-aloud through a local model of your choice, with the spoken sentence and the sounded character highlighted. Illustrations drawn in the terminal. One 3.9 MB binary with no bundled model, no assets, no runtime dependencies, and no environment variables.

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

```sh
curl -fsSL https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.sh | sh
```

The installer detects your platform, downloads the release archive, verifies it against the release's `SHA256SUMS`, and installs a single file to `~/.local/bin/readio`. It needs no sudo, no compiler and no Rust toolchain; it writes nothing outside the install directory. To uninstall, delete that file and `~/.readio`.

From source, with Rust 1.85 or newer:

```sh
cargo install --git https://github.com/hrhrng/readio readio
```

Prebuilt archives are published for `aarch64`/`x86_64` macOS and `aarch64`/`x86_64` Linux (musl, statically linked). Windows is not packaged yet; build it from `apps/tui`.

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
| `^s` | toggle read-aloud |
| `^p` `^n` · `^l` · `^c` `^d` | input history · clear · quit |

| Command | Purpose |
| --- | --- |
| `/lib` `/open <n>` `/import <path>` `/forget <n>` | manage the library |
| `/toc` `/goto <n>` `/next` `/prev` | move between chapters |
| `/find <term>` | search the whole book |
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

## Read-aloud

readio ships no speech model. It drives whichever engine you have installed through command templates in the config file, so changing models is an edit rather than a new release.

| Engine | Size · licence | Notes |
| --- | --- | --- |
| `kokoro` | 82M · Apache-2.0 | default; multilingual, best on long passages |
| `piper` | ~15M · GPL-3.0 | fastest to first sound; text on stdin |
| `supertonic` | 99M · MIT | pure ONNX, no torch, 31 languages |
| `openai` | — | any OpenAI-compatible `/v1/audio/speech` endpoint |

While a passage is spoken, its sentence is washed lightly and the word or character being sounded is washed deeply, and the reveal speed follows each clip's real duration rather than a guess.

`/device` restricts playback to named audio outputs. When headphones disconnect and the system quietly falls back to the speakers, readio mutes, names the device it found, and offers the way out on screen. An output it cannot identify counts as not allowed.

## Configuration

One file, `~/.readio/config.yaml`, written with comments on first run. readio reads no environment variables. The interface is English by default; set `language: zh` for Chinese. Commands like `/speed`, `/voice`, `/rate` and `/device` write their changes back to the same file.

## Development

```sh
cd apps/tui
cargo test                                     # 181 tests
python3 scripts/pty_probe.py 96 24 "wait:0.6,type:/sample,key:enter,wait:2"
```

The probe drives the binary in a real pty and prints the screen it produced, including a map of which cells were highlighted — the failures that matter here are raw mode, the alternate screen and whether the terminal is restored on exit, none of which a unit test can see. CI runs the suite on macOS and Linux and the probe on both.

## Repository layout

`apps/tui` holds this reader; [`apps/tui/README.md`](apps/tui/README.md) documents its architecture, the disguise vocabulary, and how distribution is built and verified (in Chinese).

`apps/web`, `apps/api` and `apps/extension` are the Speechify-like web stack this repository started as; their setup lives in [`docs/web-api-extension.md`](docs/web-api-extension.md).

## Licence

MIT. See [`apps/tui/LICENSE`](apps/tui/LICENSE).
