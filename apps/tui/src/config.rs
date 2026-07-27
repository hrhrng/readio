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

use crate::i18n::Lang;
use crate::paths;
use crate::tts::config::{EngineSpec, presets};
use crate::tts::device::Output;

/// Everything readio can be told to do differently.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Interface language.
    pub language: Lang,
    pub reading: Reading,
    pub images: Images,
    pub tts: Tts,
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
    /// Keep queueing the next passage without being asked.
    pub auto: bool,
}

/// Whether and how large in-terminal images are drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Images {
    pub enabled: bool,
    /// Tallest an illustration may be, in terminal rows.
    pub max_rows: u16,
}

/// Read-aloud settings and the engine table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tts {
    /// Start with read-aloud on.
    pub enabled: bool,
    /// Which entry of `engines` to use.
    pub engine: String,
    /// Overrides the engine's own default voice when non-empty.
    pub voice: String,
    /// Speaking rate multiplier, where the engine supports it.
    pub rate: f32,
    /// How many sentences to synthesize ahead of playback.
    pub prefetch: usize,
    /// Which audio outputs may be spoken through.
    pub output: Output,
    pub engines: BTreeMap<String, EngineSpec>,
}

impl Default for Reading {
    fn default() -> Self {
        Self {
            speed: 46.0,
            auto: false,
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

impl Default for Tts {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: "kokoro".to_string(),
            voice: String::new(),
            rate: 1.0,
            prefetch: 2,
            output: Output::default(),
            engines: presets(),
        }
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
        match serde_yaml_ng::from_str::<Self>(&raw) {
            Ok(mut config) => {
                // Presets fill gaps, so a config written by an older readio
                // still knows about engines added since.
                for (name, spec) in presets() {
                    config.tts.engines.entry(name).or_insert(spec);
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
        self.tts.rate = if self.tts.rate.is_finite() {
            self.tts.rate.clamp(0.5, 3.0)
        } else {
            1.0
        };
        self.tts.prefetch = self.tts.prefetch.clamp(1, 8);
        self.tts.output.poll = self.tts.output.poll.clamp(1, 120);
        self.images.max_rows = self.images.max_rows.clamp(2, 60);
    }

    pub fn spec(&self, name: &str) -> Option<&EngineSpec> {
        self.tts.engines.get(name)
    }

    /// The engine in use, if it exists.
    pub fn active_engine(&self) -> Option<&EngineSpec> {
        self.spec(&self.tts.engine)
    }

    /// Voice for the active engine: the explicit choice, else the preset's.
    pub fn active_voice(&self) -> String {
        if !self.tts.voice.trim().is_empty() {
            return self.tts.voice.clone();
        }
        self.active_engine()
            .map(|s| s.voice.clone())
            .unwrap_or_default()
    }

    /// Engine names, sorted, for error messages and `/tts`.
    pub fn engine_names(&self) -> Vec<String> {
        self.tts.engines.keys().cloned().collect()
    }

    /// Render the file: scalars written by hand so the comments survive a save,
    /// the engine table serialized.
    pub fn render(&self) -> Result<String> {
        let engines = serde_yaml_ng::to_string(&self.tts.engines)
            .context("cannot serialise the engine table")?;
        let engines: String = engines
            .lines()
            .map(|line| {
                if line.trim().is_empty() {
                    "\n".to_string()
                } else {
                    format!("    {line}\n")
                }
            })
            .collect();

        Ok(format!(
            "{HEADER}\n\
             language: {lang}\n\
             \n\
             reading:\n\
             {READING_NOTE}\
             \x20 speed: {speed}\n\
             \x20 auto: {auto}\n\
             \n\
             images:\n\
             {IMAGES_NOTE}\
             \x20 enabled: {images}\n\
             \x20 max_rows: {rows}\n\
             \n\
             tts:\n\
             {TTS_NOTE}\
             \x20 enabled: {tts}\n\
             \x20 engine: {engine}\n\
             \x20 voice: \"{voice}\"\n\
             \x20 rate: {rate}\n\
             \x20 prefetch: {prefetch}\n\
             {OUTPUT_NOTE}\
             \x20 output:\n\
             \x20   allow:{allow}\n\
             \x20   query: \"{query}\"\n\
             \x20   poll: {poll}\n\
             \x20   on_mismatch: {mismatch}\n\
             {ENGINES_NOTE}\
             \x20 engines:\n{engines}\
             {input_log}",
            lang = self.language.code(),
            speed = trim_float(self.reading.speed),
            auto = self.reading.auto,
            images = self.images.enabled,
            rows = self.images.max_rows,
            tts = self.tts.enabled,
            engine = self.tts.engine,
            voice = self.tts.voice,
            rate = trim_float(self.tts.rate),
            prefetch = self.tts.prefetch,
            allow = if self.tts.output.allow.is_empty() {
                " []".to_string()
            } else {
                self.tts
                    .output
                    .allow
                    .iter()
                    .map(|rule| format!("\n      - \"{rule}\""))
                    .collect::<String>()
            },
            query = self.tts.output.query,
            poll = self.tts.output.poll,
            mismatch = match self.tts.output.on_mismatch {
                crate::tts::device::Mismatch::Silence => "silence",
                crate::tts::device::Mismatch::Play => "play",
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

/// `46` rather than `46.0`, and `1.5` rather than `1.5000001`.
fn trim_float(value: f32) -> String {
    if (value - value.round()).abs() < f32::EPSILON {
        format!("{:.0}", value.round())
    } else {
        format!("{value:.2}")
    }
}

const HEADER: &str = "\
# readio configuration / 配置
#
# The one configuration file — readio reads no environment variables. Edit it
# and restart, or let commands like /speed, /voice and /tts write it for you.
#
# 这是唯一的配置文件，readio 不读任何环境变量。改完存盘，下次启动生效；
# /speed、/voice、/tts 之类的命令也会写回这里。
#
# A different host directory / 换目录: readio --home <dir>

# en or zh / 界面语言";

const READING_NOTE: &str = "  # speed: characters revealed per second; read-aloud overrides it
  # speed: 每秒吐出多少字；朗读打开时按音频长度自动接管
  # auto: keep going without pressing enter / 读完一段自动接着读
";

const IMAGES_NOTE: &str =
    "  # illustrations are drawn with half-block characters, max_rows tall at most
  # 书里的插图用半块字符画在终端里；max_rows 是最高几行
";

const TTS_NOTE: &str = "  # readio ships no model: it drives whatever engine you installed, so
  # switching models is an edit here rather than a new release.
  # readio 自己不带模型，只调用你装好的引擎，所以换模型就是改这里。
  # an empty voice means the engine default / voice 留空就用引擎默认音色
  # rate is the playback speed, 0.5 to 3.0; ^r cycles 0.75× 1× 1.25× 1.5× 2×
  # rate 是朗读倍速（0.5~3.0）；^r 在 0.75× 1× 1.25× 1.5× 2× 之间循环
  # prefetch: sentences rendered ahead of the one playing, so there is no gap
  # at a sentence boundary. Raise it if your engine is slow / 提前合成几句
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
  # stdin: true feeds the sentence on stdin instead of as an argument (piper)
  #
  # your own engine / 自己的引擎:
  #   mine:
  #     synth: my-tts --in {text} --wav {out}
  #     play: afplay {file}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_usable_without_a_file() {
        let config = Config::default();
        assert_eq!(config.language, Lang::En, "English is the default");
        assert!(!config.tts.enabled, "read-aloud is opt-in");
        assert!(config.images.enabled, "images are on by default");
        assert!(
            config.active_engine().is_some(),
            "the default engine must exist in the table"
        );
        assert!(
            !config.active_voice().is_empty(),
            "the default engine brings its own voice"
        );
    }

    #[test]
    fn the_rendered_file_parses_back_into_itself() {
        let mut config = Config {
            language: Lang::En,
            ..Config::default()
        };
        config.reading.speed = 72.0;
        config.tts.enabled = true;
        config.tts.voice = "zf_xiaoyi".to_string();
        config.tts.rate = 1.25;
        config.images.max_rows = 24;

        let body = config.render().expect("render");
        let back: Config = serde_yaml_ng::from_str(&body).expect("the file we write must parse");
        assert_eq!(back, config, "a save/load round trip must be lossless");
        assert!(
            body.contains("# readio configuration"),
            "comments belong in the file:\n{body}"
        );
        assert!(
            body.contains("speed: 72"),
            "whole numbers should stay whole:\n{body}"
        );
    }

    #[test]
    fn a_partial_file_fills_in_the_rest() {
        let raw = "language: en\nreading:\n  speed: 90\n";
        let config: Config = serde_yaml_ng::from_str(raw).expect("partial config should parse");
        assert_eq!(config.language, Lang::En);
        assert_eq!(config.reading.speed, 90.0);
        assert!(!config.reading.auto, "unset fields take their default");
        assert_eq!(config.tts.engine, "kokoro");
        assert!(
            config.active_engine().is_some(),
            "a file that mentions no engines still gets the presets"
        );
    }

    #[test]
    fn an_empty_file_is_all_defaults() {
        let config: Config = serde_yaml_ng::from_str("{}").expect("empty map should parse");
        assert_eq!(config, Config::default());
    }

    #[test]
    fn a_hand_added_engine_survives_the_preset_merge() {
        let raw = "tts:\n  engine: mine\n  engines:\n    mine:\n      synth: my-tts {text} {out}\n      play: afplay {file}\n";
        let mut config: Config = serde_yaml_ng::from_str(raw).expect("parse");
        for (name, spec) in presets() {
            config.tts.engines.entry(name).or_insert(spec);
        }
        assert!(
            config.active_engine().is_some(),
            "the reader's own engine must not be replaced by a preset"
        );
        assert!(
            config.engine_names().len() > 1,
            "presets fill in around it: {:?}",
            config.engine_names()
        );
    }

    #[test]
    fn hand_edited_nonsense_is_clamped_not_obeyed() {
        let raw = "reading:\n  speed: 900000\ntts:\n  rate: 40\n  prefetch: 500\nimages:\n  max_rows: 900\n";
        let mut config: Config = serde_yaml_ng::from_str(raw).expect("parse");
        config.clamp();
        assert_eq!(config.reading.speed, 4000.0);
        assert_eq!(config.tts.rate, 3.0);
        assert_eq!(config.tts.prefetch, 8);
        assert_eq!(config.images.max_rows, 60);
    }

    #[test]
    fn an_unknown_engine_name_is_visible_rather_than_silently_default() {
        let mut config = Config::default();
        config.tts.engine = "nope".to_string();
        assert!(config.active_engine().is_none());
        assert!(config.active_voice().is_empty());
        assert!(config.engine_names().contains(&"kokoro".to_string()));
    }
}
