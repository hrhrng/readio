//! Engine definitions and the built-in presets.
//!
//! The settings that select among these live in [`crate::config`]
//! (`~/.readio/config.yaml`); this module only describes how to drive an engine.
//!
//! readio ships presets for small local models rather than a bundled model:
//! the binary stays ~2 MB, and swapping in whatever is state of the art next
//! month is a one-line edit instead of a release. Every preset is just a
//! command template, so an engine readio has never heard of works too.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// How to drive one speech engine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineSpec {
    /// Command that writes audio. Placeholders: `{text}` `{out}` `{voice}`
    /// `{rate}` `{model}` `{lang}` `{extra}`.
    pub synth: String,
    /// Command that stays running and renders sentence after sentence, spoken
    /// to in lines of JSON. When this is set it replaces `synth`, which stays
    /// as the way to find the engine and as the fallback for a reader who
    /// deletes this line.
    ///
    /// `{python}` is the interpreter behind `synth`'s own program — the one the
    /// engine's package was installed into — and `{worker}` is readio's worker
    /// script, written out beside the config.
    #[serde(default)]
    pub serve: String,
    /// Command that plays a file, `{file}` being the clip. Empty means the
    /// synth command played it itself.
    #[serde(default)]
    pub play: String,
    /// Default voice when the config leaves `voice` empty.
    #[serde(default)]
    pub voice: String,
    /// Model path or name, for engines that need one.
    #[serde(default)]
    pub model: String,
    /// Anything else to splice in via `{extra}`.
    #[serde(default)]
    pub extra: String,
    /// Feed the sentence on stdin instead of as an argument. Piper and several
    /// other CLIs read their text that way.
    #[serde(default)]
    pub stdin: bool,
    /// One-line description shown by `/tts`.
    #[serde(default)]
    pub about: String,
    /// PyPI distribution that provides this engine's command, if it has one.
    ///
    /// readio does not bundle a model and will not vendor an installer either.
    /// This is the one fact it needs to offer `/tts install`: the package name.
    /// Everything else — whether to use `uv`, `pipx` or `pip`, and which of them
    /// this machine actually has — is worked out at the moment of installing,
    /// because the answer differs per machine and changes over time.
    #[serde(default)]
    pub pip: String,
    /// Package name for the operating system's package manager, for engines
    /// that are not Python at all.
    ///
    /// espeak-ng is a C program: there is no wheel to install and no model to
    /// download, only `brew install espeak-ng` or the same line with `apt`.
    /// Kept separate from `pip` because the two are not alternatives — an
    /// engine has one or the other, and which is available depends on the
    /// machine rather than on the engine.
    #[serde(default)]
    pub system: String,
    /// Python the package insists on, when it is fussy. `kokoro-tts` declares
    /// `>=3.11,<3.13`, so installing it under a default 3.13 fails with a
    /// resolver error that says nothing about the version.
    #[serde(default)]
    pub python: String,
    /// Files the engine needs but does not ship: voice models, mostly. Each is
    /// `url → destination`, fetched after the package is installed.
    #[serde(default)]
    pub fetch: Vec<Fetch>,
    /// Where to read about the engine when readio cannot install it.
    #[serde(default)]
    pub docs: String,
    /// What to change when the passage is in another language, keyed the way
    /// the rest of readio names languages: `zh`, `en`.
    ///
    /// A library is not monolingual, and a multilingual model still has to be
    /// told which language it is looking at. Kokoro's `--lang` defaults to
    /// `en-us`: pointed at Chinese it applies English letter-to-sound rules and
    /// produces a slurred, roughly three-times-too-long reading of text it is
    /// perfectly capable of saying properly. The voice matters just as much —
    /// `zf_*` are Chinese, `af_*` American English — and the two have to move
    /// together, so they live in one entry per language rather than as separate
    /// settings that can be set to disagree.
    #[serde(default)]
    pub languages: BTreeMap<String, LanguageSpec>,
}

/// The parts of an engine's invocation that depend on what is being read.
///
/// Every field is optional: an unset one leaves the engine's own default alone,
/// which is what an engine that only speaks one language wants.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageSpec {
    /// Voice for this language, overriding the engine's default. A reader who
    /// names a voice explicitly outranks this — an explicit choice is a choice.
    #[serde(default)]
    pub voice: String,
    /// Model for this language, for engines like Piper where the language is
    /// baked into the weights rather than selected by a flag.
    #[serde(default)]
    pub model: String,
    /// `{lang}` for this language — the flag *and* its value, because engines
    /// spell it differently (`--lang cmn`, `--language zh`, `-l zh_CN`) and
    /// readio would otherwise have to know each one. Spliced as arguments, so
    /// leaving it empty removes the flag entirely rather than passing a blank.
    #[serde(default)]
    pub lang: String,
}

/// Which language a passage is written in, as far as choosing a voice goes.
///
/// One Han character in eight is enough to call it Chinese: Chinese prose
/// quoting an English term is still Chinese, and an English page quoting a
/// single 字 is still English. Everything else answers `en`, which is readio's
/// default throughout rather than a claim about the alphabet — a Russian book
/// gets English phonemes because that is the entry that exists, and the reader
/// who has a Russian voice says so in `config.yaml`.
pub fn language_of(text: &str) -> &'static str {
    let mut han = 0usize;
    let mut total = 0usize;
    for ch in text.chars().filter(|c| !c.is_whitespace()) {
        total += 1;
        if matches!(ch as u32,
            0x3400..=0x4DBF        // CJK extension A
            | 0x4E00..=0x9FFF      // CJK unified ideographs
            | 0xF900..=0xFAFF      // CJK compatibility ideographs
            | 0x2_0000..=0x2_FFFF  // extensions B onwards
        ) {
            han += 1;
        }
    }
    if han > 0 && han * 8 >= total {
        "zh"
    } else {
        "en"
    }
}

/// One file to download before an engine can speak.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fetch {
    pub url: String,
    /// Where it lands, `~` allowed.
    pub to: String,
}

/// Built-in engine presets.
///
/// Chosen from the local-TTS benchmark at <https://github.com/5uck1ess/tts-bench>
/// (June 2026 pass): small, permissively licensed, and fast enough to stay ahead
/// of a reader on CPU or Apple Silicon. The per-engine numbers are in the
/// comments below, next to the command line each one needs.
pub fn presets() -> BTreeMap<String, EngineSpec> {
    let mut out = BTreeMap::new();

    // espeak-ng — the one that needs nothing. A 26 MB C program, no model, no
    // Python, no download: `brew install espeak-ng` and read-aloud works on the
    // next keypress. It is a formant synthesiser from the 1990s lineage and it
    // sounds like one, which is the trade being offered rather than a defect.
    //
    // Measured here: 30 ms to say a sentence that takes ten seconds to speak,
    // in either language — about 300× realtime. That matters beyond
    // impatience. An engine slower than speech can never be caught up with, so
    // the reading stalls at every sentence boundary no matter how much is
    // rendered ahead; one this fast is never the thing being waited for.
    out.insert(
        "espeak".to_string(),
        EngineSpec {
            synth: "espeak-ng {lang} -s {words} -w {out} {extra}".to_string(),
            serve: String::new(),
            play: default_player(),
            voice: String::new(),
            model: String::new(),
            extra: String::new(),
            stdin: true,
            about: "espeak-ng · GPL-3.0 · instant, tiny, robotic — nothing to download".to_string(),
            pip: String::new(),
            system: "espeak-ng".to_string(),
            python: String::new(),
            fetch: Vec::new(),
            docs: "https://github.com/espeak-ng/espeak-ng".to_string(),
            // espeak-ng's own language codes: `cmn` is Mandarin, and without it
            // Chinese comes out as a list of letter names.
            languages: BTreeMap::from([
                (
                    "zh".to_string(),
                    LanguageSpec {
                        voice: String::new(),
                        model: String::new(),
                        lang: "-v cmn".to_string(),
                    },
                ),
                (
                    "en".to_string(),
                    LanguageSpec {
                        voice: String::new(),
                        model: String::new(),
                        lang: "-v en-us".to_string(),
                    },
                ),
            ]),
        },
    );

    // Kokoro-82M — Apache-2.0, multilingual (including Chinese). The default:
    // best quality per megabyte for long-form reading.
    //
    // Speed depends entirely on which Kokoro you drive, and the CLI is the slow
    // way to drive it. Measured on an M-series CPU: 8.4 s to say a sentence
    // worth 4.2 s of audio — half of real time, so it can never keep up. Almost
    // none of that is the model. The inference is 1.5 s; the rest is a fresh
    // Python for every sentence, importing librosa and scipy inside the
    // synthesis call, plus two seconds of progress spinner. Hence `resident`
    // below, which is the same model with the startup paid once.
    //
    // The CLI takes its input and output as positional arguments, `-` meaning
    // stdin, which is also how a sentence gets in without ever touching a shell.
    //
    // `--model` and `--voices` are spelled out because their defaults are
    // `./kokoro-v1.0.onnx` and `./voices-v1.0.bin` — relative to the working
    // directory, which for readio is wherever the reader happened to launch it.
    // Left implicit, the engine works in whichever folder the models were
    // downloaded to and nowhere else.
    //
    // `{lang}` is the difference between Kokoro reading Chinese and Kokoro
    // spelling it out: see `languages` below.
    out.insert(
        "kokoro".to_string(),
        EngineSpec {
            synth: "kokoro-tts - {out} --voice {voice} --speed {rate} --format wav {lang} \
                    --model ~/.readio/voices/kokoro-v1.0.onnx \
                    --voices ~/.readio/voices/voices-v1.0.bin {extra}"
                .to_string(),
            serve: "{python} {worker} \
                    --model ~/.readio/voices/kokoro-v1.0.onnx \
                    --voices ~/.readio/voices/voices-v1.0.bin"
                .to_string(),
            play: default_player(),
            voice: "zf_xiaoxiao".to_string(),
            model: String::new(),
            extra: String::new(),
            stdin: true,
            about: "Kokoro-82M · Apache-2.0 · best voice here, and it stays loaded".to_string(),
            pip: "kokoro-tts".to_string(),
            system: String::new(),
            // The package declares >=3.11,<3.13, and a machine whose default is
            // 3.13 otherwise fails with a resolver error that never mentions it.
            python: "3.12".to_string(),
            // The wheel carries no weights: without these two the engine
            // installs cleanly and then refuses every sentence.
            fetch: vec![
                Fetch {
                    url: "https://github.com/nazdridoy/kokoro-tts/releases/download\
                          /v1.0.0/kokoro-v1.0.onnx"
                        .to_string(),
                    to: "~/.readio/voices/kokoro-v1.0.onnx".to_string(),
                },
                Fetch {
                    url: "https://github.com/nazdridoy/kokoro-tts/releases/download\
                          /v1.0.0/voices-v1.0.bin"
                        .to_string(),
                    to: "~/.readio/voices/voices-v1.0.bin".to_string(),
                },
            ],
            docs: "https://github.com/nazdridoy/kokoro-tts".to_string(),
            // The four `zf_*` voices are Kokoro's Mandarin set; `af_heart` is
            // its best-rated English one. Pairing each with the matching
            // `--lang` is what this table is for: a Chinese voice under
            // `en-us` is the worst of both, an English phonemizer driving a
            // model that knows how the sentence should sound.
            languages: BTreeMap::from([
                (
                    "zh".to_string(),
                    LanguageSpec {
                        voice: "zf_xiaoxiao".to_string(),
                        model: String::new(),
                        lang: "--lang cmn".to_string(),
                    },
                ),
                (
                    "en".to_string(),
                    LanguageSpec {
                        voice: "af_heart".to_string(),
                        model: String::new(),
                        lang: "--lang en-us".to_string(),
                    },
                ),
            ]),
        },
    );

    // Piper — ~15M and built for exactly this: speech that starts before you
    // notice you asked for it. The benchmark's 62 ms to first audio is a GPU
    // number, but Piper is the preset least dependent on having one.
    //
    // `--length-scale` is duration, not speed, so it takes `{scale}` — the
    // reciprocal of the multiplier. Feeding it `{rate}` makes 2× read at half
    // speed, which is the sort of bug that sounds like a broken model.
    out.insert(
        "piper".to_string(),
        EngineSpec {
            synth: "piper --model {model} --output-file {out} --length-scale {scale} {extra}"
                .to_string(),
            serve: String::new(),
            play: default_player(),
            voice: String::new(),
            model: "~/.readio/voices/zh_CN-huayan-medium.onnx".to_string(),
            extra: String::new(),
            stdin: true,
            about: "Piper · GPL-3.0 · fastest to first sound, models are a few MB".to_string(),
            pip: "piper-tts".to_string(),
            system: String::new(),
            python: String::new(),
            // Piper ships no voice. Fetching the model directly beats
            // `python3 -m piper.download_voices`, which only exists inside
            // whichever environment pip happened to install into.
            fetch: vec![
                Fetch {
                    url: "https://huggingface.co/rhasspy/piper-voices/resolve/main\
                          /zh/zh_CN/huayan/medium/zh_CN-huayan-medium.onnx"
                        .to_string(),
                    to: "~/.readio/voices/zh_CN-huayan-medium.onnx".to_string(),
                },
                Fetch {
                    url: "https://huggingface.co/rhasspy/piper-voices/resolve/main\
                          /zh/zh_CN/huayan/medium/zh_CN-huayan-medium.onnx.json"
                        .to_string(),
                    to: "~/.readio/voices/zh_CN-huayan-medium.onnx.json".to_string(),
                },
            ],
            docs: "https://github.com/OHF-voice/piper1-gpl".to_string(),
            // Piper's language is the model file, and readio fetches one of
            // them. Reading English with `zh_CN-huayan` works about as well as
            // you would expect; the fix is another `.onnx` and a `model:` under
            // `latin` here, not a flag readio can pass.
            languages: BTreeMap::new(),
        },
    );

    // Supertonic — 99M, pure ONNX with no torch anywhere, MIT, 31 languages.
    // `tts` is a subcommand, and the voice is one of M1–M5 / F1–F5 rather than a
    // name, so it needs a default: an empty `--voice` is an error, not a shrug.
    out.insert(
        "supertonic".to_string(),
        EngineSpec {
            synth: "supertonic tts {text} --output {out} --voice {voice} --speed {rate} {extra}"
                .to_string(),
            serve: String::new(),
            play: default_player(),
            voice: "F1".to_string(),
            model: String::new(),
            extra: String::new(),
            stdin: false,
            about: "Supertonic 99M · MIT · pure ONNX, no torch".to_string(),
            pip: "supertonic".to_string(),
            system: String::new(),
            python: String::new(),
            // The model (~400 MB) downloads itself into ~/.cache on first use.
            fetch: Vec::new(),
            docs: "https://github.com/supertone-inc/supertonic-py".to_string(),
            // Its voices are numbered rather than named per language, and the
            // CLI infers the language from the text.
            languages: BTreeMap::new(),
        },
    );

    // Any OpenAI-compatible /v1/audio/speech server, which is how most local
    // model servers expose themselves (Kokoro-FastAPI and friends).
    out.insert(
        "openai".to_string(),
        EngineSpec {
            synth: "curl -sS --fail -X POST http://127.0.0.1:8880/v1/audio/speech \
                    -H 'Content-Type: application/json' \
                    -d {json} -o {out}"
                .to_string(),
            serve: String::new(),
            play: default_player(),
            voice: "zf_xiaobei".to_string(),
            model: "kokoro".to_string(),
            extra: String::new(),
            stdin: false,
            about: "Any OpenAI-compatible /v1/audio/speech endpoint".to_string(),
            // Nothing to install: this one is a server the reader is already
            // running, and readio has no business starting it.
            pip: String::new(),
            system: String::new(),
            python: String::new(),
            fetch: Vec::new(),
            docs: "https://github.com/remsky/Kokoro-FastAPI".to_string(),
            // The request body carries only model, voice, text and speed, so
            // the language is the server's business.
            languages: BTreeMap::new(),
        },
    );

    out
}

/// Command lines readio itself shipped as a preset and has since corrected.
///
/// None of these ever worked: they were written from what the engines' READMEs
/// implied rather than from their actual argument parsers. A saved config that
/// still carries one is not carrying a decision — it is carrying a mistake of
/// readio's, saved to disk the first time an old build started up.
const SUPERSEDED: &[(&str, &str)] = &[
    // `--text` and `--output` are not kokoro-tts flags; it reads `-` on stdin
    // and takes the output file positionally.
    (
        "kokoro",
        "kokoro-tts --text {text} --output {out} --voice {voice} --speed {rate}",
    ),
    // Right shape, but the weights were left implicit — and their default is
    // the working directory, so the engine only worked when readio happened to
    // be started from the folder they were downloaded into.
    (
        "kokoro",
        "kokoro-tts - {out} --voice {voice} --speed {rate} --format wav {extra}",
    ),
    // Ran anywhere, and read Chinese with `--lang`'s default of `en-us`: an
    // English phonemizer sounding out 字 one at a time, three times slower than
    // the same sentence said properly.
    (
        "kokoro",
        "kokoro-tts - {out} --voice {voice} --speed {rate} --format wav \
         --model ~/.readio/voices/kokoro-v1.0.onnx \
         --voices ~/.readio/voices/voices-v1.0.bin {extra}",
    ),
    // piper spells these with hyphens, and `--length-scale` is duration rather
    // than rate, so the number has to be inverted as well as renamed.
    (
        "piper",
        "piper --model {model} --output_file {out} --length_scale {rate}",
    ),
    // supertonic's CLI has a `tts` subcommand and takes the text positionally.
    (
        "supertonic",
        "supertonic --text {text} --out {out} --voice {voice}",
    ),
];

/// Bring the engines in a saved config up to date with this binary.
///
/// A saved config is a record of the reader's choices, not a snapshot of what
/// readio knew on the day it was written — and the fields divide cleanly into
/// three kinds:
///
/// - **What the package is.** `about`, `docs`, `pip` and `python` describe an
///   engine readio ships a preset for. Nobody writes these by hand, so they
///   always follow the binary. Freezing them is what made every engine on an
///   upgraded machine read "a server, nothing to install": the fields did not
///   exist when the file was written, so they loaded empty and stayed empty.
/// - **How to run it.** `synth`, `play`, `stdin`, `fetch` and `languages` are the
///   reader's to change, and are replaced only when what is saved is one of
///   readio's own superseded defaults — where "leaving it alone" means keeping
///   a command that cannot run.
/// - **Which voice.** `voice`, `model` and `extra` are choices, and are never
///   touched, not even while correcting a command line around them.
///
/// Returns true when anything changed, so the caller can write the file back.
pub fn reconcile(engines: &mut BTreeMap<String, EngineSpec>) -> bool {
    let mut changed = false;
    for (name, preset) in presets() {
        let Some(saved) = engines.get_mut(&name) else {
            engines.insert(name, preset);
            changed = true;
            continue;
        };
        let before = saved.clone();

        saved.about = preset.about;
        saved.docs = preset.docs;
        saved.pip = preset.pip;
        saved.system = preset.system;
        saved.python = preset.python;

        let superseded = SUPERSEDED
            .iter()
            .any(|(engine, synth)| *engine == name && same_command(synth, &saved.synth));
        if superseded {
            saved.synth = preset.synth;
            saved.serve = preset.serve;
            saved.play = preset.play;
            saved.stdin = preset.stdin;
            saved.fetch = preset.fetch;
            saved.languages = preset.languages;
        } else if same_command(&saved.synth, &preset.synth) {
            // Their line is readio's line, so the files it needs are readio's
            // to know about too. A line they wrote themselves gets no `fetch`:
            // it is what readiness is checked against, and insisting on a voice
            // file readio chose would report a working setup as missing.
            saved.fetch = preset.fetch;
            // An empty table is a config written before readio had the field,
            // not a reader who decided one language was enough.
            if saved.languages.is_empty() {
                saved.languages = preset.languages;
            }
            // Likewise an absent `serve`: every config written before readio
            // could keep an engine resident has none, and leaving those alone
            // would mean the reader who has been running readio longest is the
            // one still paying eight seconds a sentence.
            if saved.serve.trim().is_empty() {
                saved.serve = preset.serve;
            }
        }

        changed |= *saved != before;
    }
    changed
}

/// Whether two command templates say the same thing.
///
/// Compared word by word because a long line makes the round trip through YAML
/// as a folded scalar: the content survives, the exact run of spaces need not,
/// and a config that reflowed on save is not a config the reader edited.
fn same_command(a: &str, b: &str) -> bool {
    a.split_whitespace().eq(b.split_whitespace())
}

/// Player command for this platform, chosen from what is normally present.
fn default_player() -> String {
    if cfg!(target_os = "macos") {
        "afplay {file}".to_string()
    } else if cfg!(target_os = "windows") {
        // The whole language is one quoted argument and the path is quoted inside
        // it, because `C:\Users\John Doe\...` is an ordinary Windows path and an
        // unquoted one would be read as two arguments.
        "powershell -NoProfile -Command \"(New-Object Media.SoundPlayer '{file}').PlaySync()\""
            .to_string()
    } else {
        // ALSA is the most common; ffplay is the usual fallback and is quiet
        // enough with these flags.
        "aplay -q {file}".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_wired_up() {
        for (name, spec) in presets() {
            assert!(
                spec.synth.contains("{out}"),
                "{name} must write to {{out}} so its duration can be measured"
            );
            assert!(
                spec.synth.contains("{text}") || spec.synth.contains("{json}") || spec.stdin,
                "{name} has no way to receive the text: neither a placeholder nor stdin"
            );
            assert!(!spec.about.is_empty(), "{name} has no description");
            assert!(
                spec.play.contains("{file}"),
                "{name} needs a player for its clip"
            );
        }
    }

    #[test]
    fn the_presets_cover_the_shapes_engines_come_in() {
        let presets = presets();
        assert!(
            presets.len() >= 4,
            "a text argument, stdin, and an HTTP server should all be represented"
        );
        assert!(
            presets["piper"].stdin,
            "piper reads its text on stdin, not as an argument"
        );
        assert!(
            presets["openai"].synth.contains("{json}"),
            "the OpenAI-compatible preset posts a JSON body"
        );
        assert!(
            !presets["kokoro"].voice.is_empty(),
            "the default engine should bring a working voice"
        );
    }

    #[test]
    fn the_player_is_platform_appropriate() {
        let player = default_player();
        assert!(
            player.contains("{file}"),
            "the player needs the clip: {player}"
        );
        if cfg!(target_os = "macos") {
            assert!(player.starts_with("afplay"), "got: {player}");
        }
    }

    /// The rule the table keys encode: one Han character in eight. A Chinese
    /// sentence quoting an English term is still Chinese, and an English one
    /// quoting a 字 is still English — read the other way round, either sounds
    /// like a broken model.
    #[test]
    fn a_passage_is_named_by_what_it_is_mostly_written_in() {
        assert_eq!(language_of("界面不是中立的。"), "zh");
        assert_eq!(language_of("这是 API 的设计问题"), "zh");
        assert_eq!(language_of("The interface is not neutral."), "en");
        assert_eq!(
            language_of("The character 道 appears twice in the opening chapter."),
            "en"
        );
        assert_eq!(language_of(""), "en", "and nothing at all is not an error");
    }

    /// Every language a preset offers has to actually change something, and an
    /// engine that has voices has to change those too: a Chinese voice under
    /// English phonemes is the failure this table exists to prevent. Engines
    /// whose voice *is* the language — espeak picks both with `-v cmn` — have
    /// nothing to disagree with themselves about.
    #[test]
    fn every_language_entry_names_a_voice_and_its_phonemes() {
        for (name, spec) in presets() {
            let has_voices = spec
                .languages
                .values()
                .any(|entry| !entry.voice.is_empty() || !entry.model.is_empty());
            for (language, entry) in &spec.languages {
                assert!(
                    !entry.voice.is_empty() || !entry.model.is_empty() || !entry.lang.is_empty(),
                    "{name}/{language} is an entry that changes nothing"
                );
                assert!(
                    !has_voices || !entry.voice.is_empty() || !entry.model.is_empty(),
                    "{name}/{language} switches language without switching voice"
                );
                assert!(
                    entry.lang.is_empty() || spec.synth.contains("{lang}"),
                    "{name}/{language} has a language flag the command never passes"
                );
            }
        }
    }
}
