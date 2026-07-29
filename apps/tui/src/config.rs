//! The one configuration file: `~/.readio/config.yaml`.
//!
//! readio reads no environment variables. Language, reading speed, read-aloud
//! and image settings all live in this single YAML file, which readio writes
//! (with comments) the first time it runs and rewrites whenever a command like
//! `/speed` or `/voice` changes something. Hand-editing it is expected.
//!
//! A malformed file is reported and ignored rather than replaced: the reader
//! hand-edits this, so clobbering their work would be rude.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::effort::{Effort, Ladder};
use crate::i18n::Lang;
use crate::mode::Mode;
use crate::paths;
use crate::voice::config::{EngineSpec, presets};
use crate::voice::device::Output;

/// Everything readio can be told to do differently.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Interface language.
    pub language: Lang,
    pub reading: Reading,
    /// Reading pace, worn as an agent's reasoning effort.
    pub effort: EffortConfig,
    pub images: Images,
    /// Read-aloud, all of it: which engine, which voice, which language, and
    /// the books that asked for something else.
    ///
    /// It answered to `tts:` until v0.2.0, and still does when reading an older
    /// file — the section was renamed rather than split, because "which engine"
    /// and "which voice" were never two questions.
    #[serde(alias = "tts")]
    pub voice: Voice,
    /// Diagnostics: append every terminal event to this file. Unset normally —
    /// it exists because a keystroke going missing is otherwise unprovable.
    pub input_log: Option<String>,
}

/// How text is revealed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reading {
    /// Characters per second, when speech is not driving the pace.
    pub speed: f32,
    /// Which of the three ways of reading is in force. One value, written here
    /// whenever it changes and restored on the next launch.
    pub mode: Mode,
    /// Auto-scroll as older files spelled it, before the mode had a name of its
    /// own. Read once, folded into `mode`, and never written again.
    #[serde(default, skip_serializing)]
    auto: Option<bool>,
}

/// The chosen level and what each level is worth.
///
/// One multiplier per level, and it drives both the reveal speed and read-aloud
/// playback, so a level means the same thing however the reader is taking the book
/// in. The reader is expected to edit these.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffortConfig {
    pub level: Effort,
    pub multipliers: Ladder,
}

/// Whether and how large in-terminal images are drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Images {
    pub enabled: bool,
    /// Tallest an illustration may be, in terminal rows.
    pub max_rows: u16,
}

/// Read-aloud: who reads, and everything that follows from it.
///
/// One section rather than an engine here and a voice there. In every engine
/// worth using the two are one decision — Kokoro's `zf_*` voices are Mandarin
/// and its `af_*` ones American English, so a voice implies an engine and a
/// language the way a key implies a lock — and settings that can be made to
/// disagree eventually are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Voice {
    /// Which entry of `engines` to use.
    pub engine: String,
    /// The voice itself, handed to the engine as it is spelled there. Empty
    /// keeps the selected model preset's declared default.
    ///
    /// Spelled `voice:` inside a section that was called `tts:`, which is why
    /// the old name is still accepted.
    #[serde(alias = "voice")]
    pub name: String,
    /// Which language to read in: a key of the engine's `languages` table such
    /// as `zh` or `en`. Empty keeps the model preset's declared default.
    pub language: String,
    /// Engine-specific arguments selected in the Voice workspace. Empty keeps
    /// the model preset's own defaults.
    #[serde(default)]
    pub params: String,
    /// How many sentences to synthesize ahead of playback.
    pub prefetch: usize,
    /// Which audio outputs may be spoken through.
    pub output: Output,
    /// Books that asked for a different voice, keyed by content id — the same
    /// id the library and the progress store use.
    ///
    /// A shelf is not read in one voice. A Chinese novel and an English manual
    /// want different engines as often as different voices, and a reader who
    /// sets one up per book should not have to set it up again every time they
    /// switch. Only what a book actually said is stored; everything else falls
    /// through to the settings above.
    pub books: BTreeMap<String, BookVoice>,
    pub engines: BTreeMap<String, EngineSpec>,
    /// Whether text is spoken. Independent from `reading.mode`: manual/auto
    /// controls continuation, while this controls audio.
    #[serde(default)]
    pub enabled: bool,
}

/// What one book wants, where it differs from the default.
///
/// Every field is optional and an absent one means "whatever the default says",
/// which is the difference between a book with no opinion about its language
/// and a book that explicitly restores the model default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookVoice {
    /// The book's title as it was when this was written. Nothing reads it —
    /// it is here so that a file full of content ids can be read by a person.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(alias = "voice", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Per-book engine arguments. `Some("")` deliberately restores the model
    /// defaults even when the global configuration carries custom arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<String>,
}

/// The voice in force: the default with the open book's entry laid over it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chosen {
    pub engine: String,
    /// The reader's explicit choice, which may be empty — the model preset then
    /// supplies its default voice.
    pub name: String,
    pub language: String,
    pub params: String,
}

/// Where a choice is written down.
///
/// The scope explicitly selected in the Voice workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope<'a> {
    /// Every book that has no entry of its own.
    Default,
    /// One book, named so the config file can be read.
    Book { id: &'a str, title: &'a str },
}

impl Default for Reading {
    fn default() -> Self {
        Self {
            speed: 46.0,
            mode: Mode::default(),
            auto: None,
        }
    }
}

impl Default for Images {
    fn default() -> Self {
        Self {
            enabled: true,
            max_rows: 16,
        }
    }
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            engine: String::new(),
            name: String::new(),
            language: String::new(),
            params: String::new(),
            prefetch: 2,
            output: Output::default(),
            books: BTreeMap::new(),
            engines: presets(),
            enabled: false,
        }
    }
}

impl BookVoice {
    /// Whether this entry still says anything. An entry that does not is
    /// deleted rather than written: a book listed with nothing under it reads
    /// as a setting nobody can find.
    fn speaks_up(&self) -> bool {
        self.engine.is_some()
            || self.name.is_some()
            || self.language.is_some()
            || self.params.is_some()
    }
}

impl Config {
    /// Load the config file, writing the annotated default if there is none.
    ///
    /// Returns the config plus a message to show the reader when something was
    /// wrong with their file.
    pub fn load() -> (Self, Option<String>) {
        let path = paths::config_file();
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => {
                let config = Self::default();
                let note = config.save().err().map(|err| format!("{err:#}"));
                return (config, note);
            }
        };
        match Self::parse(&raw) {
            Ok((mut config, stale)) => {
                // A saved config is the reader's settings, not a snapshot of
                // what readio knew when it was written: engines added since
                // appear, and the parts of an engine that describe its package
                // rather than the reader's taste follow the binary. Written back
                // straight away, so the file says what is actually in force —
                // and so does a file still spelling read-aloud the old way.
                if stale || crate::voice::config::reconcile(&mut config.voice.engines) {
                    let _ = config.save();
                }
                config.clamp();
                (config, None)
            }
            Err(err) => (
                Self::default(),
                Some(format!("{}: {err}", paths::display(&path))),
            ),
        }
    }

    /// Parse a config file, folding what older readios wrote into what this one
    /// reads. The flag is true when something was folded, which is the caller's
    /// cue to write the file back in the current spelling.
    pub fn parse(raw: &str) -> Result<(Self, bool), serde_yaml_ng::Error> {
        let mut config: Self = serde_yaml_ng::from_str(raw)?;
        let stale = config.migrate();
        Ok((config, stale))
    }

    /// Fold settings older readios wrote into the two independent controls.
    fn migrate(&mut self) -> bool {
        let scrolled = self.reading.auto.take();
        let legacy_aloud = self.reading.mode == Mode::Speak;
        if legacy_aloud {
            self.reading.mode = Mode::Auto;
            self.voice.enabled = true;
        } else if scrolled == Some(true) && self.reading.mode == Mode::Manual {
            self.reading.mode = Mode::Auto;
        }
        legacy_aloud || scrolled.is_some()
    }

    /// Write the file back, comments and all.
    pub fn save(&self) -> Result<()> {
        paths::write_atomic(&paths::config_file(), &self.render()?)
    }

    /// Pull hand-edited numbers into ranges the reader can survive.
    pub fn clamp(&mut self) {
        self.reading.speed = if self.reading.speed.is_finite() {
            self.reading.speed.clamp(4.0, 4000.0)
        } else {
            Reading::default().speed
        };
        self.effort.multipliers.clamp_all();
        self.voice.prefetch = self.voice.prefetch.clamp(1, 8);
        self.voice.output.poll = self.voice.output.poll.clamp(1, 120);
        self.images.max_rows = self.images.max_rows.clamp(2, 60);
        // A book entry that says nothing is not a setting, it is clutter.
        self.voice.books.retain(|_, entry| entry.speaks_up());
    }

    pub fn spec(&self, name: &str) -> Option<&EngineSpec> {
        self.voice.engines.get(name)
    }

    // ── who reads ────────────────────────────────────────────────────────────

    /// The voice in force for `book`: its own entry where it has one, the
    /// default everywhere else.
    ///
    /// `None` is the library, where there is no book to have an opinion.
    pub fn voice_for(&self, book: Option<&str>) -> Chosen {
        let entry = book.and_then(|id| self.voice.books.get(id));
        let pick = |chosen: Option<&String>, fallback: &String| {
            chosen.unwrap_or(fallback).trim().to_string()
        };
        Chosen {
            engine: pick(entry.and_then(|e| e.engine.as_ref()), &self.voice.engine),
            name: pick(entry.and_then(|e| e.name.as_ref()), &self.voice.name),
            language: pick(
                entry.and_then(|e| e.language.as_ref()),
                &self.voice.language,
            ),
            params: pick(entry.and_then(|e| e.params.as_ref()), &self.voice.params),
        }
    }

    /// The engine `book` is read with, if it exists in the table.
    pub fn engine_for(&self, book: Option<&str>) -> Option<&EngineSpec> {
        self.spec(&self.voice_for(book).engine)
    }

    /// The voice name to show for `book`: the explicit choice, else whatever
    /// the engine brings of its own.
    pub fn voice_name(&self, book: Option<&str>) -> String {
        let chosen = self.voice_for(book);
        if !chosen.name.is_empty() {
            return chosen.name;
        }
        self.spec(&chosen.engine)
            .map(|spec| spec.voice.clone())
            .unwrap_or_default()
    }

    /// What this book asked for, where it asked for anything.
    pub fn book_voice(&self, id: &str) -> Option<&BookVoice> {
        self.voice.books.get(id)
    }

    /// Engine names, sorted, for error messages and `/voice`.
    pub fn engine_names(&self) -> Vec<String> {
        self.voice.engines.keys().cloned().collect()
    }

    // ── writing a choice down ────────────────────────────────────────────────

    /// Read with this engine, here or everywhere.
    pub fn set_engine(&mut self, scope: Scope<'_>, engine: &str) {
        match scope {
            Scope::Default => self.voice.engine = engine.to_string(),
            Scope::Book { id, title } => {
                self.entry(id, title).engine = Some(engine.to_string());
            }
        }
        self.tidy();
    }

    /// Read in this voice. An empty name hands the choice back to the language
    /// of the page, which is what `/voice auto` means.
    pub fn set_voice_name(&mut self, scope: Scope<'_>, name: &str) {
        match scope {
            Scope::Default => self.voice.name = name.to_string(),
            Scope::Book { id, title } => {
                self.entry(id, title).name = Some(name.to_string());
            }
        }
        self.tidy();
    }

    /// Read in this language, or keep the preset default when empty.
    pub fn set_language(&mut self, scope: Scope<'_>, language: &str) {
        match scope {
            Scope::Default => self.voice.language = language.to_string(),
            Scope::Book { id, title } => {
                self.entry(id, title).language = Some(language.to_string());
            }
        }
        self.tidy();
    }

    /// Store model-specific arguments for one scope. These are applied to a
    /// cloned engine specification at runtime; downloading the engine never
    /// writes them.
    pub fn set_voice_params(&mut self, scope: Scope<'_>, params: &str) {
        match scope {
            Scope::Default => self.voice.params = params.to_string(),
            Scope::Book { id, title } => {
                self.entry(id, title).params = Some(params.to_string());
            }
        }
        self.tidy();
    }

    /// Let this book fall back to the default again. False when it never had an
    /// entry, so the caller can say so rather than claiming to have undone
    /// something.
    pub fn follow_default(&mut self, id: &str) -> bool {
        self.voice.books.remove(id).is_some()
    }

    /// Make what this book is read with the default for every book.
    ///
    /// The entry is removed as it is promoted: leaving it behind would mean two
    /// records of the same decision, and the only interesting thing about two
    /// records of one decision is what happens when they disagree.
    pub fn adopt_as_default(&mut self, id: &str) -> bool {
        let chosen = self.voice_for(Some(id));
        let had = self.voice.books.remove(id).is_some();
        self.voice.engine = chosen.engine;
        self.voice.name = chosen.name;
        self.voice.language = chosen.language;
        self.voice.params = chosen.params;
        had
    }

    /// The book's entry, created empty if this is the first thing it asks for.
    fn entry(&mut self, id: &str, title: &str) -> &mut BookVoice {
        let entry = self.voice.books.entry(id.to_string()).or_default();
        if entry.title.is_empty() {
            entry.title = title.to_string();
        }
        entry
    }

    /// Drop book entries that no longer differ from the default, and empty ones.
    ///
    /// A book that asks for exactly what everyone else gets is not a preference,
    /// it is a copy — and a copy that stops following the default the next time
    /// the default changes, which is a surprise nobody asked for.
    fn tidy(&mut self) {
        let default = self.voice_for(None);
        self.voice.books.retain(|_, entry| {
            if let Some(engine) = entry.engine.as_ref()
                && engine.trim() == default.engine
            {
                entry.engine = None;
            }
            if let Some(name) = entry.name.as_ref()
                && name.trim() == default.name
            {
                entry.name = None;
            }
            if let Some(language) = entry.language.as_ref()
                && language.trim() == default.language
            {
                entry.language = None;
            }
            if let Some(params) = entry.params.as_ref()
                && params.trim() == default.params
            {
                entry.params = None;
            }
            entry.speaks_up()
        });
    }

    /// Render the file: scalars written by hand so the comments survive a save,
    /// the engine table and the per-book voices serialized.
    pub fn render(&self) -> Result<String> {
        let engines = indent(
            &serde_yaml_ng::to_string(&self.voice.engines)
                .context("cannot serialise the engine table")?,
        );
        // Absent until a book asks for something, because a `books: {}` in a
        // file nobody has used the feature in is a question with no question
        // mark. The commented example in the notes above is what teaches it.
        let books = if self.voice.books.is_empty() {
            String::new()
        } else {
            format!(
                "{BOOKS_NOTE}  books:\n{}",
                indent(
                    &serde_yaml_ng::to_string(&self.voice.books)
                        .context("cannot serialise the per-book voices")?
                )
            )
        };

        Ok(format!(
            "{HEADER}\n\
             language: {lang}\n\
             \n\
             reading:\n\
             {READING_NOTE}\
             \x20 speed: {speed}\n\
             \x20 mode: {mode}\n\
             \n\
             effort:\n\
             {EFFORT_NOTE}\
             \x20 level: {level}\n\
             \x20 multipliers:\n\
             \x20   minimal: {minimal}\n\
             \x20   low: {low}\n\
             \x20   medium: {medium}\n\
             \x20   high: {high}\n\
             \x20   xhigh: {xhigh}\n\
             \x20   max: {max}\n\
             \n\
             images:\n\
             {IMAGES_NOTE}\
             \x20 enabled: {images}\n\
             \x20 max_rows: {rows}\n\
             \n\
             voice:\n\
             {VOICE_NOTE}\
             \x20 engine: {engine}\n\
             \x20 name: \"{name}\"\n\
             \x20 language: {voice_lang}\n\
             \x20 params: \"{voice_params}\"\n\
             \x20 enabled: {voice_enabled}\n\
             \x20 prefetch: {prefetch}\n\
             {OUTPUT_NOTE}\
             \x20 output:\n\
             \x20   allow:{allow}\n\
             \x20   query: \"{query}\"\n\
             \x20   poll: {poll}\n\
             \x20   on_mismatch: {mismatch}\n\
             {books}\
             {ENGINES_NOTE}\
             \x20 engines:\n{engines}\
             {input_log}",
            lang = self.language.code(),
            speed = trim_float(self.reading.speed),
            mode = self.reading.mode.code(),
            level = self.effort.level.code(),
            minimal = trim_float(self.effort.multipliers.minimal),
            low = trim_float(self.effort.multipliers.low),
            medium = trim_float(self.effort.multipliers.medium),
            high = trim_float(self.effort.multipliers.high),
            xhigh = trim_float(self.effort.multipliers.xhigh),
            max = trim_float(self.effort.multipliers.max),
            images = self.images.enabled,
            rows = self.images.max_rows,
            engine = self.voice.engine,
            name = self.voice.name,
            voice_lang = self.voice.language,
            voice_params = self.voice.params.replace('"', "\\\""),
            voice_enabled = self.voice.enabled,
            prefetch = self.voice.prefetch,
            allow = if self.voice.output.allow.is_empty() {
                " []".to_string()
            } else {
                self.voice
                    .output
                    .allow
                    .iter()
                    .map(|rule| format!("\n      - \"{rule}\""))
                    .collect::<String>()
            },
            query = self.voice.output.query,
            poll = self.voice.output.poll,
            mismatch = match self.voice.output.on_mismatch {
                crate::voice::device::Mismatch::Silence => "silence",
                crate::voice::device::Mismatch::Play => "play",
            },
            input_log = match &self.input_log {
                Some(path) => format!(
                    "\n# 调试：每个按键都记到这个文件 / log every key event\ninput_log: {path}\n"
                ),
                None => String::new(),
            },
        ))
    }
}

/// Serialized YAML, moved one nesting level in so it can sit under a key.
fn indent(block: &str) -> String {
    block
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                "\n".to_string()
            } else {
                format!("    {line}\n")
            }
        })
        .collect()
}

/// `46` rather than `46.0`, and `1.5` rather than `1.5000001`.
/// A number written the way a person would write it: `46`, `1.5`, `0.85`.
///
/// Trailing zeros matter here because this file is read by people — a ladder of
/// `2.50  2  1.50  1  0.85  0.70` looks like six different kinds of number.
fn trim_float(value: f32) -> String {
    if (value - value.round()).abs() < f32::EPSILON {
        return format!("{:.0}", value.round());
    }
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

const HEADER: &str = "\
# readio configuration / 配置
#
# The one configuration file — readio reads no environment variables. Edit it
# and restart, or let commands like /speed, /voice and /mode write it for you.
#
# 这是唯一的配置文件，readio 不读任何环境变量。改完存盘，下次启动生效；
# /speed、/voice、/mode 之类的命令也会写回这里。
#
# A different host directory / 换目录: readio --home <dir>

# en or zh / 界面语言";

const READING_NOTE: &str =
    "  # speed: characters per second at effort high; the effort multiplier scales it,
  #   and read-aloud takes the pace from the audio instead
  # speed: 强度 high 时每秒吐出多少字；其它档按倍数缩放，朗读时改为跟着音频走
  # mode: manual | auto — whether the next passage arrives by itself.
  #   shift+tab toggles it. Voice is a separate enabled switch below.
  # mode: manual | auto，只决定读完后会不会自动取下一段；shift+tab 切换。
  #   朗读由下面 Voice 的 enabled 单独控制。
";

const EFFORT_NOTE: &str = "  # Reading pace, shown as the reasoning effort of the model.
  # Each level is a multiplier: the text appears at speed × multiplier, and
  # read-aloud plays at the multiplier itself. More effort reads more slowly.
  # 阅读节奏，界面上显示为模型的推理强度。每档就是一个倍数：
  # 文字按 speed × 倍数 吐出，朗读直接按这个倍数播放。强度越高读得越慢。
  # level: minimal | low | medium | high | xhigh | max —— /effort 或 ^r 切换
";

const IMAGES_NOTE: &str =
    "  # illustrations are drawn with half-block characters, max_rows tall at most
  # 书里的插图用半块字符画在终端里；max_rows 是最高几行
";

const VOICE_NOTE: &str =
    "  # Who reads to you. One section, because which engine and which voice are
  # one question: a Kokoro zf_* voice is Mandarin and an af_* one is English,
  # so choosing a voice chooses an engine and a language with it.
  # 朗读设置都在这一节：引擎、音色、语种本来就是一件事，分开配迟早互相打架。
  # readio ships no model — it drives whatever engine you installed, so
  # switching models is an edit here rather than a new release.
  # readio 自己不带模型，只调用你装好的引擎，所以换模型就是改这里。
  # A fresh config selects nothing. /voice opens one workspace: downloads on
  # the left, explicit global/per-book configuration on the right.
  # 新配置默认不选引擎；/voice 左边下载模型，右边明确选择全局或单书配置。
  # engine: empty, or one of the `engines:` below / 留空或选下面的引擎
  # name: a voice that engine knows; empty keeps the model's declared default.
  # language: a key such as zh or en; text never changes it sentence by sentence.
  # name 留空使用模型默认音色；language 写 zh 或 en，不按句子自动切换。
  # params: model-specific arguments saved by the Voice form; empty is default.
  # params 是模型支持的独立参数；留空使用模型默认值。
  # enabled: Voice on/off, independent from manual/auto continuation.
  # enabled 单独开关朗读，不会改变上面的手动/自动阅读。
  # The playback speed is not here: it is the effort multiplier above, so one
  # ladder covers reading and listening / 倍速在上面的 effort，读和听共用一套。
  # prefetch: sentences rendered ahead of the one playing, so there is no gap
  # at a sentence boundary. Raise it if your engine is slow / 提前合成几句
";

const BOOKS_NOTE: &str =
    "  # One book, its own voice. Keyed by the same content id the library uses,
  # and only what that book asked for is stored — anything absent follows the
  # settings above. The Voice form selects the book explicitly.
  # 单独给某本书配音，键是书的内容 id；这里没写的项就跟随上面的默认设置。
  # Voice 表单里明确选择某本书后写在这里；“沿用全局”会删掉这本书的设置。
";

const OUTPUT_NOTE: &str =
    "  # Only speak on these outputs, so swapping headphones cannot leak a sentence
  # into the room. Substrings of the device name work, as does a transport like
  # bluetooth or usb. Empty [] means no restriction. An output that cannot be
  # identified is treated as not allowed.
  # 只在这些输出设备上出声，防止换耳机时不小心外放；留空 [] 表示不限制。
  #   allow:
  #     - AirPods
  #     - bluetooth
  # query: a command printing the current device name; leave empty on macOS/Linux
  # poll: seconds between checks / 每几秒检查一次
  # on_mismatch: silence (default) | play — speak anyway but say so
";

const ENGINES_NOTE: &str = "  # placeholders / 占位符:
  #   {text} the sentence · {out} the wav to write · {voice} · {rate}
  #   {model} model path · {json} OpenAI-compatible body · {file} clip to play
  #   {lang} the language entry below, spliced whole: flag and value
  #   {words} the rate as words per minute, for engines that count them that way
  # stdin: true feeds the sentence on stdin instead of as an argument (piper)
  #
  # serve: a command that stays running and renders sentence after sentence,
  # spoken to in lines of JSON. Starting a model for every sentence costs more
  # than saying the sentence does, and an engine slower than speech can never be
  # caught up with. Kept resident, kokoro goes from half of real time to three
  # times it. {python} is the interpreter the engine was installed into,
  # {worker} is the script readio writes into ~/.readio/engines, and {model}
  # is loaded once when that process starts.
  # serve: 常驻进程，逐句用 JSON 交互；模型只加载一次。
  #
  # languages: what changes when the page changes language. `lang` holds the
  # flag *and* its value, so an engine that spells it another way still works,
  # and an empty one passes nothing at all / 换语种时跟着换的音色与参数。
  #
  # your own engine / 自己的引擎:
  #   mine:
  #     synth: my-tts --in {text} --wav {out}
  #     play: afplay {file}
";

#[cfg(test)]
mod tests {
    use super::*;

    /// A book id, spelled the way `crate::book::content_id` spells one.
    const BOOK: &str = "3f9a1c7d5e2b4a10";

    #[test]
    fn defaults_are_usable_without_a_file() {
        let config = Config::default();
        assert_eq!(config.language, Lang::En, "English is the default");
        assert_eq!(
            config.reading.mode,
            Mode::Manual,
            "nothing moves until the reader says so"
        );
        assert!(config.images.enabled, "images are on by default");
        assert!(
            config.engine_for(None).is_none(),
            "a fresh reader has not chosen or downloaded a speech engine"
        );
        assert!(
            config.voice_name(None).is_empty(),
            "no engine also means no implicit voice"
        );
    }

    /// readio ships engine recipes, not model weights. MOSS and Kokoro are the
    /// recommended Chinese and English choices, but neither becomes active
    /// until the reader explicitly downloads and selects it.
    #[test]
    fn a_fresh_reader_offers_moss_and_kokoro_without_choosing_either() {
        let config = Config::default();
        assert!(config.voice.engine.is_empty());
        assert!(config.voice.name.is_empty());
        assert!(config.voice.books.is_empty());
        assert!(
            config.voice.engines.contains_key("moss")
                && config.voice.engines.contains_key("kokoro"),
            "both recommended engines must be available to choose"
        );
    }

    #[test]
    fn the_rendered_file_parses_back_into_itself() {
        let mut config = Config {
            language: Lang::En,
            ..Config::default()
        };
        config.reading.speed = 72.0;
        config.reading.mode = Mode::Auto;
        config.voice.enabled = true;
        config.voice.name = "zf_xiaoyi".to_string();
        config.set_voice_name(
            Scope::Book {
                id: BOOK,
                title: "论语",
            },
            "af_heart",
        );
        config.effort.level = crate::effort::Effort::Xhigh;
        config
            .effort
            .multipliers
            .set(crate::effort::Effort::Xhigh, 0.9);
        config.images.max_rows = 24;

        let body = config.render().expect("render");
        let (back, stale) = Config::parse(&body).expect("the file we write must parse");
        assert_eq!(back, config, "a save/load round trip must be lossless");
        assert!(!stale, "readio's own output is never in an older spelling");
        assert!(
            body.contains("# readio configuration"),
            "comments belong in the file:\n{body}"
        );
        assert!(
            body.contains("speed: 72"),
            "whole numbers should stay whole:\n{body}"
        );
        assert!(
            body.contains("mode: auto") && body.contains("enabled: true"),
            "the mode is a setting, written where the reader can read it:\n{body}"
        );
        // A file full of content ids is unreadable without them.
        assert!(
            body.contains("论语"),
            "a per-book entry should say which book it is:\n{body}"
        );
    }

    #[test]
    fn a_partial_file_fills_in_the_rest() {
        let raw = "language: en\nreading:\n  speed: 90\n";
        let (config, _) = Config::parse(raw).expect("partial config should parse");
        assert_eq!(config.language, Lang::En);
        assert_eq!(config.reading.speed, 90.0);
        assert_eq!(
            config.reading.mode,
            Mode::Manual,
            "unset fields take their default"
        );
        assert!(config.voice.engine.is_empty());
        assert!(
            config.voice.engines.contains_key("moss")
                && config.voice.engines.contains_key("kokoro"),
            "a file that mentions no engines still gets the presets without selecting one"
        );
    }

    #[test]
    fn an_empty_file_is_all_defaults() {
        let config: Config = serde_yaml_ng::from_str("{}").expect("empty map should parse");
        assert_eq!(config, Config::default());
    }

    /// Old files already carried two independent switches. Migration keeps both
    /// facts rather than collapsing Voice into continuation again.
    #[test]
    fn the_old_pair_of_switches_remains_two_independent_settings() {
        let (config, stale) = Config::parse("tts:\n  enabled: true\n").expect("parse");
        assert_eq!(config.reading.mode, Mode::Manual);
        assert!(config.voice.enabled, "they were listening");
        assert!(
            !stale,
            "the section alias already maps directly to independent Voice state"
        );

        let (config, stale) = Config::parse("reading:\n  auto: true\n").expect("parse");
        assert_eq!(config.reading.mode, Mode::Auto);
        assert!(!config.voice.enabled);
        assert!(stale);

        let (config, _) =
            Config::parse("reading:\n  auto: true\ntts:\n  enabled: true\n").expect("parse");
        assert_eq!(config.reading.mode, Mode::Auto);
        assert!(config.voice.enabled);

        // The short-lived three-state spelling maps to continuous read-aloud:
        // auto continuation with Voice enabled.
        let (config, stale) = Config::parse("reading:\n  mode: aloud\n").expect("parse");
        assert_eq!(config.reading.mode, Mode::Auto);
        assert!(config.voice.enabled);
        assert!(stale);

        let (config, _) = Config::parse("tts:\n  enabled: true\n").expect("parse");
        let body = config.render().expect("render");
        let voice_section = body.split("\nvoice:\n").nth(1).expect("a voice section");
        assert!(
            voice_section.contains("enabled: true"),
            "Voice remains explicit and independent:\n{voice_section}"
        );
        let (again, stale) = Config::parse(&body).expect("parse what we wrote");
        assert_eq!(again.reading.mode, Mode::Manual);
        assert!(again.voice.enabled);
        assert!(!stale, "with nothing left to fold");
    }

    /// The section was renamed, not split: an engine and a voice were always one
    /// choice, so a file that says `tts:` is not wrong, only older.
    #[test]
    fn a_file_that_still_says_tts_is_read_as_voice() {
        let raw = "tts:\n  engine: espeak\n  voice: af_heart\n  language: en\n";
        let (config, _) = Config::parse(raw).expect("parse");
        assert_eq!(config.voice.engine, "espeak");
        assert_eq!(config.voice.name, "af_heart", "`voice:` is `name:`");
        assert_eq!(config.voice.language, "en");
    }

    /// A shelf is not read in one voice, and the reader who sets a book up should
    /// not have to set it up again every time they open it.
    #[test]
    fn a_book_can_be_read_in_its_own_voice() {
        let mut config = Config::default();
        config.voice.engine = "kokoro".to_string();
        config.voice.name = "zf_xiaoxiao".to_string();

        config.set_engine(
            Scope::Book {
                id: BOOK,
                title: "The Analects",
            },
            "espeak",
        );
        config.set_voice_name(
            Scope::Book {
                id: BOOK,
                title: "The Analects",
            },
            "af_heart",
        );

        let mine = config.voice_for(Some(BOOK));
        assert_eq!(mine.engine, "espeak");
        assert_eq!(mine.name, "af_heart");
        assert_eq!(
            mine.language, "",
            "what the book said nothing about follows the default"
        );

        let everyone = config.voice_for(None);
        assert_eq!(everyone.engine, "kokoro", "the default is untouched");
        assert_eq!(everyone.name, "zf_xiaoxiao");
        assert_eq!(
            config.voice_for(Some("another-book")),
            everyone,
            "and so is every other book"
        );
    }

    #[test]
    fn model_parameters_can_be_global_or_belong_to_one_book() {
        let mut config = Config::default();
        config.set_voice_params(Scope::Default, "temperature=0.8");
        config.set_voice_params(
            Scope::Book {
                id: BOOK,
                title: "论语",
            },
            "temperature=0.65 top_p=0.9",
        );

        assert_eq!(config.voice_for(None).params, "temperature=0.8");
        assert_eq!(
            config.voice_for(Some(BOOK)).params,
            "temperature=0.65 top_p=0.9"
        );
        assert_eq!(
            config.voice_for(Some("another-book")).params,
            "temperature=0.8"
        );

        let body = config.render().expect("render");
        let (back, _) = Config::parse(&body).expect("parse");
        assert_eq!(
            back.voice_for(Some(BOOK)).params,
            "temperature=0.65 top_p=0.9"
        );
    }

    /// The two ways a choice moves between one book and all of them. Promoting
    /// removes the entry: two records of one decision is exactly what this whole
    /// section exists to avoid.
    #[test]
    fn a_book_can_hand_its_voice_to_everyone_or_give_it_back() {
        let mut config = Config::default();
        config.set_engine(
            Scope::Book {
                id: BOOK,
                title: "论语",
            },
            "espeak",
        );
        assert!(config.book_voice(BOOK).is_some());

        config.adopt_as_default(BOOK);
        assert_eq!(config.voice.engine, "espeak", "everyone reads with it now");
        assert!(
            config.book_voice(BOOK).is_none(),
            "and the book stops carrying a copy"
        );

        config.set_voice_name(
            Scope::Book {
                id: BOOK,
                title: "论语",
            },
            "af_heart",
        );
        assert!(config.follow_default(BOOK), "there was something to undo");
        assert!(!config.follow_default(BOOK), "and now there is not");
        assert_eq!(
            config.voice_for(Some(BOOK)),
            config.voice_for(None),
            "the book is back to whatever everyone gets"
        );
    }

    /// A book asking for exactly what everyone gets is not a preference, it is a
    /// copy — and a copy stops following the default the next time the default
    /// changes, which is a surprise nobody asked for.
    #[test]
    fn a_book_that_asks_for_the_default_stops_asking() {
        let mut config = Config::default();
        config.voice.engine = "kokoro".to_string();
        config.set_engine(
            Scope::Book {
                id: BOOK,
                title: "论语",
            },
            "kokoro",
        );
        assert!(
            config.book_voice(BOOK).is_none(),
            "nothing to record: {:?}",
            config.voice.books
        );
    }

    #[test]
    fn a_hand_added_engine_survives_the_preset_merge() {
        let raw = "voice:\n  engine: mine\n  engines:\n    mine:\n      synth: my-tts {text} {out}\n      play: afplay {file}\n";
        let (mut config, _) = Config::parse(raw).expect("parse");
        crate::voice::config::reconcile(&mut config.voice.engines);
        assert!(
            config.engine_for(None).is_some(),
            "the reader's own engine must not be replaced by a preset"
        );
        assert_eq!(
            config.voice.engines["mine"].synth, "my-tts {text} {out}",
            "and it must survive intact"
        );
        assert!(
            config.engine_names().len() > 1,
            "presets fill in around it: {:?}",
            config.engine_names()
        );
    }

    /// The bug this exists to prevent: a config written by an older readio
    /// froze every engine at the definition that build shipped. The fields that
    /// say how to install an engine had not been invented yet, so an upgraded
    /// machine showed every engine as "a server, nothing to install" — and the
    /// command lines, none of which matched the engines' real CLIs, stayed
    /// broken forever.
    #[test]
    fn a_config_from_an_older_readio_catches_up_with_the_binary() {
        let raw = "\
tts:
  engine: kokoro
  engines:
    kokoro:
      synth: kokoro-tts --text {text} --output {out} --voice {voice} --speed {rate}
      play: afplay {file}
      voice: my_own_voice
      about: 老版本写下的说明
    mine:
      synth: my-tts {text} {out}
      play: afplay {file}
";
        let (mut config, _) = Config::parse(raw).expect("parse");
        assert!(config.voice.engines["kokoro"].pip.is_empty(), "as saved");

        assert!(
            crate::voice::config::reconcile(&mut config.voice.engines),
            "something changed, so the file should be written back"
        );

        let kokoro = &config.voice.engines["kokoro"];
        assert_eq!(
            kokoro.synth,
            crate::voice::config::presets()["kokoro"].synth,
            "a command readio shipped and later corrected is replaced"
        );
        assert!(!kokoro.pip.is_empty(), "and it knows how to install itself");
        assert_eq!(
            kokoro.voice, "my_own_voice",
            "but a voice they chose is still a choice"
        );
        assert_eq!(
            config.voice.engines["mine"].synth, "my-tts {text} {out}",
            "while an engine the reader wrote is left alone"
        );
    }

    /// The other half of the rule: a reader who changed a preset's command line
    /// keeps it. Only the parts that describe the package catch up.
    #[test]
    fn a_reader_who_edited_a_preset_keeps_their_edit() {
        let raw = "\
voice:
  engines:
    piper:
      synth: piper --model /opt/voices/mine.onnx --output-file {out}
      play: aplay {file}
      model: /opt/voices/mine.onnx
      stdin: true
";
        let (mut config, _) = Config::parse(raw).expect("parse");
        crate::voice::config::reconcile(&mut config.voice.engines);

        let piper = &config.voice.engines["piper"];
        assert_eq!(
            piper.synth, "piper --model /opt/voices/mine.onnx --output-file {out}",
            "their line is their line"
        );
        assert_eq!(piper.play, "aplay {file}", "and so is their player");
        assert_eq!(piper.model, "/opt/voices/mine.onnx", "and their model");
        assert_eq!(piper.pip, "piper-tts", "but readio still knows the package");
        assert!(
            piper.fetch.is_empty(),
            "and does not insist on a voice file it chose for them"
        );
    }

    /// The command line that shipped in beta.3 ran, which is why it is easy to
    /// miss that it ran wrong: `--lang` was never passed, so Kokoro read every
    /// Chinese page with English phonemes. Anyone who used that build has the
    /// line saved, and an upgrade has to repair it.
    #[test]
    fn a_config_that_predates_the_language_table_learns_to_read_chinese() {
        let raw = "\
tts:
  engine: kokoro
  engines:
    kokoro:
      synth: kokoro-tts - {out} --voice {voice} --speed {rate} --format wav --model ~/.readio/voices/kokoro-v1.0.onnx --voices ~/.readio/voices/voices-v1.0.bin {extra}
      play: afplay {file}
      voice: zf_xiaobei
      stdin: true
";
        let (mut config, _) = Config::parse(raw).expect("parse");
        assert!(
            config.voice.engines["kokoro"].languages.is_empty(),
            "as saved"
        );

        assert!(crate::voice::config::reconcile(&mut config.voice.engines));

        let kokoro = &config.voice.engines["kokoro"];
        assert!(
            kokoro.synth.contains("{lang}"),
            "the corrected line has somewhere to put the language: {}",
            kokoro.synth
        );
        assert_eq!(
            kokoro.languages["zh"].lang, "--lang cmn",
            "and knows what to put there for Chinese"
        );
        assert_eq!(
            kokoro.voice, "zf_xiaobei",
            "the voice they were reading with is still theirs"
        );
    }

    #[test]
    fn hand_edited_nonsense_is_clamped_not_obeyed() {
        let raw = "reading:\n  speed: 900000\neffort:\n  multipliers:\n    minimal: 40\n    max: 0\nvoice:\n  prefetch: 500\n  books:\n    empty-entry:\n      title: 一本没说什么的书\nimages:\n  max_rows: 900\n";
        let (mut config, _) = Config::parse(raw).expect("parse");
        config.clamp();
        assert_eq!(config.reading.speed, 4000.0);
        assert_eq!(
            config
                .effort
                .multipliers
                .get(crate::effort::Effort::Minimal),
            3.0
        );
        assert_eq!(
            config.effort.multipliers.get(crate::effort::Effort::Max),
            0.5
        );
        assert_eq!(config.voice.prefetch, 8);
        assert_eq!(config.images.max_rows, 60);
        assert!(
            config.voice.books.is_empty(),
            "a book entry that says nothing is clutter, not a setting"
        );
    }

    #[test]
    fn an_unknown_engine_name_is_visible_rather_than_silently_default() {
        let mut config = Config::default();
        config.voice.engine = "nope".to_string();
        assert!(config.engine_for(None).is_none());
        assert!(config.voice_name(None).is_empty());
        assert!(config.engine_names().contains(&"kokoro".to_string()));
    }
}
