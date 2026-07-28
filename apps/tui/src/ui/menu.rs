//! The command menu: what you can type, and what it will do.
//!
//! Typing `/` opens it, typing more filters it, `↑` `↓` move through it, `tab`
//! completes the highlighted row and `⏎` runs it. It exists because a command set
//! nobody can see is a command set nobody uses — and because a coding agent that
//! offers no completion is a coding agent nobody believes.
//!
//! It has two levels. The first offers commands. Once a command with a fixed set
//! of answers has been typed — `/effort `, `/mode `, `/lang `, `/tts ` — the menu
//! offers those answers instead, with the one in force marked, so a reader never
//! has to know that `xhigh` is spelled without a hyphen.
//!
//! Every row carries two descriptions. The short one sits next to the label and
//! has to survive an eighty-column terminal. The long one is printed under the
//! list for the highlighted row alone, with an example, and it is where the
//! arguments, the defaults and the surprises are explained — a menu row is the
//! only documentation most readers will ever read, and "list the library" does not
//! tell them what the listing contains.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::config::Config;
use crate::effort;
use crate::i18n::{Lang, t};
use crate::mode::Mode;
use crate::theme::{self, theme};
use crate::wrap::{display_width, truncate, wrap};

/// One command as the menu presents it: what to type, and what happens.
pub struct Command {
    /// Canonical name, without the slash.
    pub name: &'static str,
    /// Argument shape, shown after the name. Angle brackets mean the command
    /// cannot run without it, so `⏎` completes the name instead of running it;
    /// square brackets mean the bare form does something worth seeing, like
    /// `/rate` printing the ladder of speeds.
    pub args: &'static str,
    /// One line, next to the name.
    pub zh: &'static str,
    pub en: &'static str,
    /// The whole story, printed under the list when this row is highlighted.
    pub zh_more: &'static str,
    pub en_more: &'static str,
    /// A real invocation, so the argument stops being an abstraction. Empty when
    /// the command takes none and the name is the whole example.
    pub example: &'static str,
}

impl Command {
    /// `/find <term>`
    pub fn label(&self) -> String {
        if self.args.is_empty() {
            format!("/{}", self.name)
        } else {
            format!("/{} {}", self.name, self.args)
        }
    }

    /// The line beside the name.
    pub fn about(&self) -> &'static str {
        match crate::i18n::current() {
            Lang::Zh => self.zh,
            Lang::En => self.en,
        }
    }

    /// The paragraph under the list, for this row alone.
    pub fn more(&self) -> &'static str {
        match crate::i18n::current() {
            Lang::Zh => self.zh_more,
            Lang::En => self.en_more,
        }
    }

    /// Whether `⏎` on this row can run the command as it stands. A required
    /// argument means the row completes instead.
    fn runnable(&self) -> bool {
        self.args.is_empty() || self.args.starts_with('[')
    }

    fn row(&'static self) -> Row {
        Row {
            label: self.label(),
            about: self.about().to_string(),
            more: self.more().to_string(),
            example: if self.example.is_empty() {
                format!("/{}", self.name)
            } else {
                self.example.to_string()
            },
            also: self.aliases().to_vec(),
            insert: if self.args.is_empty() {
                format!("/{}", self.name)
            } else {
                format!("/{} ", self.name)
            },
            run: self.runnable(),
        }
    }
}

/// One offered thing: a command, or an answer to a command.
///
/// Built per frame rather than declared, because half of what a value row says —
/// which level is active, what multiplier it stands for, which engines are
/// installed — is a fact about the reader's config, not about readio.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// What the row is called: `/effort <level>`, or `Minimal (active)`.
    pub label: String,
    pub about: String,
    /// The paragraph shown when this row is highlighted.
    pub more: String,
    /// One line showing the thing in use.
    pub example: String,
    /// Other spellings the dispatcher accepts.
    pub also: Vec<&'static str>,
    /// What `tab` writes into the prompt.
    pub insert: String,
    /// Whether `⏎` runs it now, or only completes it.
    pub run: bool,
}

/// What the menu needs to know about the reader to mark the active answer.
pub struct Ctx<'a> {
    pub cfg: &'a Config,
    pub mode: Mode,
    /// Base characters per second, before the effort multiplier.
    pub base_cps: f32,
}

/// The rows a prompt line should offer, in menu order. Empty means no menu.
pub fn offer(line: &str, ctx: &Ctx<'_>) -> Vec<Row> {
    let Some(body) = line.strip_prefix('/') else {
        return Vec::new();
    };
    match body.split_once(' ') {
        // Still naming a command.
        None => matching(body).into_iter().map(Command::row).collect(),
        // The command is named; if it has a fixed set of answers, offer those.
        Some((name, typed)) => match find(name) {
            Some(command) => values(command, typed.trim(), ctx),
            None => Vec::new(),
        },
    }
}

/// The command a name or alias refers to.
fn find(name: &str) -> Option<&'static Command> {
    let name = name.trim().to_ascii_lowercase();
    COMMANDS
        .iter()
        .find(|c| c.name == name || c.aliases().contains(&name.as_str()))
}

/// Commands a query offers, in menu order.
///
/// A query matches on the start of the name first — typing `ma` should put
/// `/mark` above `/marks` and not bury either under something that merely
/// contains the letters — and then on any position, so `/dev` finds `/device`
/// and `/out` finds it too.
pub fn matching(query: &str) -> Vec<&'static Command> {
    let needle = query.trim_start_matches('/').trim().to_ascii_lowercase();
    let needle = needle.split_whitespace().next().unwrap_or("").to_string();
    if needle.is_empty() {
        return COMMANDS.iter().collect();
    }
    let mut starts: Vec<&'static Command> = Vec::new();
    let mut contains: Vec<&'static Command> = Vec::new();
    for command in COMMANDS {
        if command.name.starts_with(&needle) {
            starts.push(command);
        } else if command.name.contains(&needle) || command.aliases().contains(&needle.as_str()) {
            contains.push(command);
        }
    }
    starts.extend(contains);
    starts
}

/// The answers a command accepts, filtered by what has been typed so far.
fn values(command: &'static Command, typed: &str, ctx: &Ctx<'_>) -> Vec<Row> {
    let rows = match command.name {
        "effort" => effort_rows(ctx),
        "mode" => mode_rows(ctx),
        "lang" => lang_rows(ctx),
        "tts" => tts_rows(ctx),
        // A path, a chapter number or a search term: nothing to suggest, and a
        // menu in the way of typing one is worse than no menu.
        _ => Vec::new(),
    };
    if typed.is_empty() {
        return rows;
    }
    let needle = typed.to_ascii_lowercase();
    rows.into_iter()
        .filter(|row| {
            let value = row.insert.rsplit(' ').next().unwrap_or_default();
            value.starts_with(&needle) || row.label.to_ascii_lowercase().starts_with(&needle)
        })
        .collect()
}

/// One answer row, with the `(active)` marker a reader needs to see the state
/// they are changing.
fn value(name: &str, code: &str, about: String, more: String, active: bool) -> Row {
    let label = if active {
        format!("{name} {}", t("menu.active"))
    } else {
        name.to_string()
    };
    Row {
        label,
        about,
        more,
        example: format!("/{code}"),
        also: Vec::new(),
        insert: format!("/{code}"),
        run: true,
    }
}

fn effort_rows(ctx: &Ctx<'_>) -> Vec<Row> {
    effort::LEVELS
        .iter()
        .map(|level| {
            let multiplier = ctx.cfg.effort.multipliers.get(*level);
            let cps = (ctx.base_cps * multiplier).round() as usize;
            let more = crate::i18n::tf(
                "effort.row_more",
                &[
                    &level.label(),
                    &effort::times(multiplier),
                    &level.about(),
                    &crate::metrics::rate_label(cps as f32),
                ],
            );
            value(
                level.label(),
                &format!("effort {}", level.code()),
                // Right-aligned, so six multipliers read as a column rather than
                // as six different sentences.
                format!("{:>5}   {}", effort::times(multiplier), level.about()),
                more,
                *level == ctx.cfg.effort.level,
            )
        })
        .collect()
}

fn mode_rows(ctx: &Ctx<'_>) -> Vec<Row> {
    [Mode::Manual, Mode::Auto, Mode::Speak]
        .iter()
        .map(|mode| {
            value(
                mode.name(),
                &format!("mode {}", mode.code()),
                t(match mode {
                    Mode::Manual => "mode.row_manual",
                    Mode::Auto => "mode.row_auto",
                    Mode::Speak => "mode.row_tts",
                })
                .to_string(),
                t(match mode {
                    Mode::Manual => "mode.set_manual",
                    Mode::Auto => "mode.row_auto_more",
                    Mode::Speak => "mode.row_tts_more",
                })
                .to_string(),
                *mode == ctx.mode,
            )
        })
        .collect()
}

fn lang_rows(ctx: &Ctx<'_>) -> Vec<Row> {
    [Lang::En, Lang::Zh]
        .iter()
        .map(|lang| {
            value(
                match lang {
                    Lang::En => "English",
                    Lang::Zh => "中文",
                },
                &format!("lang {}", lang.code()),
                t("cmd.lang_row").to_string(),
                t("cmd.lang_row_more").to_string(),
                *lang == ctx.cfg.language,
            )
        })
        .collect()
}

/// `/tts` answers: the switches, then every engine the config knows about, so a
/// reader never has to remember whether theirs is called `piper` or `piper-tts`.
fn tts_rows(ctx: &Ctx<'_>) -> Vec<Row> {
    let speaking = ctx.cfg.tts.enabled;
    let mut rows = vec![
        value(
            "on",
            "tts on",
            t("tts.row_on").to_string(),
            t("tts.row_on_more").to_string(),
            speaking,
        ),
        value(
            "off",
            "tts off",
            t("tts.row_off").to_string(),
            t("tts.row_off_more").to_string(),
            !speaking,
        ),
        value(
            "test",
            "tts test",
            t("tts.row_test").to_string(),
            t("tts.row_test_more").to_string(),
            false,
        ),
        value(
            "config",
            "tts config",
            t("tts.row_config").to_string(),
            t("tts.row_config_more").to_string(),
            false,
        ),
    ];
    for name in ctx.cfg.engine_names() {
        let active = name == ctx.cfg.tts.engine;
        let spec = ctx.cfg.spec(&name);
        // The program that would actually run: the thing a reader needs to have
        // installed, which is the only fact about an engine worth a menu row.
        let about = spec
            .and_then(|s| s.synth.split_whitespace().next().map(str::to_string))
            .unwrap_or_else(|| name.clone());
        rows.push(value(
            &name,
            &format!("tts {name}"),
            crate::i18n::tf("tts.row_engine", &[&about]),
            crate::i18n::tf("tts.row_engine_more", &[&name, &about]),
            active,
        ));
    }
    rows
}

/// Whether the prompt is asking for the menu at all.
pub fn wanted(line: &str, ctx: &Ctx<'_>) -> bool {
    !offer(line, ctx).is_empty()
}

/// How many rows fit before the list starts scrolling.
pub const VISIBLE: usize = 7;

/// Longest the detail paragraph is allowed to run, in lines. Four is enough for
/// every description here down to about seventy columns; past that the tail is
/// clipped with an ellipsis rather than silently dropped.
const MORE_LINES: usize = 4;

/// Height the menu needs: the list, the detail panel for the highlighted row,
/// and one line of key hints.
///
/// The width matters because the detail paragraph is wrapped, and a narrow
/// terminal turns two lines into four.
pub fn height(rows: &[Row], selected: usize, width: u16) -> u16 {
    if rows.is_empty() {
        return 0;
    }
    let list = rows.len().min(VISIBLE) as u16;
    list + detail_height(rows, selected, width) + 1
}

/// Lines the detail panel takes, blank separator included.
fn detail_height(rows: &[Row], selected: usize, width: u16) -> u16 {
    let Some(row) = rows.get(selected.min(rows.len().saturating_sub(1))) else {
        return 0;
    };
    let body = wrap(&row.more, text_width(width)).len().min(MORE_LINES);
    // A blank line to detach the paragraph from the list, the paragraph itself,
    // and the example.
    1 + body as u16 + 1
}

/// Columns the detail paragraph may use, after the indent.
fn text_width(width: u16) -> usize {
    (width as usize).saturating_sub(6).max(20)
}

/// Draw the menu, with `selected` highlighted, the list scrolled to keep it in
/// view, and the highlighted row explained underneath.
pub fn render(area: Rect, buf: &mut Buffer, rows: &[Row], selected: usize) {
    if rows.is_empty() || area.height == 0 {
        return;
    }
    let th = theme();
    let selected = selected.min(rows.len() - 1);
    let width = area.width as usize;

    // The list gets whatever the hint line and the detail panel leave behind, so
    // a short terminal loses rows rather than losing the explanation.
    let reserved = 1 + detail_height(rows, selected, area.width);
    let room = (area.height.saturating_sub(reserved) as usize).clamp(1, VISIBLE);
    let shown = room.min(rows.len());
    let first = selected.saturating_sub(shown - 1).min(rows.len() - shown);
    let window = &rows[first..first + shown];

    let name_width = window
        .iter()
        .map(|row| display_width(&row.label))
        .max()
        .unwrap_or(12)
        .clamp(12, 30);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(window.len() + MORE_LINES + 3);
    for (offset, row) in window.iter().enumerate() {
        let current = first + offset == selected;
        let label_w = display_width(&row.label);
        let pad = name_width.saturating_sub(label_w) + 2;
        // A label wider than the column — `/tts [on|off|…]` — pushes its own
        // description right, so measure per row rather than per menu.
        let about_width = width.saturating_sub(label_w.max(name_width) + 6).max(10);
        let (marker, name_style, about_style) = if current {
            (
                theme::ARROW,
                Style::default()
                    .fg(th.text_primary)
                    .bg(th.bg_selected)
                    .add_modifier(Modifier::BOLD),
                Style::default().fg(th.text_secondary).bg(th.bg_selected),
            )
        } else {
            (
                " ",
                Style::default().fg(th.accent_agent),
                Style::default().fg(th.text_faint),
            )
        };
        let fill = if current {
            Style::default().bg(th.bg_selected)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {marker} "), fill.fg(th.accent_agent)),
            Span::styled(row.label.clone(), name_style),
            Span::styled(" ".repeat(pad), fill),
            Span::styled(truncate(&row.about, about_width), about_style),
        ]));
    }

    lines.extend(detail(&rows[selected], area.width));

    // One faint line of instructions, because the keys are not guessable.
    let more = rows.len().saturating_sub(window.len());
    let hint = if more > 0 {
        crate::i18n::tf("menu.hint_more", &[&more])
    } else {
        crate::i18n::t("menu.hint").to_string()
    };
    lines.push(Line::from(Span::styled(
        format!("   {hint}"),
        Style::default().fg(th.text_faint),
    )));

    Paragraph::new(lines).render(area, buf);
}

/// The panel under the list: what the highlighted row actually does, and one line
/// showing it in use.
fn detail(row: &Row, width: u16) -> Vec<Line<'static>> {
    let th = theme();
    let mut out = vec![Line::from("")];
    let mut body = wrap(&row.more, text_width(width));
    let clipped = body.len() > MORE_LINES;
    body.truncate(MORE_LINES);
    if clipped {
        // A sentence that just stops looks like a bug; an ellipsis looks like a
        // decision.
        if let Some(last) = body.last_mut() {
            last.push('…');
        }
    }
    for line in body {
        out.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(line, Style::default().fg(th.text_secondary)),
        ]));
    }

    let mut tail = format!("{}  {}", crate::i18n::t("menu.example"), row.example);
    if !row.also.is_empty() {
        let spellings: Vec<String> = row.also.iter().map(|a| format!("/{a}")).collect();
        tail.push_str(&format!(
            "  ·  {} {}",
            crate::i18n::t("menu.also"),
            spellings.join(" ")
        ));
    }
    out.push(Line::from(vec![
        Span::raw("   "),
        Span::styled(
            truncate(&tail, text_width(width)),
            Style::default().fg(th.text_faint),
        ),
    ]));
    out
}

/// Every command readio answers to, in the order a reader meets them.
#[rustfmt::skip]
pub static COMMANDS: &[Command] = &[
    Command {
        name: "lib", args: "",
        zh: "书库里所有导入过的书，带序号",
        en: "Everything you have imported, numbered",
        zh_more: "列出书库里的每一本：序号、篇幅、读到几成，以及 readio 是留了副本、只记住路径，还是把原文件搬了进来。序号就是 /open 和 /forget 要的那个。",
        en_more: "Lists every book with its number, length, how far you have read, and whether readio keeps a copy, only remembers the path, or took the file in. The numbers are what /open and /forget expect.",
        example: "",
    },
    Command {
        name: "open", args: "<n>",
        zh: "打开第 n 本，回到上次停下的地方",
        en: "Open book n where you stopped",
        zh_more: "打开 /lib 里编号为 n 的书，并按字符恢复到上次停下的位置，不只是回到那一章。书库在屏幕上时，直接输数字也一样。",
        en_more: "Opens the numbered entry from /lib and restores your place to the character, not merely to the chapter. While the library is on screen a bare number does the same.",
        example: "/open 3",
    },
    Command {
        name: "import", args: "<path>",
        zh: "收一个文件进书库（epub / txt / md / pdf）",
        en: "Take a file into the library (epub, txt, md, pdf)",
        zh_more: "三种持有方式：--copy 是默认，复制一份到 ~/.readio/books，原文件挪走也不影响；--link 只记住路径，省空间但别乱动原文件；--move 把文件搬进书库，原处不留。",
        en_more: "Three ways to hold it: --copy, the default, puts a copy under ~/.readio/books, so moving the original changes nothing; --link only remembers where it lives; --move takes the file in.",
        example: "/import ~/Downloads/calvino.epub --link",
    },
    Command {
        name: "forget", args: "<n>",
        zh: "把第 n 本从书库里移除",
        en: "Remove entry n from the library",
        zh_more: "移除这一条和它的阅读进度。复制或搬进来的书，删的是书库自己那份；只记路径的书，你的原文件一根手指也不碰。",
        en_more: "Removes the entry and its reading position. For a copied or moved book the library's own copy goes; for a linked book your file is never touched.",
        example: "/forget 3",
    },
    Command {
        name: "sample", args: "",
        zh: "打开内置的示例读物",
        en: "Open the built-in sample text",
        zh_more: "打开一段随程序打包的短文，手边没有书也能先试试吐字速度、朗读和搜索是什么感觉。",
        en_more: "Opens a short piece that ships inside the binary, so you can try the reveal speed, read-aloud and search before importing anything of your own.",
        example: "",
    },
    Command {
        name: "toc", args: "",
        zh: "这本书自己的目录",
        en: "The book's own table of contents",
        zh_more: "按书的导航文件列出章节——出版方怎么分、怎么起的标题，这里就怎么显示；没标题的段落用编号代替。序号是 /goto 要的那个。",
        en_more: "Lists the chapters as the book's navigation document defines them, with the publisher's own divisions and titles; untitled sections get numbers instead. These numbers are what /goto takes.",
        example: "",
    },
    Command {
        name: "goto", args: "<n>",
        zh: "跳到第 n 章的开头",
        en: "Jump to the start of chapter n",
        zh_more: "把阅读位置移到第 n 章的第一段并存下来。序号来自 /toc。跳走之前先 /mark 一下，原来那处就还找得回来。",
        en_more: "Moves your position to the first paragraph of chapter n and saves it. The numbers come from /toc; /mark first if you want the old place back.",
        example: "/goto 12",
    },
    Command {
        name: "next", args: "",
        zh: "跳到下一章开头，不管这一章还剩多少",
        en: "Skip ahead to the next chapter",
        zh_more: "直接跳到下一章的第一段。进度是按字符算的，所以跳过去的部分不会被算成读过，读数上老实反映。",
        en_more: "Jumps to the first paragraph of the following chapter, however much of this one is unread. Progress counts characters, so skipping shows up honestly in the readout.",
        example: "",
    },
    Command {
        name: "prev", args: "",
        zh: "回到上一章的开头",
        en: "Back to the start of the previous chapter",
        zh_more: "回到前一章的第一段，而不是你当时离开它的地方——想重读一章，用这个。",
        en_more: "Goes to the first paragraph of the chapter before this one, rather than to where you left it: this is the command for re-reading something.",
        example: "",
    },
    Command {
        name: "find", args: "<term>",
        zh: "全书检索，数出每一章有几处",
        en: "Search the whole book and count the hits",
        zh_more: "忽略大小写和全半角，逐章统计命中数并给一句上下文。之后输序号跳到某一处，^g 往后、^b 往前逐处走。",
        en_more: "Folds case and character width, counts the hits chapter by chapter and shows a line of context. Type a number to jump to one, then ^g and ^b walk forwards and backwards.",
        example: "/find invisible cities",
    },
    Command {
        name: "mark", args: "[note]",
        zh: "记下现在这一处，可以附一句备注",
        en: "Keep this place, with a note if you like",
        zh_more: "存下你正读到的那个字符。写了备注就用备注做标题，没写就取眼前这段开头几个字。标记存在 state.json 里，重新导入同一本书也还在。",
        en_more: "Saves the exact character you are on, labelled with your note or, failing that, with the opening words of the passage on screen. Marks live in state.json and survive a re-import.",
        example: "/mark the passage about maps",
    },
    Command {
        name: "marks", args: "[n]",
        zh: "列出所有标记；带序号就回到那一处",
        en: "List what you kept, or go back to one",
        zh_more: "不带参数时列出每个标记的序号、备注和它在全书的位置；带一个数字就跳回那一处。",
        en_more: "With no argument it lists every mark with its number, label and how deep into the book it sits; with a number it takes you back there.",
        example: "/marks 2",
    },
    Command {
        name: "unmark", args: "<n>",
        zh: "删掉第 n 个标记",
        en: "Delete one of the places you kept",
        zh_more: "只删标记，序号按 /marks 列出的来；你正读到哪里不受影响，标记删了也不会把你带走。",
        en_more: "Removes one mark, numbered as /marks lists them. Your reading position is left exactly where it is — deleting a mark never moves you.",
        example: "/unmark 2",
    },
    Command {
        name: "mode", args: "[manual|auto|tts]",
        zh: "三种读法：手动、自动滚动、朗读",
        en: "The three ways to read: manual, auto-scroll, read-aloud",
        zh_more: "手动是你说一段算一段：回车或滚到底按 ↓ 载入下一段。自动滚动按 /speed 的速度自己往下走。朗读自带滚动，速度由人声定，用 /rate 调。shift+tab 就地循环切换，^r 调当前模式的速度。",
        en_more: "Manual waits for you: ⏎ or ↓ at the bottom loads the next passage. Auto-scroll keeps going at the speed /speed sets. Read-aloud scrolls itself and takes its pace from the voice, which /rate changes. shift+tab cycles the three, and ^r changes the speed of whichever is on.",
        example: "/mode auto",
    },
    Command {
        name: "auto", args: "",
        zh: "自动滚动开关，等于在手动与自动之间切",
        en: "Switch auto-scroll on or off",
        zh_more: "在手动和自动滚动之间来回切，等价于 /mode auto 和 /mode manual。速度用 /speed <字/秒> 或 ^r，esc 暂停。",
        en_more: "Flips between manual and auto-scroll, the same as /mode auto and /mode manual. Speed comes from /speed <chars per second> or ^r, and esc pauses.",
        example: "",
    },
    Command {
        name: "plan", args: "",
        zh: "把章节当待办清单看",
        en: "The chapters as a checklist",
        zh_more: "读过的章节打勾，当前那一章标出来，剩下的排队等着并注明篇幅。长书只显示当前附近的几章，其余折成一行计数。",
        en_more: "Read chapters are ticked, the current one is marked, and the rest queue up with their lengths. A long book shows the chapters around you and folds the remainder into a count.",
        example: "",
    },
    Command {
        name: "context", args: "",
        zh: "这次阅读的完整读数",
        en: "The full readout for this session",
        zh_more: "把状态栏塞不下的都打出来：全书读到几成、这一章多长、开着 readio 以来读了多少、当前吐字速度，以及是不是由人声在带。",
        en_more: "Prints what the status line has no room for: how far through the book you are, how long this chapter runs, how much you have read since opening readio, the reveal speed, and whether a voice is setting it.",
        example: "",
    },
    Command {
        name: "progress", args: "",
        zh: "一行进度：百分比、已读 / 全书、第几章第几段",
        en: "One line: percentage, amount, position",
        zh_more: "一行回答“我在哪”：读了全书的百分之几、已读多少 / 共多少，以及现在在第几章第几段。",
        en_more: "One line answering where you are: the percentage of the book, the amount read against the total, and the chapter and paragraph you are in.",
        example: "",
    },
    Command {
        name: "effort", args: "[level]",
        zh: "推理强度，其实就是读得多快多慢",
        en: "Reasoning effort — which is to say, how fast you read",
        zh_more: "六档：minimal 扫读、max 细读，强度越高读得越慢。每档是一个倍数，文字按 speed × 倍数 吐出，朗读直接按这个倍数播放。^r 循环，/rate 改当前档的倍数，六档都写在 config.yaml 里。",
        en_more: "Six levels from minimal (skim) to max (close reading): more effort reads more slowly. Each level is a multiplier — text appears at speed × multiplier and read-aloud plays at the multiplier. ^r cycles, /rate retunes the level you are on, and all six live in config.yaml.",
        example: "/effort xhigh",
    },
    Command {
        name: "speed", args: "<n>",
        zh: "吐字速度，单位字/秒",
        en: "Reveal speed, in characters per second",
        zh_more: "自动滚动和手动模式下文字出现的快慢：20 是慢读，120 比大多数人默读还快。朗读模式下由人声定速，这个值先放着；^r 可以在几档之间循环。",
        en_more: "How fast text appears in manual and auto-scroll: 20 is a slow crawl, 120 is quicker than most people read. In read-aloud the voice owns the pace and this waits its turn; ^r steps through a ladder of speeds.",
        example: "/speed 45",
    },
    Command {
        name: "tts", args: "[on|off|<engine>|test|config]",
        zh: "朗读：开关、换引擎、试一句、找配置",
        en: "Read-aloud: switch it, pick an engine, test it",
        zh_more: "readio 自己不带模型，只调用你配好的命令。on / off 开关，写引擎名（kokoro、piper、supertonic、openai）套用预设，test 念一句验证接线，config 打印 ~/.readio/config.yaml 的位置。",
        en_more: "readio ships no model and only runs the command you configured. on and off switch it, an engine name (kokoro, piper, supertonic, openai) applies a preset, test speaks a line, config prints where config.yaml lives.",
        example: "/tts kokoro",
    },
    Command {
        name: "voice", args: "<name>",
        zh: "换音色，名字由你装的引擎决定",
        en: "Choose which voice speaks",
        zh_more: "把音色名传给你配置的引擎；有哪些名字取决于你装的模型，不是 readio 说了算。切换后从当前这句重新念。",
        en_more: "Passes a voice name through to the engine you configured; which names exist depends on the model you installed, not on readio. Speech resumes from the current sentence.",
        example: "/voice af_heart",
    },
    Command {
        name: "rate", args: "[0.5-3.0]",
        zh: "改当前这一档强度值多少倍",
        en: "Retune what the current effort level is worth",
        zh_more: "强度只是名字，倍数才是内容。/rate 1.2 就是把你正在用的那一档改成 1.2 倍，立刻生效并写回 config.yaml——文字和人声一起变。不带参数会把六档连倍数一起列出来。",
        en_more: "A level is a label; the multiplier behind it is the substance. /rate 1.2 makes the level you are on worth 1.2×, applies it at once and writes it back to config.yaml — text and voice together. With no argument it lists all six levels and their multipliers.",
        example: "/rate 1.2",
    },
    Command {
        name: "device", args: "",
        zh: "音频输出白名单",
        en: "Devices the voice is allowed to use",
        zh_more: "点名允许出声的设备。切到别的输出——比如安静办公室里的笔记本喇叭——readio 就闭嘴，并告诉你为什么，而不是把书念给整间屋子听。",
        en_more: "Names the outputs readio may speak on. Switch to anything else — the laptop speaker in a quiet office, say — and it stays silent and tells you why, instead of reading your book to the room.",
        example: "",
    },
    Command {
        name: "lang", args: "en|zh",
        zh: "界面语言（正文不受影响）",
        en: "Interface language, English or Chinese",
        zh_more: "在中文和英文之间切换界面，并把选择写进 config.yaml。书永远是作者写它时的那种语言。",
        en_more: "Switches the interface between English and Chinese and remembers the choice in config.yaml. A book always stays in the language it was written in.",
        example: "/lang zh",
    },
    Command {
        name: "help", args: "",
        zh: "按键与命令的完整清单",
        en: "Every key and command, on one screen",
        zh_more: "打开速查面板：按键、命令，以及状态栏里那些伪装过的读数到底是什么意思。按 ? 也能开，esc 关掉。",
        en_more: "Opens the reference overlay: the keys, the commands, and what the disguised readouts in the status line actually mean. ? opens it too; esc closes it.",
        example: "",
    },
    Command {
        name: "quit", args: "",
        zh: "退出 readio",
        en: "Leave readio, with your place saved",
        zh_more: "关掉 readio。什么都不会丢——位置是边读边写进 ~/.readio/state.json 的，不是退出时才存。^c 和 ^d 同理。",
        en_more: "Closes readio. Nothing is lost: your position is written to ~/.readio/state.json as you read, not on the way out. ^c and ^d do the same.",
        example: "",
    },
];

impl Command {
    /// Spellings the dispatcher also accepts, so the menu can find a command by
    /// the name the reader remembers.
    fn aliases(&self) -> &'static [&'static str] {
        match self.name {
            "lib" => &["library", "ls"],
            "find" => &["grep", "search"],
            "tts" => &["speak", "read"],
            "device" => &["devices", "audio", "output"],
            "marks" => &["bookmarks"],
            "quit" => &["exit", "q"],
            "lang" => &["language"],
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effort::Effort;
    use ratatui::layout::Rect;

    fn ctx(cfg: &Config) -> Ctx<'_> {
        Ctx {
            cfg,
            mode: Mode::Manual,
            base_cps: 46.0,
        }
    }

    fn draw(width: u16, height_: u16, rows: &[Row], selected: usize) -> String {
        let area = Rect::new(0, 0, width, height_);
        let mut buf = Buffer::empty(area);
        render(area, &mut buf, rows, selected);
        (0..height_)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_bare_slash_offers_everything() {
        let cfg = Config::default();
        assert_eq!(offer("/", &ctx(&cfg)).len(), COMMANDS.len());
    }

    #[test]
    fn a_prefix_comes_before_a_mere_containment() {
        let rows = matching("/mark");
        assert_eq!(rows.first().map(|c| c.name), Some("mark"));
        assert!(rows.iter().any(|c| c.name == "marks"));
        assert!(
            !rows.iter().any(|c| c.name == "lib"),
            "an unrelated command should not be offered"
        );
    }

    #[test]
    fn an_alias_finds_its_command() {
        assert!(matching("/grep").iter().any(|c| c.name == "find"));
        assert!(matching("/library").iter().any(|c| c.name == "lib"));
    }

    #[test]
    fn a_command_that_takes_a_path_closes_the_menu() {
        let cfg = Config::default();
        let ctx = ctx(&cfg);
        assert!(wanted("/im", &ctx));
        assert!(!wanted("/import ", &ctx), "nothing to suggest for a path");
        assert!(!wanted("/import ~/book.epub", &ctx));
        assert!(!wanted("what is this", &ctx));
    }

    #[test]
    fn every_command_explains_itself_twice_in_both_languages() {
        for command in COMMANDS {
            assert!(
                command.zh.chars().count() >= 6,
                "{} zh too terse",
                command.name
            );
            assert!(
                command.en.split_whitespace().count() >= 4,
                "{} en too terse",
                command.name
            );
            // The detail panel is the documentation, so it has to say something
            // the one-liner did not.
            assert!(
                command.zh_more.chars().count() >= 30,
                "{} zh_more too terse",
                command.name
            );
            assert!(
                command.en_more.split_whitespace().count() >= 15,
                "{} en_more too terse",
                command.name
            );
            assert!(
                command.example.is_empty()
                    || command.example.starts_with(&format!("/{}", command.name)),
                "{} example should invoke the command",
                command.name
            );
        }
    }

    #[test]
    fn the_detail_panel_explains_the_highlighted_row() {
        crate::i18n::set(Lang::En);
        let cfg = Config::default();
        let rows = offer("/import", &ctx(&cfg));
        let screen = draw(96, height(&rows, 0, 96), &rows, 0);
        assert!(screen.contains("~/.readio/books"), "{screen}");
        assert!(
            screen.contains("/import ~/Downloads/calvino.epub --link"),
            "{screen}"
        );
    }

    #[test]
    fn a_narrow_terminal_loses_rows_before_it_loses_the_explanation() {
        crate::i18n::set(Lang::En);
        let cfg = Config::default();
        let rows = offer("/", &ctx(&cfg));
        // Half the height the menu would like: the paragraph and the hint stay.
        let screen = draw(60, 6, &rows, 0);
        assert!(screen.contains("Lists every book"), "{screen}");
        assert!(
            !screen.lines().any(|l| display_width(l) > 60),
            "nothing may overflow the width"
        );
    }

    #[test]
    fn aliases_are_offered_next_to_the_example() {
        crate::i18n::set(Lang::En);
        let cfg = Config::default();
        let rows = offer("/lib", &ctx(&cfg));
        let screen = draw(96, height(&rows, 0, 96), &rows, 0);
        assert!(screen.contains("/library"), "{screen}");
    }

    #[test]
    fn a_second_level_offers_every_effort_level_with_the_active_one_marked() {
        crate::i18n::set(Lang::En);
        let mut cfg = Config::default();
        cfg.effort.level = Effort::Medium;
        let rows = offer("/effort ", &ctx(&cfg));

        assert_eq!(rows.len(), effort::LEVELS.len());
        assert_eq!(rows[0].label, "Minimal");
        assert!(rows[2].label.contains("Medium"), "{:?}", rows[2].label);
        assert!(rows[2].label.contains("active"), "{:?}", rows[2].label);
        // A value row runs as it stands, and says what it will do.
        assert!(rows.iter().all(|row| row.run));
        assert_eq!(rows[5].insert, "/effort max");
        assert!(rows[0].about.contains("2.5×"), "{:?}", rows[0].about);
    }

    #[test]
    fn typing_part_of_a_level_narrows_it() {
        let cfg = Config::default();
        let rows = offer("/effort x", &ctx(&cfg));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].insert, "/effort xhigh");
    }

    #[test]
    fn the_multipliers_a_reader_configured_are_what_the_menu_shows() {
        crate::i18n::set(Lang::En);
        let mut cfg = Config::default();
        cfg.effort.multipliers.set(Effort::Max, 0.6);
        let rows = offer("/effort ", &ctx(&cfg));
        let max = rows.last().expect("a row for max");
        assert!(max.about.contains("0.6×"), "{:?}", max.about);
        let screen = draw(96, height(&rows, 5, 96), &rows, 5);
        assert!(screen.contains("0.6×"), "{screen}");
    }

    #[test]
    fn the_other_second_levels_answer_too() {
        crate::i18n::set(Lang::En);
        let cfg = Config::default();
        let ctx = ctx(&cfg);

        let modes = offer("/mode ", &ctx);
        assert_eq!(modes.len(), 3);
        assert!(modes[0].label.contains("active"), "manual is the default");

        let langs = offer("/lang ", &ctx);
        assert_eq!(langs.len(), 2);
        assert!(langs.iter().any(|row| row.insert == "/lang zh"));

        // Switches first, then every engine the config knows about.
        let tts = offer("/tts ", &ctx);
        assert!(tts.len() > 4);
        assert_eq!(tts[0].insert, "/tts on");
        assert!(tts.iter().any(|row| row.insert == "/tts kokoro"));
    }
}
