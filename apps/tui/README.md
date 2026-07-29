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
    block.rs      nine block kinds, from user input to a rendered illustration
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
| Manual | `⏵ manual` | `⏎`, or `↓` `pgdn` and the wheel once you are at the bottom |
| Auto-scroll | `⏵⏵ auto` | nothing to press: passages follow one another |
| Read-aloud | `⏵⏵ aloud` | the voice, which brings its own scrolling |

`shift+tab` cycles them, which is the gesture a coding agent uses for exactly this kind of switch, and `/mode [manual|auto|tts]` says it in words. The chip carries no musical note and never will: a `♪` in the corner of the screen announces a media player, which is the one thing this interface must not look like.

`mode::Mode` is not stored. It is read back from the two flags that already existed — `reading.auto` and `tts.enabled` — through `Mode::of`, and `App::set_mode` is the only thing that writes them, which is what keeps them from disagreeing. Entering read-aloud with no working engine is refused rather than half-applied: `start_speaker` says why, the flags stay where they were, and a `shift+tab` cycle skips the mode instead of trapping the reader in it.

Three smaller rules came out of using it:

- **A mode is a setting, not a consequence.** `esc` interrupts the turn and `⏎` picks it back up, but neither demotes auto-scroll to manual. Interrupting used to do exactly that, which is how a reader could stop, press enter, and watch the same paragraph arrive twice.
- **In manual mode the bottom of the page loads more.** `↓`, `pgdn` and the wheel scroll as usual until there is nothing below, and then they fetch the next passage, the way reaching the end of a list loads the next page.
- **A mode explains itself once.** The first switch prints a full line — what moves, how to change its speed, how to interrupt it. After that a switch is a flash in the status row, because someone cycling with `shift+tab` does not want the paragraph three times.

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

The numbers belong to the reader, not to readio. All six live under `effort.multipliers` in the config file; `/rate <0.5-3.0>` retunes the level in force and writes it back, so `/effort xhigh` then `/rate 0.9` is how someone makes their slow gear their own; `/speed <n>` moves the base the multipliers scale. `/effort` with no argument opens the ladder above the composer, each level showing the multiplier behind it and `(active)` on the one in force — the one place the costume and the honest numbers sit side by side.

Clips are rendered at a speed rather than resampled at playback, so a level change throws away everything prefetched and re-queues from the start of the sentence that was playing: the reader hears the new pace within a sentence instead of at the next passage.

## The command menu

The menu has two levels, because a value is as hard to remember as a command.

1. `/` and a partial name offers commands: name, argument shape, and a one-line description.
2. `/name ` offers that command's answers. For a fixed set — `/effort`, `/mode`, `/lang`, `/tts` — those are the levels, modes, languages and engines, each with what it means and `(active)` on the one in force; nobody should have to know that `xhigh` is spelled without a hyphen, or which of their engines the config calls `piper`. For a command whose answers are the reader's own things — `/open`, `/goto`, `/marks`, `/unmark` — they are their books, chapters and bookmarks. For `/import` they are files and directories, read from the disk.

`↑` `↓` move, `tab` completes the highlighted row, `⏎` runs it — or completes it, when the command cannot run without an argument. The bracket in `args` is what decides: `<n>` means `⏎` completes, `[n]` means the bare form does something worth seeing. Value rows are built per frame rather than declared, because half of what they say — which level is active, what multiplier it stands for, which engines are installed — is a fact about the reader's config.

The same list is what a key opens. `^r`, a bare `/effort`, launching with a library, or `/goto` with no number all raise a **select**: the same rows, anchored above the composer, `↑` `↓` to choose and `⏎` to confirm. A select keeps its own narrowing text rather than borrowing the prompt line, because writing `/effort ` into the composer to hold the state throws away whatever the reader was halfway through typing. Two rules make it safe: a select never touches what the reader was typing, and `/` always begins a command — the library select is open the moment readio starts, so the first thing anyone types would otherwise be eaten by a filter.

That is also why no command prints a numbered listing into the transcript any more. Sixteen books in the log with "type 7" underneath is a listing pretending to be a control: it costs a screenful, it goes stale as soon as anything changes, and it cannot be navigated. The transcript keeps the book and what happened to it; the questions live where they are answered.

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

readio ships no model. It invokes an engine you installed through a command template, which is why the binary is 4 MB and why a better model next month is a config edit rather than a release. Five presets ship, chosen from [tts-bench](https://github.com/5uck1ess/tts-bench):

| Engine | Size · licence | Why it is here |
| --- | --- | --- |
| `kokoro` | 82M · Apache-2.0 | default; the best voice here, and it stays loaded |
| `espeak` | ~4M · GPL-3.0 | instant and robotic; one `brew`/`apt` package, no download |
| `piper` | ~15M · GPL-3.0 | fastest neural voice to first sound; text on stdin |
| `supertonic` | 99M · MIT | pure ONNX, no torch, 31 languages |
| `openai` | — | any OpenAI-compatible `/v1/audio/speech` endpoint |

A word on the speed figures you will find in that benchmark: they measure the models, usually through PyTorch on a GPU. readio drives command-line engines, and those are mostly onnxruntime on the CPU. It is worth knowing which part of that is the model: for `kokoro-tts`, one short sentence through the CLI took 8.4 seconds on an M4 — four times slower than saying it out loud — of which the ONNX inference was 0.5 s. The rest was starting Python, loading the weights, and, once those were fixed, rebuilding an espeak backend inside `phonemizer` for every single sentence. Addressed below, the same sentence takes 0.5–0.8 s, comfortably ahead of a listener. `espeak-ng` takes 0.03 s and always did; it is the preset to reach for on a slow machine, or when you want sound the instant you press a key.

### The engine stays running

An engine that loads an 82M model per sentence spends all its time loading an 82M model. So a preset may carry a `serve:` line beside its `synth:` one — a command that starts the engine once and speaks a line-delimited JSON protocol over its pipes, one request per sentence, one reply per clip. readio starts it on the first sentence of the session (in fact at boot, under the opening turn, so the wait lands where nobody is listening), keeps it for as long as the app runs, and falls back to the plain `synth:` command line for engines that have no such mode. The worker shipped for Kokoro is `src/tts/worker.py`, compiled into the binary with `include_str!` and written out to `~/.readio/engines/kokoro_worker.py` — atomically, through a staging file, so two readios starting at once cannot hand each other half a script.

The profile of what remained is worth recording, because it is not where anyone would look: with the model warm and resident, 2.216 seconds of every 2.752 were still inside `phonemizer.phonemize`, which builds a fresh espeak backend on each call — eight `dlopen`s, 2.189 s — and documents that it has no cache. Caching one backend per language brings a short sentence to 0.5–0.8 s, of which the actual inference is 0.5. That is a 3.6× improvement made without touching the model.

Templates are argument lists with the placeholders `{text} {out} {voice} {rate} {scale} {model} {lang} {json} {file} {extra}`, so an engine readio has never heard of still works. A sentence is always passed as **one argument** and never through a shell: `$(whoami)` in the text is just eight characters.

### Reading in more than one language

A multilingual model still has to be told which language it is looking at, and the default is rarely yours: `kokoro-tts` assumes `en-us`, so Chinese handed to it unannounced is sounded out with English letter-to-sound rules. The same eighteen-character sentence takes 13.6 seconds that way and 4.1 seconds said properly — slurred, and three times too slow.

So each engine carries a small table of what changes when the page changes language:

```yaml
languages:
  zh:
    voice: zf_xiaoxiao
    lang: --lang cmn
  en:
    voice: af_heart
    lang: --lang en-us
```

The voice moves with the language because in Kokoro the two are the same decision — `zf_*` is Mandarin, `af_*` American English — and settings that can be made to disagree eventually will. `lang` holds the flag *and* its value, so an engine that spells it `--language zh` needs no support from readio, and an engine with nothing to say there leaves it empty and is passed no flag at all. Piper's table is empty for a different reason: its language is the model file, so the entry to write is a `model:`, not a flag.

Which entry applies is decided per sentence, from the text: one Han character in eight makes a passage Chinese, so a Chinese page quoting an English term stays Chinese and an English page quoting a single 字 stays English. A bilingual chapter switches voice mid-page without anyone reaching for a setting.

`/voice` is where that is visible, and its first row is `auto`:

```
❯ auto (active)  match each passage as it comes
  en             pin this language, read by af_heart
  zh             pin this language, read by zf_xiaoxiao
```

Picking a language pins `tts.language` rather than a voice — the voice that belongs to it is recorded beside it, and settings that can be made to disagree eventually are. A name typed instead (`/voice zf_xiaoyi`) goes straight through to the engine and outranks both; one readio recognises brings its language along, so asking for a Mandarin voice is asking for Mandarin. `auto` is a row rather than only a config value because a setting that can only be turned on is a trap: before it existed, `/voice af_heart` was a one-way door that only a text editor could reopen.

### Choosing one, and getting one

`/tts` is the voice, and nothing else: read-aloud is one of the three reading modes, so `shift+tab` and `/mode` turn it on and off, and a switch in this menu as well would be a second control for the same fact. What the menu answers is *which* voice — and, because the honest answer to that depends on the machine, whether you have it at all:

```
❯ kokoro (active)  Kokoro-82M · Apache-2.0 · multilingual, best on long passages · ⏎ installs it
  openai           a server, nothing to install
  piper            Piper · GPL-3.0 · fastest to first sound · ⏎ installs it
  supertonic       Supertonic 99M · MIT · pure ONNX, no torch · ⏎ reads with it
```

Each row checks the engine's program, and for an engine whose voice is a separate download, that file too — so `⏎` either switches and starts reading, or installs and then does that. Nobody has to find out an engine is missing by choosing it and hearing nothing.

Installing shows itself, one `Bash` call per command, streaming:

```
● Bash uv tool install --python 3.12 kokoro-tts  1/1  ·  24.8s
  Resolved 61 packages in 1.31s
  Installed 61 packages in 3.42s
   + kokoro-tts==0.9.4

○ kokoro is installed (25s). Read-aloud is on: kokoro · zf_xiaobei
```

What it decides, and why each decision cannot be a constant in a release:

| Decision | How |
| --- | --- |
| Which installer | `uv tool install`, then `pipx install`, then `pip install --user` — whichever this machine has. pip is last because Homebrew and every modern distribution now refuse it outright (PEP 668), and a refusal the reader has to decode is worse than saying up front that nothing suitable is here. |
| Which Python | From the preset when the package is fussy. `kokoro-tts` declares `>=3.11,<3.13`, so a machine defaulting to 3.13 otherwise fails with a resolver error that never mentions the version. |
| The voice model | Neither Kokoro nor Piper ships weights in its wheel, so their model files are fetched as further steps of the same install and written into the engine's command line. Kokoro's `--model` and `--voices` default to `./`, which means an engine installed without them works in exactly one directory: whichever one the files were downloaded into. An engine whose command exists but whose voice does not is not installed, and the menu says so. |
| Where the command went | `uv` and `pipx` write into `~/.local/bin`, which is on the PATH of the shell that set them up and not necessarily on the one readio inherited. If the program is not on PATH afterwards, the place it actually landed is written into the engine's command line — rather than asking anyone to edit a shell profile and start over. |

Nothing runs through a shell here either; every command is an argv, and every command is printed before it runs, so "install it for me" and "tell me what you would run" are the same feature. A subprocess's output is stripped of escape sequences before it reaches the screen — an installer is not entitled to move readio's cursor — and split on carriage returns as well as newlines, which is the only way a progress bar shows progress rather than arriving in one lump after the download finishes.

`openai` is not installable and says so: it is a server you run yourself, and readio has no business starting it.

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

That covers every boundary inside a paragraph. It cannot cover the boundary *between* two of them, because until the next turn starts the next paragraph does not exist: the reader waits out a thinking line, a tool call and then a whole synthesis, in silence, every few hundred characters. Rendering ahead has nothing to render.

So the turn is asked where it is going while it is still running. `Step::Advance` sits at the back of its queue for the whole turn, which makes it an honest answer; when the voice is down to its last clip and no more speech is queued, `App::render_ahead` works out the paragraph that comes next, takes its opening sentence, and sends it to the engine as `Command::Warm`. The render thread keeps it in a slot of its own — outside the prefetch queue, which belongs to the passage already begun — and hands it over the moment that sentence is asked for in earnest. Nothing is displayed early and no step runs out of order; the only thing that moves is the engine's work, into the time when the engine has nothing else to do.

Measured with a stand-in engine that takes two seconds a sentence, on a book of one-sentence chapters so the window is almost nothing but boundaries: silence fell from 43% of frames to 18%, and the longest unbroken gap from 3.3 s to 1.3 s — which is the thinking line and the tool call, and those are meant to be there. On a real Kokoro reading Chinese, the reveal goes from 170 to 240 words in the same 75 seconds, with no stall at all after the model has loaded.

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
  language: auto          # or zh / en, to stop readio guessing per sentence
  prefetch: 2             # sentences rendered ahead of the one playing
# input_log: /tmp/keys.log   # every terminal event appended, for debugging input
```

The `engines:` block below that is settings for the engines themselves, and it is the one part of the file readio maintains as well as reads. On startup each built-in engine is reconciled with the binary, in three categories: what the package *is* — the description, the docs link, the PyPI name, the Python it needs — always follows the release, because nobody writes those by hand and a file written before those fields existed would otherwise freeze them empty forever. How to *run* it — the command, the player, whether the text goes on stdin, the language table — is yours, and is replaced only when what is saved is a default readio itself shipped and has since corrected. Which *voice* — `voice`, `model`, `extra` — is never touched.

This is not hypothetical tidiness: every local preset shipped before v0.2.0 had a command line that did not match its engine's actual CLI, and the one shipped in v0.2.0-beta.3 ran but never passed a language, so it read Chinese with English phonemes. Without reconciliation an upgrade would leave read-aloud broken, or quietly bad, on exactly the machines that had used it longest.

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
| `esc` | interrupt the turn, keeping your place; `⏎` carries on |
| `shift+tab` | cycle manual → auto-scroll → read-aloud |
| `/` | the command menu: `↑ ↓` to choose, `tab` completes, `⏎` runs |
| `↑ ↓`, wheel, `pgup` `pgdn`, `home` `end` | scroll; at the bottom in manual mode, load more |
| `^t` · `^o` | fold reasoning · tool calls |
| `^s` · `^r` | read-aloud on or off · next reasoning effort |
| `^g` · `^b` | next · previous search hit |
| `^p` `^n` · `^l` · `^c` `^d` | input history · clear the screen · discard the turn, then quit |

| Area | Commands |
| --- | --- |
| Library | `/lib` `/open <n>` `/import <path> [--copy\|--link\|--move]` `/forget <n>` `/sample` |
| Reading | `/mode [manual\|auto\|tts]` `/effort [level]` `/toc` `/goto <n>` `/next` `/prev` `/find <term>` `/auto` `/plan` `/context` `/progress` `/speed <n>` |
| Bookmarks | `/mark [note]` `/marks [n]` `/unmark <n>` |
| Read-aloud | `/tts [<engine>\|install <engine>\|test\|config]` `/voice [auto\|zh\|en\|<name>]` `/rate <0.5-3.0>` `/device` |
| Interface | `/lang en\|zh` `/help` `/quit` |

`/forget` removes only the library's own copy. A file imported with `-l` stays where it is, and one imported with `-m` is not deleted from the library either.

## Testing

```sh
cargo test          # 333: wrapping, pacing, parsing, import modes, reading, whole frames,
                    # illustrations, read-aloud, the device allowlist, search and jumps,
                    # chapter shape, emphasis, bookmarks, the three modes, effort levels
```

- `tests/render.rs` draws a real `App` through ratatui's `TestBackend` and asserts on the screen, so "is the library offered", "did typing 2 open the second book", "does esc interrupt without discarding" and "does `^c` discard" all have regression cover.
- `tests/select.rs` pins the two rules the select lives by: it keeps its narrowing text out of the composer, a `/` always escapes it into a command, a filter that would empty the list is refused, choosing a row records the result rather than the command, and a bare `/import` opens the filesystem at `~/` and walks into directories.
- `tests/effort.rs` holds the pace honest: `^r` walks the ladder and the pacer slows with it, a level names what it is worth, `/rate` retunes only the level in force and writes it back, and `/speed` moves the base the multiplier scales.
- `tests/mode.rs` drives the modes the way a reader does: `shift+tab` into auto-scroll and the text starts moving, two passages arrive with nobody pressing anything, manual mode then sits still, `↓` at the bottom fetches more, and an interruption does not silently change the mode.
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

Install it by rename, not by copying over the old one:

```sh
mv target/release/readio ~/.local/bin/readio.new && mv ~/.local/bin/readio.new ~/.local/bin/readio
```

On macOS, `cp` over an existing binary leaves the old inode with a signature that no longer matches its contents, and the kernel answers by killing the process — an upgrade that "installed fine" and then dies with signal 9 and no message. A rename gives the new binary its own inode, and it also means a half-written download can never be left behind under a name someone is about to run. `install.sh` does exactly this, which is why it never had the problem.

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
