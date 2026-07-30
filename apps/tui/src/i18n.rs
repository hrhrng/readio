//! Interface language.
//!
//! Every user-visible string lives in one table with a Chinese and an English
//! column, looked up at render time so the language can change without a
//! restart. Placeholders are positional (`{0}`, `{1}`), which keeps the table
//! readable and lets a test assert that both columns take the same arguments.
//!
//! Book content is never translated — only readio's own voice.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// The interface language. English is the default; `config.yaml` and `/lang`
/// are the only ways to change it — readio reads no locale variables, so the
/// default has to be the one that is legible to the most people.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Zh,
    #[default]
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }

    /// Parse what a reader might type after `/lang`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "zh_cn" | "cn" | "chinese" | "中文" => Some(Lang::Zh),
            "en" | "en-us" | "en_us" | "english" => Some(Lang::En),
            _ => None,
        }
    }
}

/// The language in use. Process-global because every render reads it and
/// threading it through each call site would be noise. `1` is English, matching
/// [`Lang::default`].
static CURRENT: AtomicU8 = AtomicU8::new(1);

pub fn set(lang: Lang) {
    CURRENT.store(
        match lang {
            Lang::Zh => 0,
            Lang::En => 1,
        },
        Ordering::Relaxed,
    );
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        0 => Lang::Zh,
        _ => Lang::En,
    }
}

/// One lock every test that pins the language must hold.
///
/// The language is global, and `cargo test` runs a binary's tests on many
/// threads, so a test asserting on Chinese output and a test asserting on English
/// output will otherwise take it in turns to fail on a loaded machine. Anything
/// that calls [`set`] takes this first.
#[cfg(test)]
pub(crate) fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Look up a message in the current language.
pub fn t(key: &str) -> &'static str {
    let row = TABLE.iter().find(|(k, _, _)| *k == key);
    match (row, current()) {
        (Some((_, zh, _)), Lang::Zh) => zh,
        (Some((_, _, en)), Lang::En) => en,
        // A missing key is a bug, but showing the key beats showing nothing.
        (None, _) => leak_missing(key),
    }
}

/// Look up a message and substitute `{0}`, `{1}`, … positionally.
pub fn tf(key: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = t(key).to_string();
    for (index, arg) in args.iter().enumerate() {
        let needle = format!("{{{index}}}");
        if out.contains(&needle) {
            out = out.replace(&needle, &arg.to_string());
        }
    }
    out
}

/// `t` returns `&'static str`, so an unknown key has to outlive the call.
fn leak_missing(key: &str) -> &'static str {
    Box::leak(format!("?{key}").into_boxed_str())
}

/// `key`, Chinese, English.
///
/// Grouped by area: `chrome.*` for the frame, `block.*` for scrollback blocks,
/// `cmd.*` for command feedback, `lib.*` for the library, `voice.*` for speech,
/// `flow.*` for the reading narration, `cli.*` for the command line.
#[rustfmt::skip]
pub const TABLE: &[(&str, &str, &str)] = &[
    // ── frame ──
    ("chrome.library", "书库", "Library"),
    ("chrome.library_count", "书库 {0} 本", "{0} in library"),
    ("chrome.chapter", "第 {0}/{1} 章", "ch {0}/{1}"),
    ("chrome.pick_hint", "输入序号选书", "type a number to pick a book"),
    ("chrome.pick_tail", "  ·  /lib 书库  ·  /import <路径>  ·  /help 更多",
        "  ·  /lib library  ·  /import <path>  ·  /help for more"),
    // An empty library has no number to type. Telling someone to pick one is how
    // a first launch turns into a keystroke that does nothing.
    ("chrome.empty_hint", "导入一本：/import <路径>", "import one: /import <path>"),
    ("chrome.empty_tail", "  ·  /sample 看示例  ·  /help 更多",
        "  ·  /sample for the sample  ·  /help for more"),
    ("chrome.continue", "⏎ 继续", "⏎ keep reading"),
    ("chrome.idle_tail", "  ·  shift+tab 换模式  ·  /help 更多",
        "  ·  shift+tab cycles modes  ·  /help for more"),
    ("chrome.busy_tail", "  ·  esc 中断  ·  ↑↓ 滚动", "  ·  esc interrupts  ·  ↑↓ to scroll"),
    // Pausing wears the agent's own clothes: a stopped stream is a model
    // thinking, which is the one state a coding agent is always allowed to be in.
    // esc holds the turn where it is. It is called an interruption because that
    // is what it is, and because it is the word the reader already knows from
    // every agent that stops mid-answer.
    ("chrome.paused", "已中断", "Interrupted"),
    ("chrome.paused_keys", "⏎ 或空格继续", "⏎ or space to continue"),
    ("menu.hint", "↑↓ 选  ·  tab 补全  ·  ⏎ 执行  ·  esc 关掉",
        "↑↓ to choose  ·  tab completes  ·  ⏎ runs it  ·  esc closes"),
    // A select has nothing to complete: the row is the answer, so tab runs it
    // too, and typing narrows the list instead of reaching the composer.
    ("menu.hint_select", "↑↓ 选  ·  ⏎ 确定  ·  直接打字筛选  ·  esc 取消",
        "↑↓ to choose  ·  ⏎ confirms  ·  type to narrow  ·  esc cancels"),
    ("menu.hint_rest", "还有 {0} 条", "{0} more below"),
    ("menu.hint_filter", "筛选：{0}", "filter: {0}"),
    ("menu.example", "例：", "e.g."),
    ("menu.active", "（当前）", "(active)"),
    // A book ⏎ would resume is not a book that is open, and the chapter you are
    // reading is not a setting in force: both need their own word.
    ("menu.last_read", "（上次在读）", "(last read)"),
    ("menu.you_are_here", "（读到这里）", "(you are here)"),
    // Rows built from the reader's own library, book and bookmarks.
    ("menu.book_more", "打开《{0}》，回到上次停下的地方（已读 {1}%，{2} 方式持有）",
        "Open {0} and pick up where you stopped — {1}% read, held by {2}"),
    ("menu.book_missing", "文件找不到了：{0}。/forget 可以把这一条清掉",
        "The file is gone: {0}. /forget clears the entry"),
    ("menu.path_dir", "目录", "folder"),
    ("menu.path_file", "{0}", "{0}"),
    ("menu.path_dir_more", "进入 {0} 继续找。⏎ 或 tab 都能往里走",
        "Step into {0} and keep looking — ⏎ or tab walks in"),
    ("menu.path_file_more", "导入 {0}。默认复制一份进书库（--copy）；加 --link 只记路径，--move 把文件搬进来",
        "Import {0}. It is copied into the library by default (--copy); --link only remembers the path, --move brings the file in"),
    ("menu.chapter_read", "已读过", "read"),
    ("menu.chapter_here", "正在这里", "you are here"),
    ("menu.chapter_ahead", "还没读", "not yet"),
    ("menu.chapter_more", "跳到《{0}》（第 {1} 章）的开头。跳走之前可以先 /mark 记下现在这一处",
        "Jump to the start of {0} (chapter {1}). /mark keeps your current place first"),
    ("menu.mark_more", "回到「{0}」，大约在全书 {1}% 的位置",
        "Go back to “{0}”, about {1}% into the book"),
    ("menu.also", "也可写作", "also"),

    // ── the three parallel reading modes ──
    // Read-aloud owns token pace and never silently falls back to auto.
    //
    // The chip says the same word as the mode wherever it fits. It used to say
    // 逐段 / step for manual, which is a description of what the mode does
    // rather than a name anyone recognises: the first question it got was "这个
    // 逐段是啥意思". A chip has no room to answer that, so it should not raise
    // the question.
    ("mode.manual", "手动", "manual"),
    ("mode.auto", "自动滚动", "auto-scroll"),
    ("mode.aloud", "朗读", "read-aloud"),
    ("mode.chip_manual", "手动", "manual"),
    ("mode.chip_auto", "自动", "auto"),
    ("mode.chip_aloud", "朗读", "aloud"),

    // ── reading pace, worn as reasoning effort ──
    // Higher effort is slower, which is exactly how the real thing behaves, so the
    // costume and the meaning point the same way.
    ("effort.minimal", "扫读，几乎不停", "skim, barely pausing"),
    ("effort.low", "快读", "quick read"),
    ("effort.medium", "偏快，仍跟得上", "brisk, still easy to follow"),
    ("effort.high", "常速阅读", "normal reading pace"),
    ("effort.xhigh", "慢读，字句留得住", "slow, words stay with you"),
    ("effort.max", "细读，一句一句来", "close reading, sentence by sentence"),
    ("effort.set", "推理强度 {0}  ·  {1}", "Reasoning effort {0} · {1}"),
    ("effort.tuned", "{0} 这一档现在是 {1}，已写回 config.yaml",
        "Level {0} is now {1}, written back to config.yaml"),
    ("effort.row_more", "{0}：{1}，{2}。文字约 {3}，朗读也按这个倍数播。倍数可以在 config.yaml 里改，或者 /rate。",
        "{0} is {1} — {2}. Text arrives at about {3}, and read-aloud plays at the same multiplier. Change it in config.yaml, or with /rate."),
    ("effort.usage", "用法：/effort minimal | low | medium | high | xhigh | max（^r 循环切换）",
        "Usage: /effort minimal | low | medium | high | xhigh | max (^r cycles)"),
    ("mode.set_manual", "手动模式：不会自己往下走。⏎ 载入下一段，滚到底按 ↓ 也可以。shift+tab 换模式",
        "Manual: nothing advances on its own. ⏎ loads the next passage, and so does ↓ at the bottom. shift+tab cycles modes"),
    ("mode.set_auto", "自动滚动：一段接一段，当前 {0}。/effort 或 ^r 调速，esc 中断，shift+tab 换模式",
        "Auto-scroll: passage after passage at {0}. /effort or ^r changes the pace, esc interrupts, shift+tab cycles modes"),
    ("mode.row_manual", "你说一段算一段", "nothing moves until you say so"),
    ("mode.row_auto", "一段接一段自己往下走", "passages follow one another by themselves"),
    ("mode.row_aloud", "念出来，token 跟着人声", "spoken aloud, tokens follow the voice"),
    ("mode.row_auto_more", "自动滚动：读完一段接着下一段，速度按当前推理强度。esc 中断，回车接着读；shift+tab 也能切模式。",
        "Auto-scroll: each passage is followed by the next at the pace the current effort level sets. esc interrupts, ⏎ carries on, and shift+tab cycles the modes."),
    ("mode.row_aloud_more", "朗读模式：TTS 是 token 的时钟。声音慢就等，声音不可用就停在朗读模式，绝不退成自动滚动。",
        "Read-aloud: TTS is the token clock. A slow voice is waited for; an unavailable voice stops in read-aloud and never falls back to auto."),
    ("mode.now", "当前是{0}模式（{1}）。shift+tab 循环，或 /mode manual|auto|aloud",
        "Mode: {0} ({1}). shift+tab cycles, or /mode manual|auto|aloud"),
    ("mode.usage", "用法：/mode manual | auto | aloud；也可以 shift+tab 循环切换",
        "Usage: /mode manual | auto | aloud — or press shift+tab to cycle"),
    ("mode.aloud_unavailable", "TTS 不可用，已停在朗读模式；不会降级成自动滚动",
        "TTS is unavailable. Stopped in read-aloud; auto fallback is disabled"),
    ("cmd.needs_slash", "命令以 / 开头，比如 /find。要检索全书就用 /find <词>",
        "Commands start with a slash — /find, say. To search the book: /find <term>"),
    ("chrome.scrolled", "{0} 已上滚 {1}%", "{0} scrolled up {1}%"),
    ("chrome.scrolled_tail", "  ·  end 回到底部", "  ·  end returns to the tail"),
    ("chrome.cps", "{0} 字/秒", "{0} chars/s"),
    ("chrome.session", "  ·  本次 {0}", "  ·  {0} this session"),
    ("chrome.help_title", " 快捷键与命令 ", " Keys and commands "),
    ("chrome.help_close", " 按任意键关闭", " any key closes"),

    // ── prompt placeholders ──
    ("prompt.reading", "回车或空格继续读，/ 看命令，esc 中断",
        "enter or space reads on  ·  / for commands  ·  esc interrupts"),
    ("prompt.pick", "输入序号选书，或 /import <路径> 导入",
        "type a number to pick a book, or /import <path>"),
    ("prompt.empty", "readio <文件> 导入一本，或 /sample 试试内置示例",
        "readio <file> to import, or /sample for the built-in sample"),

    // ── blocks ──
    ("block.thinking", "Thinking", "Thinking"),
    ("block.thought_for", "Thought for {0}s", "Thought for {0}s"),
    ("block.thought", "Thought", "Thought"),
    ("block.turn_done", "已读 {0} tok  ·  {1}s", "read {0} tok  ·  {1}s"),
    // `^c` throws the turn away; `esc` only interrupts one, and that state is
    // `chrome.paused` above the prompt. Two outcomes, two words — a transcript
    // that calls both of them "interrupted" cannot be read back.
    ("block.interrupted", "这一轮已取消", "turn cancelled"),
    ("block.chapter_done", "本章读完：{0}  ·  {1} tok", "chapter done: {0}  ·  {1} tok"),
    ("block.book_done", "全书读完。要不要重头开始？/goto 1",
        "that's the whole book. Start over with /goto 1?"),
    ("block.plan_title", "阅读清单 · {0}  （{1}% 已读）", "Reading plan · {0}  ({1}% read)"),
        ("block.image", "插图 {0}  ·  {1}×{2}", "figure {0}  ·  {1}×{2}"),
    ("block.image_failed", "插图 {0} 打不开", "figure {0} could not be opened"),
    ("block.context_title", "阅读上下文", "Reading context"),
    ("block.ctx_book", "书名", "book"),
    ("block.ctx_window", "窗口", "window"),
    ("block.ctx_used", "已用", "used"),
    ("block.ctx_chapter", "当前章", "chapter"),
    ("block.ctx_session", "本次", "session"),
    ("block.ctx_speech", "朗读", "speech"),

    // ── library ──
    ("lib.empty", "还没有导入任何书。", "Nothing imported yet."),
    ("lib.empty_hint", "导入一本：readio <文件> [-c 复制 | -l 引用 | -m 移动]，默认复制到 {0}。\n想先试试的话，/sample 打开内置示例。",
        "Import one: readio <file> [-c copy | -l link | -m move]. Copies land in {0}.\nOr try /sample for the built-in sample."),
    ("lib.pick_hint", "输入序号开始读（比如 1），或 /open <序号>；/import <路径> 再导入一本。",
        "Type a number to start reading (say 1), or /open <n>; /import <path> adds another."),
    ("lib.resume_hint", "回车继续上次的《{0}》（第 {1} 本），或输入别的序号。",
        "Enter resumes {0} (#{1}), or type another number."),
    ("lib.no_such_entry", "书库里没有第 {0} 本（现在有 {1} 本），/lib 看列表。",
        "There is no #{0} in the library (it holds {1}); /lib lists them."),
    ("lib.current", "   ← 当前", "   ← open"),
    ("lib.mode_copy", "已复制进书库", "copied into the library"),
    ("lib.mode_link", "已登记引用（原文件留在原处）", "linked (the original stays where it is)"),
    ("lib.mode_move", "已移入书库（原文件已移走）", "moved into the library (the original is gone)"),
    ("lib.forget_copy", "书库里的副本已删除", "the library's copy is deleted"),
    ("lib.forget_link", "原文件保持不动", "the original file is untouched"),
    ("lib.forget_move", "文件仍在书库目录里，需要的话自己删",
        "the file stays in the books directory; remove it yourself if you want"),
    ("lib.imported", "{0}：《{1}》 → {2}（{3}、{4} 章）",
        "{0}: {1} → {2} ({3}, {4} chapters)"),
    ("lib.import_note", "{0}：《{1}》 → {2}（第 {3} 本，{4}）",
        "{0}: {1} → {2} (#{3}, {4})"),
    ("lib.import_failed", "导入失败：{0}", "Import failed: {0}"),
    ("lib.open_failed", "读不了《{0}》（{1}）：{2}", "Cannot read {0} ({1}): {2}"),
    ("lib.no_such", "书库里没有第 {0} 本（现在有 {1} 本），/lib 看列表。",
        "There is no #{0} in the library (it holds {1}), try /lib."),
    ("lib.opened", "已打开《{0}》", "Opened {0}"),
    ("lib.switched", "已切换书目", "Switched books"),
    ("lib.forgot", "已从书库移除《{0}》，{1}。", "Removed {0} from the library, {1}."),
    ("lib.forgot_copy", "书库里的副本已删除", "the library's copy is deleted"),
    ("lib.forgot_link", "原文件保持不动", "the original file is untouched"),
    ("lib.forgot_move", "文件仍在书库目录里，需要的话自己删",
        "the file stays in the books directory — delete it yourself if you want"),
    ("lib.err_dir", "{0} 是目录，请指定一个文件", "{0} is a directory; name a file"),
    ("lib.err_format", "不支持的格式：{0}（支持 .epub / .txt / .md）",
        "Unsupported format: {0} (.epub / .txt / .md)"),
    ("lib.err_missing", "找不到 {0}", "Cannot find {0}"),
    ("lib.err_index", "书库里没有第 {0} 本", "There is no #{0} in the library"),

    // ── commands ──
    ("cmd.unknown", "未知命令 /{0}，试试 /help", "Unknown command /{0} — try /help"),
    ("cmd.no_book", "还没有打开的书。输入序号选一本，/lib 看书库，或 /import <路径> 导入。",
        "No book is open. Type a number, browse with /lib, or /import <path>."),
    ("cmd.pick_first", "先选一本书：输入序号，或 /open <序号>。",
        "Pick a book first: type a number, or /open <n>."),
    ("cmd.library_empty", "书库是空的。用 readio <文件> 导入，或 /sample 读内置示例。",
        "The library is empty. Import with readio <file>, or read /sample."),
    ("cmd.cleared", "已清屏", "Cleared"),
    ("cmd.thinking_toggled", "思考过程已切换", "Reasoning folded / unfolded"),
    ("cmd.tools_expanded", "工具调用已展开", "Tool calls expanded"),
    ("cmd.tools_folded", "工具调用已折叠", "Tool calls folded"),
    ("cmd.quit_again", "再按一次 ctrl+c 退出", "press ctrl+c again to quit"),
    // Spelled out on purpose: `-c | -l | -m` is three letters nobody can decode,
    // and the reader is deciding what happens to their file.
    ("cmd.usage_import", "\
用法：/import <路径> [方式]

  --copy    复制一份进书库（默认，原文件留在原处）
  --link    只记下路径，不复制（原文件挪走就读不到了）
  --move    搬进书库，原位置不再保留

短写 -c / -l / -m 也认。",
        "\
Usage: /import <path> [how]

  --copy    copy it into the library (default; your file stays where it is)
  --link    remember the path only (move the file and the entry goes stale)
  --move    move it into the library, leaving nothing behind

The short forms -c / -l / -m work too."),
    ("cmd.usage_open", "用法：/open <序号> 或 /open <路径>", "Usage: /open <n> or /open <path>"),
    ("cmd.usage_forget", "用法：/forget <序号>（从书库移除，引用和移入的原文件不会删）",
        "Usage: /forget <n> (removes the entry; linked and moved originals are kept)"),
    ("cmd.usage_find", "用法：/find <关键词>", "Usage: /find <term>"),
    ("cmd.usage_goto", "用法：/goto <1-{0}>", "Usage: /goto <1-{0}>"),
    ("cmd.usage_speed", "用法：/speed <4-4000>，例如 /speed 60；平时用 /effort 就够了",
        "Usage: /speed <4-4000>, e.g. /speed 60 — day to day, /effort is the one you want"),
    ("cmd.lang_row", "换界面语言，正文不动", "switch the interface; the book is untouched"),
    ("cmd.lang_row_more", "只改界面用哪种语言，并写回 config.yaml。书永远保持作者写它时的语言。",
        "Changes which language the interface speaks and remembers it in config.yaml. A book always stays in the language it was written in."),
    ("cmd.usage_lang", "用法：/lang zh | en | auto", "Usage: /lang zh | en | auto"),
    ("cmd.speed_set", "基准速度已改，当前 {0} 字/秒（强度 {1}）",
        "Base pace changed: {0} chars/s at effort {1}"),
    ("cmd.auto_on", "自动续读已开启，esc 可以随时中断",
        "Auto-continue is on; esc interrupts it"),
    ("cmd.auto_off", "自动续读已关闭", "Auto-continue is off"),
    ("cmd.last_chapter", "已经是最后一章了", "That's the last chapter"),
    ("cmd.first_chapter", "已经是第一章了", "That's the first chapter"),
    ("cmd.jumped", "跳到第 {0} 章「{1}」", "Jumped to chapter {0}: {1}"),
    ("cmd.progress", "《{0}》 {1}%  ·  {2} / {3}  ·  第 {4} 章第 {5} 段",
        "{0} — {1}%  ·  {2} / {3}  ·  chapter {4}, paragraph {5}"),
    ("cmd.sample_opened", "已打开内置示例", "Opened the built-in sample"),
    ("cmd.lang_set", "界面语言：{0}", "Interface language: {0}"),

    // ── bookmarks ──
    ("cmd.mark_set", "第 {0} 个标记：{1}", "Mark {0}: {1}"),
    ("cmd.marks_none", "还没有标记。/mark 记下现在这一处，/mark <备注> 顺手写句话。",
        "No marks yet. /mark keeps this place, /mark <note> keeps it with a word about why."),
    ("cmd.marks_hint", "输 /marks <序号> 回到某一处，/unmark <序号> 删掉它。",
        "/marks <n> goes back to one, /unmark <n> drops it."),
    ("cmd.mark_no_such", "只有 {0} 个标记。/marks 看列表。",
        "There are only {0} marks. /marks lists them."),
    ("cmd.mark_jumped", "回到第 {0} 个标记：{1}（第 {2} 章）",
        "Back at mark {0}: {1} (chapter {2})"),
    ("cmd.unmark_done", "已删掉第 {0} 个标记：{1}", "Dropped mark {0}: {1}"),

    // ── speech ──
    //
    // Read-aloud is one of the three modes; ^s is its shortcut.
    ("voice.row_test", "念一句，验证引擎接得通", "speak one line to prove the wiring"),
    ("voice.row_config", "告诉我配置文件在哪", "print where the config file lives"),
    ("voice.row_engine", "{0} · 已下载", "{0} · downloaded"),
    ("voice.row_engine_more", "{0} 已在本机可用。要使用它，请在 Voice 配置表单里选择作用范围并保存。",
        "{0} is available locally. To use it, choose a scope in the Voice configuration form and save."),
    ("voice.row_engine_missing", "{0} · 未下载", "{0} · not downloaded"),
    ("voice.row_engine_missing_more", "{0} 还没下载。下载只增加本地模型，不会修改全局或单书 Voice 配置。",
        "{0} is not downloaded. Downloading only adds a local model; it does not change global or per-book Voice configuration."),
    ("voice.row_engine_server", "服务端，装不了", "a server, nothing to install"),
    ("voice.row_engine_server_more", "{0} 连的是你自己起的 OpenAI 兼容服务，readio 不负责装、也不负责起。看 {1}。",
        "{0} talks to an OpenAI-compatible server you run yourself; readio neither installs nor starts it. See {1}."),
    // Same sentence for an engine the reader defined themselves, where there is
    // no link to give: "See ." is worse than not offering to point anywhere.
    ("voice.row_engine_server_bare", "{0} 是你自己配的命令，readio 不负责装、也不负责起。",
        "{0} is a command you configured yourself; readio neither installs nor starts it."),
    ("voice.engine_set", "朗读引擎：{0}", "Speech engine: {0}"),
    ("voice.voice_set", "朗读音色：{0}", "Speech voice: {0}"),
    ("voice.on", "已进入朗读模式：{0}", "Read-aloud mode: {0}"),
    ("voice.off", "已退出朗读模式", "Left read-aloud mode"),
    ("voice.auto_set", "已清除音色和语言覆盖，使用模型默认值。",
        "Cleared voice and language overrides; using the model defaults."),
    ("voice.language_set", "整本都按 {0} 念，音色 {1}",
        "Reading everything as {0}, in {1}"),
    ("voice.row_auto", "使用模型默认音色和语言",
        "use the model's default voice and language"),
    ("voice.row_auto_more",
        "清除音色和语言覆盖，整本书保持当前模型。不会根据句子自动切换模型或语言。",
        "Clears voice and language overrides while keeping the configured model. \
         It never switches model or language based on a sentence."),
    ("voice.row_language", "固定用这一种语言念，音色 {0}",
        "pin this language, read by {0}"),
    ("voice.row_language_more",
        "整本书都当作{0}来念，音色固定为 {1}。适合书本来就只有一种语言，不想让 readio 猜。",
        "Reads the whole book as {0}, in {1}. For a shelf that is all one language, \
         where a guess is one more thing that can be wrong."),
    ("voice.row_pinned", "你点名的音色", "the voice you named"),
    ("voice.row_pinned_more",
        "这是你自己指定的音色，readio 不会覆盖它。readio 认识的音色会带上自己的语种。第一行恢复模型默认值。",
        "A voice you named yourself, which readio will not overrule. One it recognises brings \
         its language along. The first row restores the model defaults."),
    ("voice.language_zh", "中文", "Chinese"),
    ("voice.language_en", "英文", "English"),
    ("voice.speed_set", "朗读倍速 {0}", "Read-aloud speed {0}"),
    ("voice.speed_later", "朗读倍速 {0}，下次开启朗读时生效",
        "Read-aloud speed {0}; it takes effect when you turn speech on"),
    ("voice.speed_now", "当前倍速 {0}  ·  可选 {1}  ·  ^r 循环切换",
        "Speed {0}   ladder: {1}   (^r cycles)"),
    ("voice.unknown_engine", "没有这个引擎：{0}。可用：{1}", "No such engine: {0}. Available: {1}"),
    // Short on purpose: this rides the status row, where it fades on its own
    // after a few seconds. It used to be a transcript line long enough to name
    // the config file, which meant every failed Voice attempt left another copy of
    // the same sentence stacked in the reader's book. `/voice` lists the engines
    // that are actually installed, so the way out is one command away.
    ("voice.missing_binary", "找不到 {0}，朗读没开成。/voice 里 ⏎ 一下就装上",
        "{0} not found, so read-aloud is off. /voice and ⏎ installs one"),
    ("voice.failed", "朗读失败：{0}", "Read-aloud failed: {0}"),
    ("voice.stopped", "朗读失败，停在朗读模式：{0}。修好后回车重试，或手动切换模式；不会自动降级。",
        "Read-aloud stopped in place: {0}. Fix it and press Enter to retry, or change mode explicitly; there is no automatic fallback."),
    ("voice.engine_gone", "引擎「{0}」不在了", "the {0} engine is no longer there"),
    ("voice.worker_start", "起不来 {0}：{1}", "cannot start {0}: {1}"),
    ("voice.worker_gone", "常驻引擎中途退出了{0}", "the resident engine exited{0}"),
    ("voice.worker_slow", "常驻引擎 {0} 秒还没就绪", "the resident engine did not come up within {0}s"),
    ("voice.worker_no_command", "常驻引擎没有配置启动命令{0}",
        "the resident engine has no command to run{0}"),
    ("voice.config_at", "朗读配置：{0}", "Speech config: {0}"),
    ("voice.usage", "用法：/voice 打开 Voice 工作台。开关朗读用 ^s。",
        "Usage: /voice opens the Voice workspace. ^s turns Voice on and off."),
    ("voice.no_switch", "Voice 工作台只配置模型。用 /mode aloud 或 ^s 进入朗读模式。",
        "The Voice workspace configures models. Use /mode aloud or ^s to enter read-aloud."),
    ("voice.usage_rate", "用法：/rate <0.5-3.0>，改的是当前强度这一档的倍数",
        "Usage: /rate <0.5-3.0> — it retunes the level you are on"),
    ("voice.testing", "试念一句：{0}", "Test line: {0}"),
    ("voice.test_line", "界面不是中立的，它替你决定了什么值得注意。",
        "An interface is never neutral: it decides for you what deserves attention."),
    ("voice.engines", "可用引擎：{0}", "Engines: {0}"),
    ("voice.speaking", "朗读中", "reading aloud"),

    // ── who reads what ──
    //
    // Every change of voice says which books it changed. A reader who sets a
    // voice while reading and finds their next book unchanged has been surprised
    // by a rule nobody told them, and one clause is the whole fix.
    ("voice.for_book", "{0}——只对《{1}》生效。/voice everywhere 可以让所有书都这样念。",
        "{0} — for {1} alone. /voice everywhere makes it the default for every book."),
    ("voice.for_every_book", "{0}——所有没单独设过的书都这样念。",
        "{0} — for every book that has not asked for something else."),
    ("voice.now_default", "以后所有书都用 {0} 念，这本书不再单独设置。",
        "Every book is read by {0} now, and this one no longer has a setting of its own."),
    ("voice.follows_default", "《{0}》跟回默认设置：{1}",
        "{0} follows the default again: {1}"),
    ("voice.was_default", "《{0}》本来就跟着默认设置。",
        "{0} was already following the default."),
    ("voice.already_default", "书架上改的就是默认设置——先打开一本书，才有“这一本”可说。",
        "In the library there is only the default: open a book before asking for one book's voice."),
    // An install is a fact about this machine, so it writes the default. A book
    // that had asked for another engine keeps it, and is told so rather than
    // quietly overruled.
    ("voice.book_keeps_engine", "这本书仍然用 {0} 念；想跟上新装的引擎就 /voice default。",
        "This book still reads with {0}. /voice default lets it follow the new one."),
    ("voice.row_everywhere", "让所有书都这样念（{0}）",
        "use this for every book ({0})"),
    ("voice.row_everywhere_more",
        "把《{0}》现在这套朗读设置升为默认，所有没单独设过的书都跟着变，这本书的单独设置随之删掉——同一个决定不该有两份记录。",
        "Promotes what {0} is read with to the default, so every book without a \
         setting of its own follows it, and removes this book's entry: one decision, one record."),
    ("voice.row_default", "这本书跟回默认设置", "let this book follow the default"),
    ("voice.row_default_more",
        "删掉《{0}》自己的朗读设置，改回跟随上面的默认；默认以后再变，这本书也跟着变。",
        "Deletes {0}'s own read-aloud setting and lets it follow the default again, \
         including whatever the default becomes later."),
    ("voice.workspace", "Voice 工作台", "Voice workspace"),
    ("voice.workspace_row", "模型下载与配置在同一个 TUI 中分开进行",
        "Download models and configure them separately in one TUI"),
    ("voice.workspace_more",
        "左边只管理本地模型，右边只保存全局或单书配置；下载绝不会自动启用模型。",
        "The left manages local models; the right saves global or per-book configuration. A download never applies itself."),

    // ── installing an engine ──
    //
    // readio bundles no model, so at some point every reader who wants a voice
    // has to install one. The whole point of these lines is that they name the
    // next keypress: an install that fails and leaves the reader reading a
    // stack trace has failed twice.
    ("install.starting", "正在下载 {0}，使用 {1}。完成后仍需在 Voice 配置中保存。",
        "Downloading {0} with {1}. Save it in Voice configuration after it finishes."),
    ("install.done", "{0} 下载完成，用了 {1} 秒。Voice 配置没有改变。",
        "{0} downloaded after {1}s. Voice configuration was not changed."),
    // Ten columns of header, no more: the tool line has already spent its width
    // on the command. What went wrong is explained underneath.
    ("install.step_failed", "失败", "failed"),
    ("install.failed", "{0} 没装成。上面红色那条就是断掉的命令——照着它在终端里跑一遍，能看到完整报错。装法看 {1}",
        "{0} did not install. The command in red above is where it stopped; running it in a terminal shows the full error. Instructions: {1}"),
    ("install.busy", "{0} 还在装，等它装完。", "{0} is still installing; let it finish."),
    ("install.no_space", "空间不足：当前可用 {0}，安全下载至少需要 {1}。",
        "Not enough disk space: {0} available, at least {1} required for a safe download."),
    ("install.already", "{0} 已经可用；Voice 配置没有改变。",
        "{0} is already available; Voice configuration was not changed."),
    ("install.not_yet", "{0} 还没装。/voice 里 ⏎ 一下就装上", "{0} is not installed yet. /voice and ⏎ puts it there"),
    ("install.no_recipe", "{0} 不是 readio 能装的东西——它是个你自己起的服务。怎么起看 {1}",
        "{0} is not something readio can install: it is a server you run yourself. See {1}"),
    ("install.no_recipe_bare", "{0} 不是 readio 能装的东西——那条命令是你自己写的，得你自己备齐",
        "{0} is not something readio can install: that command line is yours, so its program is too"),
    ("install.no_installer", "这台机器上没找到支持的系统包管理器，无法安装这个非 Python 引擎。",
        "No supported system package manager was found for this non-Python engine."),
    ("install.unsupported_runtime", "Readio 暂无适用于 {0} 的自管理 Python 运行时。",
        "Readio does not yet provide a managed Python runtime for {0}."),

    // ── what a speech engine says when it fails ──
    //
    // These come back from a subprocess, so they are the one class of message
    // readio cannot phrase in advance. They still have to arrive in the
    // reader's language, and they have to name the file that can be edited —
    // config.yaml, the only file readio has.
    ("synth.failed", "{0} 合成失败", "{0} could not synthesize that"),
    ("synth.no_audio", "{0} 没写出音频文件（对一下 config.yaml 里它的 synth 命令）",
        "{0} wrote no audio file (check its synth command in config.yaml)"),
    ("synth.bad_wav", "{0} 写出的不是能识别的 wav（{1}）",
        "{0} wrote something that is not a readable wav ({1})"),
    ("synth.empty_command", "命令是空的", "the command is empty"),
    ("synth.empty_play", "播放命令是空的", "the play command is empty"),
    ("synth.cannot_run", "运行不了 {0}", "could not run {0}"),
    ("synth.cannot_play", "运行不了播放器 {0}", "could not run the player {0}"),
    ("synth.player_exited", "播放器退出了（{0}）", "the player exited ({0})"),
    ("synth.exited", "{0} 退出了（{1}）", "{0} exited ({1})"),
    ("synth.exited_saying", "{0} 退出了（{1}）：{2}", "{0} exited ({1}): {2}"),
    ("synth.timeout", "{0} 超时了", "{0} timed out"),

    // ── audio output whitelist ──
    ("dev.muted_title", "朗读已静音：当前输出设备不在白名单",
        "Read-aloud muted: this output is not on your whitelist"),
    ("dev.muted_device", "现在的输出是「{0}」，白名单里是：{1}",
        "Sound is going to {0}; your whitelist says: {1}"),
    ("dev.muted_unknown", "认不出当前的输出设备（{0}），为了不外放，先静音。",
        "Cannot tell what the output device is ({0}), so staying silent rather than risking it."),
    ("dev.muted_fix", "怎么办：/device allow {0} 把它加进白名单 · /device any 不再限制 · /device 看全部设备",
        "Options: /device allow {0} to add it · /device any to stop checking · /device to list everything"),
    ("dev.muted_fix_plain", "怎么办：/device 看全部设备并选一个 · /device any 不再限制",
        "Options: /device to list the outputs and pick one · /device any to stop checking"),
    ("dev.resumed", "输出回到「{0}」。仍在朗读模式，回车重试。", "Back on {0}. Still in Read-aloud; press Enter to retry."),
    ("dev.muted_status", "静音（设备不在白名单）", "muted (output not allowed)"),
    ("dev.title", "音频输出设备", "Audio outputs"),
    ("dev.current", "   ← 当前", "   ← current"),
    ("dev.allowed_mark", "  ✓ 白名单", "  ✓ allowed"),
    ("dev.whitelist", "白名单：{0}", "Whitelist: {0}"),
    ("dev.whitelist_empty", "白名单：空（任何设备都出声）",
        "Whitelist: empty — any output is allowed"),
    ("dev.hint", "/device allow <序号或名字> 加进白名单 · /device deny <序号或名字> 移出 · /device any 清空限制 · /device refresh 重新检测",
        "/device allow <n|name> adds one · /device deny <n|name> removes it · /device any clears the list · /device refresh re-checks"),
    ("dev.probe_failed", "读不到音频设备列表：{0}", "Cannot list audio outputs: {0}"),
    ("dev.probe_hint", "在 config.yaml 里 voice.output.query 写一条能打印设备名的命令即可。",
        "Set voice.output.query in config.yaml to a command that prints the device name."),
    ("dev.probe_muted", "白名单开着，认不出的设备一律不出声。/device any 解除限制。",
        "The whitelist is on and an output that cannot be identified counts as not allowed, so speech stays muted. /device any lifts the restriction."),
    ("dev.allowed_added", "已加入白名单：{0}", "Added to the whitelist: {0}"),
    ("dev.allowed_exists", "「{0}」已经在白名单里了", "{0} is already on the whitelist"),
    ("dev.denied", "已从白名单移出：{0}", "Removed from the whitelist: {0}"),
    ("dev.deny_missing", "白名单里没有「{0}」", "{0} is not on the whitelist"),
    ("dev.any", "白名单已清空：任何输出设备都会出声。",
        "Whitelist cleared: readio will speak on any output."),
    ("dev.usage", "用法：/device [allow <序号或名字> | deny <序号或名字> | any | refresh]",
        "Usage: /device [allow <n|name> | deny <n|name> | any | refresh]"),
    ("dev.no_such", "没有第 {0} 个设备", "There is no output #{0}"),

    ("lib.forgotten", "已从书库移除《{0}》，{1}。", "Dropped {0} from the library, {1}."),
    ("lib.not_found", "找不到 {0}", "cannot find {0}"),
    ("lib.copy_failed", "无法复制到 {0}", "cannot copy to {0}"),
    ("lib.write_failed", "无法写入 {0}", "cannot write {0}"),
    ("lib.moved_but_kept", "已复制到书库，但删不掉原文件 {0}",
        "copied into the library, but the original {0} could not be removed"),
    ("lib.no_entry", "书库里没有第 {0} 本", "the library has no entry #{0}"),
    ("lib.is_dir", "{0} 是目录，请指定一个文件", "{0} is a directory; name a file"),
    ("lib.unsupported", "不支持的格式：{0}（支持 .epub / .pdf / .txt / .md）",
        "unsupported format: {0} (readio reads .epub, .pdf, .txt and .md)"),
    ("prompt.empty_library", "readio <文件> 导入一本，或 /sample 试试内置示例",
        "readio <file> imports one, or /sample tries the built-in sample"),
    ("pdf.unreadable", "{0} 不是能读的 PDF", "{0} is not a readable PDF"),
    ("pdf.broken", "{0} 的结构读不下去，文件可能已损坏",
        "{0} could not be parsed; the file may be damaged"),
    ("pdf.encrypted", "这份 PDF 有密码保护，readio 打不开",
        "this PDF is password protected, so readio cannot open it"),
    ("pdf.part", "第 {0} 部分 ({1})", "Part {0} ({1})"),
    ("book.section", "第 {0} 节", "Section {0}"),
    ("media.epub_unopenable", "打不开 {0}", "cannot open {0}"),
    ("media.not_zip", "{0} 不是有效的 EPUB（zip）文件", "{0} is not a valid EPUB (zip) file"),
    ("media.cache_failed", "无法创建图片缓存目录 {0}",
        "cannot create the image cache directory {0}"),

    // ── reading narration ──
    ("flow.resume", "先确认上次停在哪：第 {0} 章「{1}」第 {2} 段。核对一下段落偏移再往下读。",
        "First, where did we stop: chapter {0} ({1}), paragraph {2}. Check the offset, then continue."),
    ("flow.chapter_start", "进入第 {0} 章「{1}」，一共 {2} 段、约 {3}。先读开头 {4} 段，建立上下文。",
        "Entering chapter {0} ({1}): {2} paragraphs, about {3}. Read the first {4} to get the shape."),
    ("flow.continue", "{0}接着第 {1} 段往下，这一轮约 {2}，{3}。",
        "{0}Continuing from paragraph {1}; about {2} this turn, {3}."),
    ("flow.wraps_chapter", "刚好收尾本章", "which finishes the chapter"),
    ("flow.remaining", "读完还剩 {0} 段", "leaving {0} paragraphs"),
    ("flow.restored", "restored from {0}  ·  {1}%", "restored from {0}  ·  {1}%"),
    ("flow.locate_detail", "progress → ch{0} p{1}", "progress → ch{0} p{1}"),
    ("flow.write_detail", "ch{0} complete", "ch{0} complete"),
    ("flow.search_thought", "「{0}」——先在全书里按字面检索，再看命中的段落能不能直接回答。",
        "\"{0}\" — grep the whole book first, then see whether the hits answer it."),
    ("flow.no_hits", "全书没有出现「{0}」。\n\n可以换个说法再试，或者用 /toc 看章节标题挑一个入口。",
        "\"{0}\" does not appear anywhere.\n\nTry different words, or use /toc to pick an entry point."),
    ("flow.hits_intro", "「{0}」在全书出现 {1} 处，分布在 {2} 段。列出前 {3} 处：\n",
        "\"{0}\" appears {1} times across {2} paragraphs. Here are the first {3}:\n"),
    ("flow.grep_detail", "pattern: {0}  ·  {1} matches  ·  {2} lines",
        "pattern: {0}  ·  {1} matches  ·  {2} lines"),
    ("flow.grep_detail_capped", "pattern: {0}  ·  {1} matches  ·  {2} lines  ·  showing {3}",
        "pattern: {0}  ·  {1} matches  ·  {2} lines  ·  showing {3}"),
    ("find.jumped", "跳到第 {0}/{1} 处：第 {2} 章第 {3} 段", "Hit {0}/{1}: chapter {2}, paragraph {3}"),
    ("find.wrapped", "已经是最后一处，回到第 1 处。", "That was the last hit; back to the first."),
    ("find.wrapped_back", "已经是第一处，跳到最后一处。", "That was the first hit; jumping to the last."),
    ("find.none_active", "现在没有检索结果。先 /find <词>。",
        "No search is active — try /find <term> first."),
    ("find.no_such", "只有 {0} 处可跳，没有第 {1} 处。", "There are {0} hits, so there is no #{1}."),
    ("find.cleared", "检索结果已清空。", "Search cleared."),
    ("flow.grep_none", "没有匹配", "no matches"),
    ("flow.grep_zero", "0 匹配", "0 matches"),
    ("flow.jump_hint", "\n输入序号跳到那一处（^g 下一处，^b 上一处）。",
        "\nType a number to jump to a hit; ^g for the next one, ^b for the previous.\n"),
    ("flow.hit_ref", "第 {0} 章「{1}」第 {2} 段", "chapter {0} ({1}), paragraph {2}"),
    ("flow.hits_outro", "\n要从哪一处开始读？", "\nWhere would you like to start?"),
    ("flow.hits_goto", "直接 /goto {0} 会跳到第一处所在的章；那一处在第 {1} 段。",
        "/goto {0} jumps to the chapter holding the first hit; it sits at paragraph {1}."),
    ("flow.toc_thought", "列一下目录：{0} 章，合计约 {1} 字。",
        "List the table of contents: {0} chapters, about {1} chars."),
    ("flow.welcome_resume", "接着上次读《{0}》，进度 {1}%，停在第 {2} 章。",
        "Resuming {0} at {1}%, chapter {2}."),
    ("flow.welcome_fresh", "已载入《{0}》，{1} 章、约 {2}。回车或空格开始读，/toc 选章节。",
        "Loaded {0}: {1} chapters, about {2}. Enter or space starts reading; /toc picks a chapter."),

    // ── reading narration ──
    ("flow.think_resumed", "先确认上次停在哪：第 {0} 章「{1}」第 {2} 段。核对一下段落偏移再往下读。",
        "First, where did I stop: chapter {0} ({1}), paragraph {2}. Checking the offset before reading on."),
    ("flow.think_chapter_start", "进入第 {0} 章「{1}」，一共 {2} 段、约 {3}。先读开头 {4} 段，建立上下文。",
        "Into chapter {0} ({1}): {2} paragraphs, about {3}. Reading the first {4} to establish context."),
    ("flow.think_continue", "{0}接着第 {1} 段往下，这一轮约 {2}，{3}。",
        "{0}Continuing from paragraph {1}; about {2} this turn, {3}."),
    ("flow.think_wraps_chapter", "刚好收尾本章", "which finishes the chapter"),
    ("flow.think_paras_left", "读完还剩 {0} 段", "leaving {0} paragraphs after it"),
    ("flow.filler_ok", "好，", "Right, "),
    ("flow.filler_go_on", "继续。", "Onward. "),
    ("flow.think_narrow", "「{0}」整串没有命中，收窄成「{1}」再找一遍。",
        "Nothing matches \"{0}\" as a whole; narrowing the search to \"{1}\"."),
    ("flow.think_search", "「{0}」——先在全书里按字面检索，再看命中的段落能不能直接回答。",
        "{0} — literal search across the book first, then whether the hits answer it directly."),
    ("flow.hit_line", "{0}. 第 {1} 章「{2}」第 {3} 段  {4}\n",
        "{0}. ch {1} ({2}), para {3}  {4}\n"),
    ("flow.toc_intro", "列一下目录：{0} 章，合计约 {1}。",
        "Listing the contents: {0} chapters, about {1} in total."),
    ("flow.toc_current", "   ← 当前", "   ← current"),
    ("flow.plan_title", "阅读清单 · {0}  ({1}% 已读)", "Reading plan · {0}  ({1}% read)"),
    ("flow.figure_inline", "> 〔插图〕", "> [figure]"),
    ("flow.cover", "封面", "cover"),

    // ── command line ──
    ("cli.usage", "\
用法：
  readio                 进入书库，列出所有导入过的书
  readio <文件>          导入并开始读（默认复制一份）
  readio <文件> --copy   复制一份到书库目录（-c）
  readio <文件> --link   只登记引用，文件留在原处（-l）
  readio <文件> --move   移动进书库，原位置不再保留（-m）

支持 .epub / .pdf / .txt / .md。
书库目录：{0}
阅读进度：{1}
全部设置：{2}
换目录：readio --home <目录>。启动后 /help 查看命令。",
        "\
Usage:
  readio                 open the library and list everything imported
  readio <file>          import and start reading (a copy, by default)
  readio <file> --copy   copy it into the books directory (-c)
  readio <file> --link   link it: your file stays where it is (-l)
  readio <file> --move   move it in; nothing is left behind (-m)

Supports .epub / .pdf / .txt / .md.
Books:    {0}
Progress: {1}
Settings: {2}
A different directory: readio --home <dir>. /help once inside."),
    ("cli.mode_conflict", "-c / -l / -m 只能选一个", "pick only one of -c / -l / -m"),
    ("cli.one_file", "一次只能打开一个文件", "one file at a time"),
    ("cli.unknown_flag", "不认识的参数 {0}", "unknown argument {0}"),
    ("cli.mode_needs_file", "导入模式要配一个文件，比如 readio book.epub -m",
        "an import mode needs a file, as in readio book.epub -m"),
    ("cli.short_usage", "用法：readio [文件] [-c 复制 | -l 引用 | -m 移动] [--home <目录>]",
        "Usage: readio [file] [-c copy | -l link | -m move] [--home <dir>]"),
    ("cli.home_needs_dir", "--home 后面要跟一个目录", "--home needs a directory"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Mutex, MutexGuard};

    /// The current language is process-global, so tests that switch it must not
    /// run concurrently.
    fn exclusive() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Both columns must take the same placeholders, or one language would
    /// silently drop a number.
    fn placeholders(text: &str) -> HashSet<String> {
        let mut found = HashSet::new();
        for index in 0..10 {
            let needle = format!("{{{index}}}");
            if text.contains(&needle) {
                found.insert(needle);
            }
        }
        found
    }

    #[test]
    fn every_entry_is_complete_and_consistent() {
        let mut keys = HashSet::new();
        for (key, zh, en) in TABLE {
            assert!(keys.insert(*key), "duplicate key {key}");
            assert!(!zh.trim().is_empty(), "{key} has no Chinese text");
            assert!(!en.trim().is_empty(), "{key} has no English text");
            assert_eq!(
                placeholders(zh),
                placeholders(en),
                "{key}: placeholders differ between languages"
            );
        }
        assert!(TABLE.len() > 80, "the table looks suspiciously short");
    }

    #[test]
    fn keys_are_namespaced() {
        for (key, _, _) in TABLE {
            assert!(
                key.contains('.'),
                "{key} should be namespaced, like chrome.something"
            );
        }
    }

    #[test]
    fn lookup_follows_the_current_language() {
        let _guard = exclusive();
        set(Lang::Zh);
        assert_eq!(t("block.interrupted"), "这一轮已取消");
        set(Lang::En);
        assert_eq!(t("block.interrupted"), "turn cancelled");
    }

    #[test]
    fn formatting_substitutes_positionally() {
        let _guard = exclusive();
        set(Lang::En);
        assert_eq!(
            tf("chrome.chapter", &[&2, &7]),
            "ch 2/7",
            "both arguments should land"
        );
        set(Lang::Zh);
        assert_eq!(tf("chrome.chapter", &[&2, &7]), "第 2/7 章");
    }

    #[test]
    fn an_unknown_key_is_visible_rather_than_silent() {
        assert_eq!(t("nope.missing"), "?nope.missing");
    }

    #[test]
    fn language_preferences_parse() {
        assert_eq!(Lang::parse("zh"), Some(Lang::Zh));
        assert_eq!(Lang::parse("EN"), Some(Lang::En));
        assert_eq!(Lang::parse("中文"), Some(Lang::Zh));
        assert!(Lang::parse("klingon").is_none());
        assert!(
            Lang::parse("auto").is_none(),
            "there is no locale to consult: the config file decides"
        );
        assert_eq!(Lang::default(), Lang::En, "English is the default");
    }
}
