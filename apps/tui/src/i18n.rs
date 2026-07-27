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
/// `cmd.*` for command feedback, `lib.*` for the library, `tts.*` for speech,
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
    ("chrome.continue", "⏎ 继续", "⏎ keep reading"),
    ("chrome.idle_tail", "  ·  ^t 思考  ·  ^o 工具  ·  /help 更多",
        "  ·  ^t thinking  ·  ^o tools  ·  /help for more"),
    ("chrome.busy_tail", "  ·  esc 停下  ·  ↑↓ 滚动", "  ·  esc to stop  ·  ↑↓ to scroll"),
    ("chrome.scrolled", "{0} 已上滚 {1}%", "{0} scrolled up {1}%"),
    ("chrome.scrolled_tail", "  ·  end 回到底部", "  ·  end returns to the tail"),
    ("chrome.cps", "{0} 字/秒", "{0} chars/s"),
    ("chrome.session", "  ·  本次 {0}", "  ·  {0} this session"),
    ("chrome.help_title", " 快捷键与命令 ", " Keys and commands "),
    ("chrome.help_close", " 按任意键关闭", " any key closes"),

    // ── prompt placeholders ──
    ("prompt.reading", "回车继续阅读，或提问 / 输入命令",
        "enter to keep reading, or ask a question / type a command"),
    ("prompt.pick", "输入序号选书，或 /import <路径> 导入",
        "type a number to pick a book, or /import <path>"),
    ("prompt.empty", "readio <文件> 导入一本，或 /sample 试试内置示例",
        "readio <file> to import, or /sample for the built-in sample"),

    // ── blocks ──
    ("block.thinking", "Thinking", "Thinking"),
    ("block.thought_for", "Thought for {0}s", "Thought for {0}s"),
    ("block.thought", "Thought", "Thought"),
    ("block.turn_done", "已读 {0} tok  ·  {1}s", "read {0} tok  ·  {1}s"),
    ("block.interrupted", "已中断", "interrupted"),
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
    ("lib.title", "书库  {0} 本", "Library  ·  {0} books"),
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
    ("cmd.usage_import", "用法：/import <路径> [-c | -l | -m]（默认 -c 复制进书库）",
        "Usage: /import <path> [-c | -l | -m] (default -c, copies into the library)"),
    ("cmd.usage_open", "用法：/open <序号> 或 /open <路径>", "Usage: /open <n> or /open <path>"),
    ("cmd.usage_forget", "用法：/forget <序号>（从书库移除，引用和移入的原文件不会删）",
        "Usage: /forget <n> (removes the entry; linked and moved originals are kept)"),
    ("cmd.usage_find", "用法：/find <关键词>", "Usage: /find <term>"),
    ("cmd.usage_goto", "用法：/goto <1-{0}>", "Usage: /goto <1-{0}>"),
    ("cmd.usage_speed", "用法：/speed <4-4000>，例如 /speed 60",
        "Usage: /speed <4-4000>, for example /speed 60"),
    ("cmd.usage_lang", "用法：/lang zh | en | auto", "Usage: /lang zh | en | auto"),
    ("cmd.speed_set", "阅读速度设为 {0} 字/秒", "Reading speed set to {0} chars/s"),
    ("cmd.auto_on", "自动续读已开启，esc 可以随时停下",
        "Auto-continue is on; esc stops it"),
    ("cmd.auto_off", "自动续读已关闭", "Auto-continue is off"),
    ("cmd.last_chapter", "已经是最后一章了", "That's the last chapter"),
    ("cmd.first_chapter", "已经是第一章了", "That's the first chapter"),
    ("cmd.jumped", "跳到第 {0} 章「{1}」", "Jumped to chapter {0}: {1}"),
    ("cmd.progress", "《{0}》 {1}%  ·  {2} / {3}  ·  第 {4} 章第 {5} 段",
        "{0} — {1}%  ·  {2} / {3}  ·  chapter {4}, paragraph {5}"),
    ("cmd.sample_opened", "已打开内置示例", "Opened the built-in sample"),
    ("cmd.lang_set", "界面语言：{0}", "Interface language: {0}"),

    // ── speech ──
    ("tts.on", "朗读已开启：{0}", "Read-aloud on: {0}"),
    ("tts.off", "朗读已关闭", "Read-aloud off"),
    ("tts.engine_set", "朗读引擎：{0}", "Speech engine: {0}"),
    ("tts.voice_set", "朗读音色：{0}", "Speech voice: {0}"),
    ("tts.speed_set", "朗读倍速 {0}", "Read-aloud speed {0}"),
    ("tts.speed_later", "朗读倍速 {0}，下次开启朗读时生效",
        "Read-aloud speed {0}; it takes effect when you turn speech on"),
    ("tts.speed_now", "当前倍速 {0}　可选：{1}　（^r 循环切换）",
        "Speed {0}   ladder: {1}   (^r cycles)"),
    ("tts.unknown_engine", "没有这个引擎：{0}。可用：{1}", "No such engine: {0}. Available: {1}"),
    ("tts.missing_binary", "找不到 {0}。装好之后再开 /tts，或改 {1} 换一个引擎。",
        "Cannot find {0}. Install it and run /tts again, or pick another engine in {1}."),
    ("tts.failed", "朗读失败：{0}", "Read-aloud failed: {0}"),
    ("tts.fallback", "朗读已停，改回按字速阅读。", "Speech stopped; back to timed reading."),
    ("tts.config_at", "朗读配置：{0}", "Speech config: {0}"),
    ("tts.usage", "用法：/tts [on | off | <引擎> | test | config]",
        "Usage: /tts [on | off | <engine> | test | config]"),
    ("tts.usage_voice", "用法：/voice <音色>", "Usage: /voice <name>"),
    ("tts.usage_rate", "用法：/rate <0.5-3.0>，或 ^r 在常用倍速间循环",
        "Usage: /rate <0.5-3.0>, or ^r to cycle the usual speeds"),
    ("tts.testing", "试念一句：{0}", "Test line: {0}"),
    ("tts.test_line", "界面不是中立的，它替你决定了什么值得注意。",
        "An interface is never neutral: it decides for you what deserves attention."),
    ("tts.engines", "可用引擎：{0}", "Engines: {0}"),
    ("tts.speaking", "朗读中", "reading aloud"),

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
    ("dev.resumed", "输出回到「{0}」，朗读继续。", "Back on {0}; read-aloud resumed."),
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
    ("dev.probe_hint", "在 config.yaml 里 tts.output.query 写一条能打印设备名的命令即可。",
        "Set tts.output.query in config.yaml to a command that prints the device name."),
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
    ("flow.welcome_fresh", "已载入《{0}》，{1} 章、约 {2}。回车开始读，/help 看命令。",
        "Loaded {0}: {1} chapters, about {2}. Enter starts reading; /help lists commands."),

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

    // ── command line ──
    ("cli.usage", "\
用法：
  readio                 进入书库，列出所有导入过的书
  readio <文件>          导入并开始读（默认 -c）
  readio <文件> -c       复制一份到书库目录
  readio <文件> -l       只登记引用，文件留在原处
  readio <文件> -m       移动进书库，原文件不再保留

支持 .epub / .pdf / .txt / .md。
书库目录：{0}
阅读进度：{1}
全部设置：{2}
换目录：readio --home <目录>。启动后 /help 查看命令。",
        "\
Usage:
  readio                 open the library and list everything imported
  readio <file>          import and start reading (default -c)
  readio <file> -c       copy the file into the books directory
  readio <file> -l       link it: the file stays where it is
  readio <file> -m       move it in; the original is not kept

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
        assert_eq!(t("block.interrupted"), "已中断");
        set(Lang::En);
        assert_eq!(t("block.interrupted"), "interrupted");
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
