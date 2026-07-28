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
| Characters | tokens | `1.2k tok` |
| Progress through the book | context window used | `ctx 41.8%` |
| Chapters | a plan, with ticks | `/plan` |
| Time since launch | session clock | `12:03` |
| Paused | `thinking…` | status row |
| Reading pace | **reasoning effort** | `readio-1 (high)` |
| Unattended reading | the mode `shift+tab` cycles | `⏵⏵ auto` |
| Latin words inside Chinese prose | identifiers, tinted | body text |

Two things are deliberately *not* disguised: the book's own words, and any number a reader might act on. A fake line range would make the costume a lie the moment someone opened the file.

There is no musical note anywhere. A `♪` in the corner announces a media player, which is the one thing the interface must never look like; read-aloud is `⏵⏵ voice` and the engine name appears where a model name would.

## Screen anatomy

```
 readio   看不见的城市  卡尔维诺                         第 3/72 章  ·  ctx 1.3%   ← header
                                                                                
 ● Read epub://看不见的城市/index_split_002.html  L1-17  ·  0.4s                  ← scrollback
   城市、文学与历史                                                               
   —— 阅读《看不见的城市》                                                        
                                                                                
 ❯ /effort <level>   推理强度，其实就是读得多快多慢                               ← menu (only when /)
   …                                                                            
╭──────────────────────────────────────────────────────────────────────╮        
│❯ 回车继续读，/ 看命令，esc 暂停                                       │        ← prompt
╰──────────────────────────────────────────────────────────────────────╯        
  ⏎ 继续  ·  shift+tab 换模式  ·  /help 更多     ⏵ 逐段  ⸬ readio-1 (high)  74 tok  ·  0:10
  ↑ what to do next                              ↑ mode   ↑ model + effort   ↑ session
```

The status row is the only permanent teacher. Its left half says what to do next in the current state; its right half says what state that is. Both halves are narrow on purpose: the left is truncated before the right, and the right is capped at twelve columns for the mode chip, because a hint cut in half is worse than a hint that fits.

## Reading modes

Three, because a reader is doing one of three things.

| Mode | Chip | What advances the text |
| --- | --- | --- |
| Manual | `⏵ step` | `⏎`, or `↓` `pgdn` wheel once at the bottom |
| Auto-scroll | `⏵⏵ auto` | nothing — passages follow one another |
| Read-aloud | `⏵⏵ voice` | the voice, which brings its own scrolling |

`shift+tab` cycles them, which is the gesture a coding agent uses for exactly this kind of switch. `/mode` says the same thing in words, and the second level of its menu offers the three with the current one marked.

Rules that follow from calling it a mode:

- **A mode is a setting, not a consequence.** `esc` pauses, `esc` again stops the turn; neither demotes auto-scroll to manual.
- **Read-aloud implies auto-scroll.** Turning the voice off lands in auto-scroll, not manual — silence is all the reader asked for.
- **A mode that cannot work is skipped, not entered.** No speech engine means `shift+tab` steps past read-aloud, having said why once.
- **A mode explains itself the first time and flashes afterwards.** Cycling should not reprint a paragraph.

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

The numbers belong to the reader. All six live under `effort.multipliers` in `~/.readio/config.yaml`, `/rate <0.5-3.0>` retunes the level in force without opening the file, and `/speed <n>` moves the base the multipliers scale. `/effort` with no argument prints the whole ladder with the active level marked, which is the one place the honest numbers and the costume sit side by side.

## The command menu

Two levels, because a value is as hard to remember as a command.

1. `/` and a partial name → commands, filtered by prefix first and then by containment, each with a one-line description.
2. `/name ` for a command with a fixed set of answers — `/effort`, `/mode`, `/lang`, `/tts` — → those answers, each with what it means, and `(active)` on the one in force.

Anything else after a command name closes the menu: a path, a chapter number and a search term have nothing to suggest, and a menu in the way of typing one is worse than no menu.

Under the list, the highlighted row is explained in full and shown in use. That panel is where `--copy` versus `--link` versus `--move` gets settled, and it is sized before the list is: a short terminal loses rows, never the explanation.

`↑` `↓` choose, `tab` completes, `⏎` runs — or completes, when the command cannot run without an argument. The bracket in the argument shape decides: `<n>` completes, `[n]` runs.

## Keys, by state

The same key may mean different things in different states, but never two things in the same state. This table is the whole input grammar.

| Key | Menu open | Streaming | Paused | Idle with a book | Library |
| --- | --- | --- | --- | --- | --- |
| `⏎` | run or complete the row | rush this turn to its end | resume where it stopped | load the next passage | open the last book |
| `esc` | close the menu | pause | stop the turn | clear the prompt, else to the tail | as idle |
| `↑` `↓` | move the selection | scroll | scroll | scroll; `↓` at the tail loads more in manual mode | scroll |
| `tab` | complete the row | — | — | — | — |
| `shift+tab` | cycle the mode | cycle the mode | cycle the mode | cycle the mode | cycle the mode |
| `/` | filter further | — | — | open the menu | open the menu |
| digits | filter | — | — | jump to a search hit | open that book |
| other text | filter | — | — | nothing, with a hint | nothing, with a hint |
| `^r` | next effort level | next effort level | next effort level | next effort level | next effort level |
| `^s` | — | read-aloud on or off | same | same | same |
| `^c` | clear | interrupt | interrupt | clear, then quit on the second press | same |

Text that is not a command does nothing. It used to be read as a question and searched for, which turned a mistyped `2` into a `Grep` across eighty-five paragraphs; now the line stays in the prompt and the status row says commands start with a slash.

## Discovery

A reader should be able to learn readio without reading anything, in this order:

1. **The status row** always names the next useful key: `⏎ 继续 · shift+tab 换模式 · /help 更多` when idle, `esc 暂停 · ↑↓ 滚动` while streaming, `⏎ 继续 · esc 停止这一轮` while paused.
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
