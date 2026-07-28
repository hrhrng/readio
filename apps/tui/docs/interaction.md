# readio — interaction design

<sub>Why the interface behaves the way it does. The [implementation guide](../README.md) says how the code is built; this file says what the thing is supposed to feel like, and it is the document to argue with before changing a key binding.</sub>

## The one rule

**Looking like a coding agent outranks everything.** Not as a joke: it is the feature. Someone reading a novel in an office should be able to leave the window open, and a colleague glancing over should see a tool that streams text, calls tools, thinks, and reports token counts.

Every other rule in this document is downstream of that one. Where reader comfort and the costume disagree, the costume wins — but they disagree far less often than one would expect, because a coding agent's interface is already a good interface: one input line, discoverable commands, a status row that never lies, and a stream you can interrupt.

## What is disguised as what

| What it really is | What the screen calls it | Where |
| --- | --- | --- |
| Passage of a book | streamed model output | scrollback |
| Loading the next passage | a `Read` tool call with a real file and line range | `● Read epub://…/index_split_002.html  L1-17` |
| Full-text search | a `Grep` call with a real match count | `● Grep pattern: memory · 45 matches` |
| Saving a bookmark or position | a `Write` call | `● Write ~/.readio/state.json` |
| Installing a speech engine | a `Bash` call — and this one is real | `● Bash uv tool install kokoro-tts  ·  24.8s` |
| Characters | tokens | `1.2k tok` |
| Progress through the book | context window used | `ctx 41.8%` |
| Chapters | a plan, with ticks | `/plan` |
| Time since launch | session clock | `12:03` |
| An interrupted turn | a turn the user stopped | `❙ 已中断` above the prompt |
| Reading pace | **reasoning effort** | `readio-1 (high)` |
| Unattended reading | the mode `shift+tab` cycles | `⏵⏵ auto` |
| Latin words inside Chinese prose | identifiers, tinted | body text |

Two things are deliberately *not* disguised: the book's own words, and any number a reader might act on. A fake line range would make the costume a lie the moment someone opened the file.

There is no musical note anywhere. A `♪` in the corner announces a media player, which is the one thing the interface must never look like; read-aloud is `⏵⏵ aloud` and the engine name appears where a model name would.

## Screen anatomy

```
 readio   看不见的城市  卡尔维诺                         第 3/72 章  ·  ctx 1.3%   ← header
                                                                                
 ● Read epub://看不见的城市/index_split_002.html  L1-17  ·  0.4s                  ← scrollback
   城市、文学与历史                                                               
   —— 阅读《看不见的城市》                                                        
                                                                                
 ❯ /effort <level>   推理强度，其实就是读得多快多慢                               ← menu (only when /)
   …                                                                            
╭──────────────────────────────────────────────────────────────────────╮        
│❯ 回车继续读，/ 看命令，esc 中断                                       │        ← prompt
╰──────────────────────────────────────────────────────────────────────╯        
  ⏎ 继续  ·  shift+tab 换模式  ·  /help 更多     ⏵ 手动  ⸬ readio-1 (high)  74 tok  ·  0:10
  ↑ what to do next                              ↑ mode   ↑ model + effort   ↑ session
```

The status row is the only permanent teacher. Its left half says what to do next in the current state; its right half says what state that is. Both halves are narrow on purpose: the left is truncated before the right, and the right is capped at twelve columns for the mode chip, because a hint cut in half is worse than a hint that fits.

## Reading modes

Three, because a reader is doing one of three things.

| Mode | Chip | What advances the text |
| --- | --- | --- |
| Manual | `⏵ manual` | `⏎`, or `↓` `pgdn` wheel once at the bottom |
| Auto-scroll | `⏵⏵ auto` | nothing — passages follow one another |
| Read-aloud | `⏵⏵ aloud` | the voice, which brings its own scrolling |

`shift+tab` cycles them, which is the gesture a coding agent uses for exactly this kind of switch. `/mode` says the same thing in words, and the second level of its menu offers the three with the current one marked.

Rules that follow from calling it a mode:

- **A mode is a setting, not a consequence.** `esc` interrupts the turn and `⏎` picks it back up; neither demotes auto-scroll to manual.
- **Read-aloud implies auto-scroll.** Turning the voice off lands in auto-scroll, not manual — silence is all the reader asked for.
- **A mode that cannot work is skipped, not entered.** No speech engine means `shift+tab` steps past read-aloud, having said why once.
- **A mode explains itself the first time and flashes afterwards.** Cycling should not reprint a paragraph.
- **The voice follows the page, not the session.** Which language a sentence is in is read off the sentence, and the engine is given the matching voice and phonemes; a bilingual chapter switches mid-page with nothing to set. A reader who wants one voice throughout pins `tts.language`.
- **In read-aloud the voice is the clock.** The text may not go past the sentence being spoken, and the next passage is not fetched until the voice is finished with this one. See below.

## Pace, as reasoning effort

Six levels, the six a coding agent offers, and the honest consequence of asking for more is kept: **more effort reads more slowly.**

| Level | Default multiplier | What it feels like |
| --- | --- | --- |
| `minimal` | 2.5× | skim, barely pausing |
| `low` | 2× | quick read |
| `medium` | 1.5× | brisk |
| `high` | 1× | normal reading — the default |
| `xhigh` | 0.85× | slow, words stay with you |
| `max` | 0.7× | close reading, sentence by sentence |

One multiplier drives both worlds: text appears at `reading.speed × multiplier`, and read-aloud plays at the multiplier itself. So a level means the same thing whether the book is being typed out or spoken, and `^r` is one key that always means "change how fast I am reading".

The numbers belong to the reader. All six live under `effort.multipliers` in `~/.readio/config.yaml`, `/rate <0.5-3.0>` retunes the level in force without opening the file, and `/speed <n>` moves the base the multipliers scale. `/effort` with no argument opens the ladder as a select, with each level's multiplier beside it and the one in force marked — the one place the honest numbers and the costume sit side by side.

## Pace, when the voice has it

Read-aloud takes the clock off the reveal and gives it to the audio, which needs more than a change of speed: **the text is not allowed past the sentence being spoken.**

Speed alone is not enough because the two are not merely different, they drift. Chinese comes out of Kokoro at about four characters a second and the default reveal is forty-six, so the reveal has ten seconds of work for every second the voice has. Setting the reveal to the clip's own rate once the clip starts — which is what readio used to do — closes the gap only while a clip is playing. It leaves every synthesis wait wide open, and there is a wait before the first sentence of every passage and between every pair of sentences the renderer has not got ahead of. Each one is small. They only ever accumulate in the same direction.

So there are two rules, and neither is about speed:

- **A passage shows nothing until its first clip exists,** and never goes further than the end of the sentence now playing. A wait for audio is a wait on screen too.
- **The next passage is fetched when the voice finishes, not when the text does.** Otherwise the reasoning line and tool call belonging to the next turn arrive underneath a paragraph still being read, scrolling the highlighted sentence out of sight.

Within a sentence the pace is still a pace: whatever is left to show, divided by however long the clip runs. Dividing by what is *left* rather than by the sentence's own length is what lets a reveal that fell behind during a wait catch up over the next sentence instead of trailing the voice for the rest of the chapter.

Every hold releases on its own. The voice stopping — finished, interrupted, switched off, or an engine that died — lifts it, and a frame in which something is held with nothing left to speak lifts it too. A reveal waiting on a voice is one missed event away from a page that never fills, so it is worth more than one guard.

## Where things are shown

Two surfaces, and a rule about which gets what.

**The transcript carries the book, and what happened to it.** Passages, chapter completions, the reading plan, search results, the writes to `state.json`, and an interrupted turn. It is a record of reading, and it is the part a reader scrolls back through.

**Everything else lives on the composer and the surfaces above it.** Which book, which chapter, which bookmark, which effort level, which mode, which engine, which file to import — all of these are *questions*, and a question belongs next to the place you answer it, not in the log of what you have read.

This is why there are no numbered listings any more. Printing sixteen books into the transcript and asking the reader to type `7` back at it is a listing pretending to be a control: it costs a screenful, it goes stale the moment anything changes, and it cannot be navigated. The select above the composer is the control.

```
 ❯ 1. 看不见的城市 （上次在读）  卡尔维诺  ·  6.4万字  ·  37%
   2. 树上的男爵                卡尔维诺  ·  9.1万字  ·   0%
   3. 寒冬夜行人                卡尔维诺  ·  8.7万字  ·  12%

   打开《看不见的城市》，回到上次停下的地方（已读 37%，copy 方式持有）
   例：  /open 1
   ↑↓ 选  ·  ⏎ 确定  ·  直接打字筛选  ·  esc 取消  ·  筛选：卡尔
```

Two rules keep it usable:

- **A select never touches what the reader was typing.** It has its own narrowing text, echoed on its own hint line. Opening one used to write `/effort ` into the composer and read the rows back out of it, which put a command the reader never typed on the line under their cursor.
- **A slash always begins a command.** The library select is open the moment readio starts, so the first thing anyone ever types would otherwise be swallowed by a filter. `/` closes the select and starts a command, in every state.

A path is the exception that proves the rule: `/import` leaves it in the composer. A path is text — the reader may want to edit it, and it takes a `--copy` or `--link` after it — so the filesystem is offered as menu rows that complete into the line, directories first, and only the formats readio can open. `⏎` on a directory walks in; `⏎` on a file imports it.

## The command menu

Two levels, because a value is as hard to remember as a command.

1. `/` and a partial name → commands, filtered by prefix first and then by containment, each with a one-line description.
2. `/name ` for a command with a fixed set of answers — `/effort`, `/mode`, `/lang`, `/tts` — → those answers, each with what it means, and `(active)` on the one in force. For a command whose answers are the reader's own things — `/open`, `/toc`, `/marks` — the answers are their books, chapters and bookmarks. For `/import` they are files and directories, read from disk.

A row is allowed to know things about the machine, and the speech engines are where that matters. `/tts` checks whether each engine's program is actually there — and, for an engine whose voice is a separate download, whether that file is there too — so a row reads `⏎ reads with it` or `⏎ installs it`, and points `⏎` at the command that matches what it says. Nobody has to find out that an engine is missing by choosing it and hearing nothing.

That menu also shows where the line between a command and a mode is drawn. `/tts` has no on and no off: read-aloud is one of the three reading modes, `shift+tab` cycles them, and two controls for one fact are interesting only in the state where they disagree.

A chapter number or a search term suggests nothing: there is nothing to suggest, and a menu in the way of typing one is worse than no menu.

Under the list, the highlighted row is explained in full and shown in use. That panel is where `--copy` versus `--link` versus `--move` gets settled, and it is sized before the list is: a short terminal loses rows, never the explanation.

`↑` `↓` choose, `tab` completes, `⏎` runs — or completes, when the command cannot run without an argument. The bracket in the argument shape decides: `<n>` completes, `[n]` runs. In a select there is nothing to complete, so `tab` and `⏎` both confirm, and typing narrows instead of reaching the composer.

## Interruption

`esc` stops the turn. One press, one meaning, and the screen says what happened:

```
 ● Read epub://看不见的城市/index_split_002.html  L1-17  ·  0.4s
   城市与记忆之三 —— 城市不会讲述它的过去，而是像手纹一样

 ❙ 已中断
╭──────────────────────────────────────────────────────────────────────╮
│❯ 回车继续读，/ 看命令，esc 中断                                       │
╰──────────────────────────────────────────────────────────────────────╯
  ⏎ 继续                                        ⏵⏵ 自动  ⸬ readio-1 (high)  74 tok  ·  0:10
```

The strip sits directly above the composer, where a coding agent puts what it is doing, and it carries no spinner: a spinner means work is happening, and the point of this state is that none is. The place is kept exactly — mid-sentence, mid-passage — and `⏎` carries on from there rather than starting the passage again.

This used to escalate. One press paused, a second abandoned the turn, and the same key therefore did two different things a second apart; the reader had to know which one they were about to get, and the pause was disguised as `thinking…`, which reads as *the machine is busy* rather than *you stopped it*. Now `esc` only interrupts, `⏎` continues, and `^c` — the key that has meant this in every terminal for forty years — is what throws the turn away.

The two outcomes are worded apart, because a transcript that calls both of them "中断" cannot be read back later. `esc` puts `已中断` on the strip above the prompt, where it stays until the reader deals with it; `^c` writes `✗ 这一轮已取消` into the transcript, which is where things that have finished happening go.

## Keys, by state

The same key may mean different things in different states, but never two things in the same state. This table is the whole input grammar.

| Key | Menu open | Select open | Streaming | Interrupted | Idle with a book | Library |
| --- | --- | --- | --- | --- | --- | --- |
| `⏎` | run or complete the row | confirm the row | rush this turn to its end | carry on where it stopped | load the next passage | open the last book |
| `esc` | close the menu | cancel the question | interrupt the turn | — | clear the prompt, else to the tail | as idle |
| `↑` `↓` | move the selection | move the selection | scroll | scroll | scroll; `↓` at the tail loads more in manual mode | scroll |
| `tab` | complete the row | confirm the row | — | — | — | — |
| `shift+tab` | cycle the mode | cycle the mode | cycle the mode | cycle the mode | cycle the mode | cycle the mode |
| `/` | filter further | close it, start a command | — | — | open the menu | open the menu |
| digits | filter | narrow the rows | — | — | jump to a search hit | pick that book |
| other text | filter | narrow the rows | — | — | nothing, with a hint | narrow the rows |
| `backspace` | edit the line | rub out the filter, then close | — | — | edit the line | rub out the filter |
| `^r` | next effort level | next effort level | next effort level | next effort level | next effort level | next effort level |
| `^s` | — | — | read-aloud on or off | same | same | same |
| `^l` | clear the screen | clear the screen | clear the screen | clear the screen | clear the screen | clear the screen |
| `^c` | clear | clear | discard the turn | discard the turn | clear, then quit on the second press | same |

`esc` has one meaning wherever it lands: it takes back the thing that is currently in front of the reader — a menu, a question, or a running turn. It does not escalate. Pressing it twice during a turn interrupts once and then does nothing, because a key that means "interrupt" the first time and "throw the turn away" the second is a key the reader has to time rather than press. Discarding is `^c`, which has meant that in every terminal for forty years.

Text that is not a command does nothing. It used to be read as a question and searched for, which turned a mistyped `2` into a `Grep` across eighty-five paragraphs; now the line stays in the prompt and the status row says commands start with a slash.

## Discovery

A reader should be able to learn readio without reading anything, in this order:

1. **The status row** always names the next useful key: `⏎ 继续 · shift+tab 换模式 · /help 更多` when idle, `esc 中断 · ↑↓ 滚动` while streaming, `⏎ 继续` once interrupted.
2. **The prompt placeholder** repeats the three that matter: enter, `/`, esc.
3. **`/`** shows every command with a description, and one more keystroke shows every value.
4. **`/help`** is the reference: keys, commands, and what the disguised readouts actually mean.
5. **The config file** is written with bilingual comments, so the settings explain themselves in place.

Nothing important is discoverable only through documentation. If a feature can only be found in this file, that is a bug in the interface.

## Vocabulary

- English says **words**, Chinese says **字**; both are approximate and both are labelled as tokens in the costume. Numbers round the way a person would say them: `8.8k`, `1.2M`, `65.9k tok`.
- Latin words and numbers inside Chinese prose are tinted like identifiers in code, because that is what they look like to a coding agent — and because it genuinely helps the eye.
- Emphasis the book asked for (italic, bold) survives to the screen and composes with the read-aloud highlight: an italic phrase being spoken is italic *and* washed.
- Messages say what happened and what to do next, in that order, in one line. Two lines is a paragraph, and a paragraph in the status row is a wall.
