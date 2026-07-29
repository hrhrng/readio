//! Driving a speech engine as a subprocess.
//!
//! Templates rather than an SDK per engine: a preset is a command line with
//! placeholders, so a model readio has never heard of works as long as it can
//! write a wav file. Arguments are split with shell-like quoting but never
//! handed to a shell, so a sentence full of punctuation cannot turn into a
//! command.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};

use super::config::{EngineSpec, LanguageSpec};
use super::resident;
use super::{Clip, Synthesizer, wav};
use crate::i18n::{t, tf};

/// A synthesizer built from an [`EngineSpec`].
pub struct CommandSynth {
    name: String,
    spec: EngineSpec,
    voice: String,
    language: String,
    /// The reading speed, as f32 bits, because it changes under the render
    /// thread's feet: `^r` retunes a running engine rather than replacing it.
    rate: AtomicU32,
    /// Set when the engine can stay running, which is how it stops being
    /// slower than the speech it produces.
    resident: Option<resident::Resident>,
}

impl CommandSynth {
    /// `voice` is the reader's explicit choice; empty means the model preset's
    /// declared default. Sentence content never changes the selection.
    pub fn new(name: &str, spec: EngineSpec, voice: String, rate: f32) -> Self {
        let resident = resident_for(name, &spec);
        Self {
            name: name.to_string(),
            spec,
            voice,
            language: String::new(),
            rate: AtomicU32::new(rate.to_bits()),
            resident,
        }
    }

    /// The speed sentences are being rendered at.
    fn speed(&self) -> f32 {
        f32::from_bits(self.rate.load(Ordering::Relaxed))
    }

    /// Pin the language, or keep the model preset's declared default.
    ///
    /// `auto` survives as an older config spelling, but no longer turns on
    /// per-sentence routing. Anything else names an entry in the engine's
    /// `languages` table.
    pub fn in_language(mut self, language: &str) -> Self {
        self.language = language.trim().to_string();
        self
    }

    /// Check the engine's program exists before switching read-aloud on, so the
    /// failure arrives as a sentence rather than as silence.
    pub fn program(&self) -> Option<String> {
        split_args(&self.spec.synth).into_iter().next()
    }

    pub fn is_available(&self) -> bool {
        match self.program() {
            Some(program) => which(&program),
            None => false,
        }
    }
}

impl Synthesizer for CommandSynth {
    /// Load the model before the reader needs it.
    ///
    /// Only the resident path has anything to warm; for a per-sentence CLI
    /// there is nothing that survives to be warm.
    fn warm(&self) -> Result<()> {
        match self.resident.as_ref() {
            Some(resident) => resident.warm(),
            None => Ok(()),
        }
    }

    fn synthesize(&self, text: &str, out: &Path) -> Result<Clip> {
        if let Some(engine) = self.resident.as_ref() {
            let language = self.language_for(text);
            let voice = self.voice_for(language);
            engine
                .render(resident::Request {
                    text,
                    out,
                    voice: &voice,
                    lang: language_code(language),
                    speed: self.speed(),
                    params: &self.spec.extra,
                })
                .with_context(|| tf("synth.failed", &[&self.name]))?;
        } else {
            let args = self.build(&self.spec.synth, text, out, None);
            let stdin = self.spec.stdin.then_some(text);
            run(&args, stdin, Duration::from_secs(120))
                .with_context(|| tf("synth.failed", &[&self.name]))?;
        }

        if !out.exists() {
            return Err(anyhow!("{}", tf("synth.no_audio", &[&self.name])));
        }
        let ms = wav::duration_ms(out)
            .with_context(|| tf("synth.bad_wav", &[&self.name, &out.display()]))?;
        Ok(Clip {
            path: out.to_path_buf(),
            ms,
        })
    }

    fn play(&self, clip: &Clip, cancel: &AtomicBool) -> Result<()> {
        if self.spec.play.trim().is_empty() {
            // The synth command played it itself; wait out its duration so the
            // caller's pacing still lines up.
            return sleep_cancellable(clip.ms, cancel);
        }
        let args = self.build(&self.spec.play, "", &clip.path, Some(&clip.path));
        let Some((program, rest)) = args.split_first() else {
            return Err(anyhow!("{}", t("synth.empty_play")));
        };

        let mut child = Command::new(program)
            .args(rest)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| tf("synth.cannot_play", &[&program]))?;

        // Poll so an interrupt can cut the audio off mid-sentence.
        loop {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(());
            }
            match child.try_wait()? {
                Some(status) if status.success() => return Ok(()),
                Some(status) => {
                    return Err(anyhow!("{}", tf("synth.player_exited", &[&status])));
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }

    fn describe(&self) -> String {
        let voice = if self.voice.trim().is_empty() {
            String::new()
        } else {
            format!(" · {}", self.voice)
        };
        format!("{}{voice}", self.name)
    }
}

impl CommandSynth {
    /// The language entry to use for this configuration.
    ///
    /// In order: the one the reader pinned in `config.yaml`; the one the voice
    /// they pinned belongs to, since `zf_xiaoyi` under `en-us` is the exact
    /// mismatch this table exists to prevent; otherwise the entry belonging to
    /// the model's declared default voice. The sentence itself never changes
    /// this decision.
    fn language_for(&self, _text: &str) -> Option<&LanguageSpec> {
        let named = match self.language.as_str() {
            "" | "auto" => None,
            name => self.spec.languages.get(name),
        };
        named.or_else(|| self.voice_family()).or_else(|| {
            self.spec
                .languages
                .values()
                .find(|language| !self.spec.voice.is_empty() && language.voice == self.spec.voice)
        })
    }

    /// The language entry a pinned voice belongs to, if readio ships one.
    ///
    /// Matched on the family a voice name starts with as well as on the name
    /// itself: Kokoro's Mandarin set is `zf_xiaobei`, `zf_xiaoni`, `zf_xiaoyi`
    /// and the `zf_xiaoxiao` in the table, and a reader swapping between them
    /// has not changed language.
    fn voice_family(&self) -> Option<&LanguageSpec> {
        let voice = self.voice.trim();
        if voice.is_empty() {
            return None;
        }
        let family = |name: &str| name.split_once('_').map(|(head, _)| head.to_string());
        self.spec.languages.values().find(|s| {
            s.voice == voice || (family(&s.voice) == family(voice) && !s.voice.is_empty())
        })
    }

    /// The configured voice: the reader's explicit choice first, then the voice
    /// declared by the configured language, then the model preset's default.
    fn voice_for(&self, language: Option<&LanguageSpec>) -> String {
        if !self.voice.trim().is_empty() {
            return self.voice.clone();
        }
        match language.map(|s| s.voice.as_str()) {
            Some(voice) if !voice.is_empty() => voice.to_string(),
            _ => self.spec.voice.clone(),
        }
    }

    /// Expand a template into argv.
    ///
    /// `{text}` and `{json}` are substituted after splitting, so no amount of
    /// quoting inside the sentence can add an argument.
    fn build(&self, template: &str, text: &str, out: &Path, file: Option<&Path>) -> Vec<String> {
        // Anything the reader wrote in `extra` still has the last word: it is
        // spliced after `{lang}`, and a repeated flag is the later one.
        let language = self.language_for(text);
        let voice = self.voice_for(language);
        let model = match language.map(|s| s.model.as_str()) {
            Some(model) if !model.is_empty() => model,
            _ => &self.spec.model,
        };
        let lang = language.map(|s| s.lang.as_str()).unwrap_or_default();
        let json = request_body(model, &voice, text, self.speed());
        split_args(template)
            .into_iter()
            .flat_map(|arg| {
                // `{extra}` is the reader's own words from the config file, so it
                // is spliced as arguments rather than substituted into one. A
                // `--lang cmn` that arrives as a single argv entry is an engine
                // saying "unrecognised option --lang cmn".
                if arg.trim() == "{extra}" {
                    return split_args(&self.spec.extra);
                }
                // Same for `{lang}`, and for the same reason — with the added
                // point that an engine with nothing to say here contributes no
                // argument at all rather than an empty one.
                if arg.trim() == "{lang}" {
                    return split_args(lang);
                }
                // Nothing here goes through a shell, so `~` would otherwise
                // reach the engine as a directory named "~". Expanded on the
                // template rather than on the result, so a sentence that opens
                // with a tilde is still just a sentence.
                let arg = expand_tilde(&arg);
                let expanded = arg
                    .replace("{out}", &out.to_string_lossy())
                    .replace(
                        "{file}",
                        &file
                            .map(|f| f.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    )
                    .replace("{voice}", &voice)
                    .replace("{rate}", &format_rate(self.speed()))
                    .replace("{scale}", &format_rate(inverse(self.speed())))
                    .replace("{words}", &words_per_minute(self.speed()))
                    .replace("{model}", &expand_tilde(model))
                    .replace("{extra}", &self.spec.extra);
                // Whole-argument substitutions last: their content is data.
                let arg = if expanded.trim() == "{text}" {
                    text.to_string()
                } else if expanded.trim() == "{json}" {
                    json.clone()
                } else {
                    expanded.replace("{text}", text).replace("{json}", &json)
                };
                vec![arg]
            })
            .filter(|arg| !arg.is_empty())
            .collect()
    }
}

/// Rate as engines expect it: two decimals, no exponent.
fn format_rate(rate: f32) -> String {
    format!("{rate:.2}")
}

/// The same speed said the other way round, for engines whose knob is duration.
///
/// Piper's `--length-scale` stretches the audio: 0.5 is twice as fast. Handing
/// it a speed multiplier makes "read faster" read slower, which sounds like a
/// broken model rather than a swapped placeholder.
fn inverse(rate: f32) -> f32 {
    if rate.abs() < f32::EPSILON {
        1.0
    } else {
        1.0 / rate
    }
}

/// Speed as words per minute, for engines whose knob is a reading rate.
///
/// espeak-ng's `-s` is words per minute and its own default is 175. Expressing
/// the multiplier against that default keeps one meaning of "1.5×" across every
/// engine: half again as fast as this engine normally reads.
fn words_per_minute(rate: f32) -> String {
    let wpm = (175.0 * rate).round().clamp(80.0, 450.0);
    format!("{wpm:.0}")
}

/// Build the resident engine for a spec that has one.
///
/// Returns `None` when the spec names no `serve` line, or when the interpreter
/// behind the engine cannot be found — an engine that is not installed has
/// nothing to keep resident, and saying so through the ordinary
/// "engine missing" path is clearer than a worker that fails to import.
fn resident_for(name: &str, spec: &EngineSpec) -> Option<resident::Resident> {
    if spec.serve.trim().is_empty() {
        return None;
    }
    let program = split_args(&spec.synth).into_iter().next()?;
    let python = resident::interpreter_behind(&program)?;
    // The script readio ships for this engine. A `serve:` line that names no
    // `{worker}` — someone driving their own daemon — needs none, so a missing
    // one is only fatal if the template actually asks for it.
    let worker = resident::worker_script(&crate::paths::engines_dir(), name).ok()?;
    if worker.is_none() && spec.serve.contains("{worker}") {
        return None;
    }
    let worker = worker.unwrap_or_default();

    let argv: Vec<String> = split_args(&spec.serve)
        .into_iter()
        .map(|arg| {
            arg.replace("{python}", &python.to_string_lossy())
                .replace("{worker}", &worker.to_string_lossy())
                .replace("{model}", &expand_tilde(&spec.model))
        })
        .map(|arg| expand_tilde(&arg))
        .collect();
    (!argv.is_empty()).then(|| resident::Resident::new(argv))
}

/// The language on its own, without the flag it is usually spelled with.
///
/// The table stores `--lang cmn` because engines disagree about how to write
/// the flag and readio would otherwise have to know each spelling. A worker
/// talking JSON wants only the value, which is the last word either way —
/// `--lang cmn`, `-v cmn`, `--language zh` all end in the part that matters.
fn language_code(language: Option<&LanguageSpec>) -> &str {
    language
        .map(|spec| spec.lang.as_str())
        .and_then(|lang| lang.split_whitespace().next_back())
        .unwrap_or_default()
}

/// Body for an OpenAI-compatible `/v1/audio/speech` call.
fn request_body(model: &str, voice: &str, text: &str, rate: f32) -> String {
    let model = if model.trim().is_empty() {
        "tts-1"
    } else {
        model
    };
    let voice = if voice.trim().is_empty() {
        "alloy"
    } else {
        voice
    };
    serde_json::json!({
        "model": model,
        "voice": voice,
        "input": text,
        "speed": rate,
        "response_format": "wav",
    })
    .to_string()
}

/// Split a command template into arguments, honouring quotes.
pub fn split_args(template: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;

    for ch in template.chars() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"') | (None, '\'') => {
                quote = Some(ch);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, c) => current.push(c),
        }
    }
    if started || !current.is_empty() {
        out.push(current);
    }
    out
}

/// Run a command to completion with a ceiling on how long it may take.
///
/// `input` is written to the child's stdin, for engines like Piper that read
/// their text there rather than from an argument.
fn run(args: &[String], input: Option<&str>, timeout: Duration) -> Result<()> {
    let Some((program, rest)) = args.split_first() else {
        return Err(anyhow!("{}", t("synth.empty_command")));
    };
    let mut child = Command::new(program)
        .args(rest)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| tf("synth.cannot_run", &[&program]))?;

    if let Some(text) = input {
        use std::io::Write;
        if let Some(mut pipe) = child.stdin.take() {
            // A closed pipe means the engine already gave up; the exit status
            // below carries the real complaint.
            let _ = pipe.write_all(text.as_bytes());
            let _ = pipe.write_all(b"\n");
        }
    }

    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait()? {
            Some(status) if status.success() => return Ok(()),
            Some(status) => {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    use std::io::Read;
                    let _ = pipe.read_to_string(&mut stderr);
                }
                let detail = stderr.lines().next_back().unwrap_or("").trim().to_string();
                return Err(anyhow!(
                    "{}",
                    if detail.is_empty() {
                        tf("synth.exited", &[&program, &status])
                    } else {
                        tf("synth.exited_saying", &[&program, &status, &detail])
                    }
                ));
            }
            None => {
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(anyhow!("{}", tf("synth.timeout", &[&program])));
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        }
    }
}

/// Sleep in slices so a cancel lands promptly.
fn sleep_cancellable(ms: u64, cancel: &AtomicBool) -> Result<()> {
    let deadline = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < deadline {
        if cancel.load(Ordering::SeqCst) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    Ok(())
}

/// Whether a program is on `PATH` (or is an existing absolute path).
pub fn which(program: &str) -> bool {
    let path = PathBuf::from(expand_tilde(program));
    if path.components().count() > 1 {
        return path.exists();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        let candidate = dir.join(program);
        candidate.is_file() || candidate.with_extension("exe").is_file()
    })
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest).to_string_lossy().into_owned();
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(synth: &str) -> EngineSpec {
        EngineSpec {
            synth: synth.to_string(),
            play: "afplay {file}".to_string(),
            ..EngineSpec::default()
        }
    }

    #[test]
    fn splits_arguments_respecting_quotes() {
        assert_eq!(
            split_args("piper --model a.onnx --out {out}"),
            vec!["piper", "--model", "a.onnx", "--out", "{out}"]
        );
        assert_eq!(
            split_args("curl -H 'Content-Type: application/json' -d {json}"),
            vec![
                "curl",
                "-H",
                "Content-Type: application/json",
                "-d",
                "{json}"
            ]
        );
        assert_eq!(
            split_args("  spaced   out  "),
            vec!["spaced", "out"],
            "extra whitespace should collapse"
        );
    }

    #[test]
    fn a_sentence_can_never_become_an_argument() {
        let synth = CommandSynth::new(
            "test",
            spec("engine --text {text} --out {out}"),
            String::new(),
            1.0,
        );
        let nasty = "念这句；rm -rf / --no-preserve-root \"还有引号\" $(whoami)";
        let args = synth.build(
            &synth.spec.synth.clone(),
            nasty,
            Path::new("/tmp/a.wav"),
            None,
        );

        assert_eq!(args.len(), 5, "argument count must not depend on the text");
        assert_eq!(args[2], nasty, "the sentence stays exactly one argument");
        assert!(
            !args.iter().any(|a| a == "rm" || a == "-rf"),
            "no part of the sentence may become its own argument: {args:?}"
        );
    }

    #[test]
    fn placeholders_are_filled_from_the_spec() {
        let mut engine = spec("piper --model {model} --out {out} --length_scale {rate}");
        engine.model = "/models/zh.onnx".to_string();
        let synth = CommandSynth::new("piper", engine, "zf_xiaobei".to_string(), 1.25);
        let args = synth.build(
            &synth.spec.synth.clone(),
            "文本",
            Path::new("/tmp/out.wav"),
            None,
        );
        assert!(args.contains(&"/models/zh.onnx".to_string()));
        assert!(args.contains(&"/tmp/out.wav".to_string()));
        assert!(
            args.contains(&"1.25".to_string()),
            "rate should be formatted plainly, got {args:?}"
        );
    }

    #[test]
    fn openai_engines_get_a_json_body() {
        let mut engine = spec("curl -d {json} -o {out}");
        engine.model = "kokoro".to_string();
        let synth = CommandSynth::new("openai", engine, "zf_xiaobei".to_string(), 1.0);
        let args = synth.build(
            &synth.spec.synth.clone(),
            "你好",
            Path::new("/tmp/o.wav"),
            None,
        );
        let body = args
            .iter()
            .find(|a| a.starts_with('{'))
            .expect("a json body");
        let parsed: serde_json::Value = serde_json::from_str(body).expect("valid json");
        assert_eq!(parsed["input"], "你好");
        assert_eq!(parsed["voice"], "zf_xiaobei");
        assert_eq!(parsed["model"], "kokoro");
        assert_eq!(parsed["response_format"], "wav");
    }

    #[test]
    fn a_missing_engine_is_detected_before_use() {
        let synth = CommandSynth::new(
            "ghost",
            spec("definitely-not-installed-abc123 --out {out}"),
            String::new(),
            1.0,
        );
        assert_eq!(
            synth.program().as_deref(),
            Some("definitely-not-installed-abc123")
        );
        assert!(!synth.is_available(), "should report the binary as missing");
    }

    /// A failing engine has to explain itself in the reader's language. The
    /// sentence comes back from a subprocess, so this is the one place where an
    /// untranslated string could slip past the message table.
    #[test]
    fn a_failing_engine_reports_why_in_the_readers_language() {
        let _lock = crate::i18n::exclusive();
        let before = crate::i18n::current();
        let synth = CommandSynth::new("false", spec("false --out {out}"), String::new(), 1.0);
        let complain = || {
            format!(
                "{:#}",
                synth
                    .synthesize("text", Path::new("/tmp/readio-never-written.wav"))
                    .expect_err("`false` cannot synthesize anything")
            )
        };

        crate::i18n::set(crate::i18n::Lang::En);
        let english = complain();
        crate::i18n::set(crate::i18n::Lang::Zh);
        let chinese = complain();
        crate::i18n::set(before);

        assert!(
            english.contains("false") && english.contains("could not synthesize"),
            "an English reader should be told in English: {english}"
        );
        assert!(
            chinese.contains("合成失败"),
            "a Chinese reader should be told in Chinese: {chinese}"
        );
    }

    /// An engine that exits happily without writing a wav is a misconfigured
    /// command line, so the message has to name the file the reader can edit.
    /// `config.yaml` is the only file readio keeps; naming any other one sends
    /// them looking for something that was never there.
    #[test]
    fn an_engine_that_writes_no_audio_points_at_the_file_you_can_edit() {
        let _lock = crate::i18n::exclusive();
        let before = crate::i18n::current();
        crate::i18n::set(crate::i18n::Lang::En);

        let out = std::env::temp_dir().join("readio-no-audio-test.wav");
        let _ = std::fs::remove_file(&out);
        let synth = CommandSynth::new("quiet", spec("true --out {out}"), String::new(), 1.0);
        let message = format!(
            "{:#}",
            synth
                .synthesize("text", &out)
                .expect_err("`true` exits happily and writes nothing")
        );
        crate::i18n::set(before);

        assert!(
            message.contains("config.yaml"),
            "the message has to name the file to edit: {message}"
        );
    }

    #[test]
    fn which_finds_real_programs_only() {
        assert!(which("sh") || which("cmd"), "a shell should be on PATH");
        assert!(!which("readio-nonexistent-binary-xyz"));
    }

    /// No shell runs these commands, so `~` in a config file has to be expanded
    /// here or reach the engine as the name of a directory nobody has.
    #[test]
    fn a_tilde_in_a_command_means_home() {
        let synth = CommandSynth::new(
            "kokoro",
            spec("engine --model ~/.readio/voices/m.onnx --out {out}"),
            String::new(),
            1.0,
        );
        let args = synth.build(
            &synth.spec.synth.clone(),
            "文本",
            Path::new("/tmp/a.wav"),
            None,
        );
        let home = dirs::home_dir().expect("a home directory");
        assert_eq!(
            args[2],
            home.join(".readio/voices/m.onnx").to_string_lossy(),
            "the path has to be absolute by the time the engine sees it: {args:?}"
        );
        assert!(
            !args.iter().any(|arg| arg.starts_with('~')),
            "nothing may still be holding a tilde: {args:?}"
        );
    }

    /// An engine whose model path is left implicit works only in the directory
    /// its weights were downloaded into — and readio is started from wherever
    /// the reader happens to be. `kokoro-tts` defaults `--model` and `--voices`
    /// to `./`, which is exactly this trap.
    #[test]
    fn no_preset_depends_on_the_directory_readio_was_started_from() {
        for (name, spec) in crate::voice::config::presets() {
            for fetch in &spec.fetch {
                let file = fetch.to.rsplit('/').next().unwrap_or_default();
                // A sidecar is found by the engine itself: piper reads
                // `voice.onnx.json` next to the `voice.onnx` it was given, so
                // naming the model is naming both.
                let sidecar_of = file.rsplit_once('.').map(|(base, _)| base);
                let named =
                    |needle: &str| spec.synth.contains(needle) || spec.model.contains(needle);
                assert!(
                    named(file) || sidecar_of.is_some_and(named),
                    "{name} downloads {file} and then never tells the engine where it went"
                );
            }
        }
    }

    /// An unset language means the model's declared default, not a detector that
    /// changes configuration sentence by sentence.
    #[test]
    fn an_unpinned_model_keeps_its_declared_voice_and_language() {
        let preset = crate::voice::config::presets()["kokoro"].clone();
        let template = preset.synth.clone();
        let synth = CommandSynth::new("kokoro", preset, String::new(), 1.0);

        let chinese = synth.build(&template, "界面不是中立的。", Path::new("/tmp/a.wav"), None);
        assert!(
            window(&chinese, "--lang") == Some("cmn"),
            "Chinese has to be phonemised as Chinese: {chinese:?}"
        );
        assert!(
            window(&chinese, "--voice").is_some_and(|v| v.starts_with("zf_")),
            "and read by a Chinese voice: {chinese:?}"
        );

        let english = synth.build(
            &template,
            "The interface is not neutral.",
            Path::new("/tmp/a.wav"),
            None,
        );
        assert_eq!(window(&english, "--lang"), Some("cmn"), "{english:?}");
        assert!(
            window(&english, "--voice").is_some_and(|v| v.starts_with("zf_")),
            "the text does not silently change the configured voice: {english:?}"
        );
    }

    /// Naming a voice is a decision. readio corrects its own defaults, never a
    /// reader's — and a voice it recognises brings its language with it, so a
    /// Chinese voice is never handed English phonemes.
    #[test]
    fn a_voice_the_reader_named_outranks_the_language_it_belongs_to() {
        let preset = crate::voice::config::presets()["kokoro"].clone();
        let template = preset.synth.clone();
        let synth = CommandSynth::new("kokoro", preset, "zf_xiaoyi".to_string(), 1.0);
        let args = synth.build(
            &template,
            "The interface is not neutral.",
            Path::new("/tmp/a.wav"),
            None,
        );
        assert_eq!(
            window(&args, "--voice"),
            Some("zf_xiaoyi"),
            "the voice they asked for is the voice they get: {args:?}"
        );
        assert_eq!(
            window(&args, "--lang"),
            Some("cmn"),
            "and it is spoken as the language it belongs to: {args:?}"
        );
    }

    /// `{extra}` is spliced after `{lang}`, so a reader who wants a language
    /// readio has no entry for — Japanese, say — writes it there and wins.
    #[test]
    fn a_language_written_by_hand_has_the_last_word() {
        let mut preset = crate::voice::config::presets()["kokoro"].clone();
        preset.extra = "--lang ja".to_string();
        let template = preset.synth.clone();
        let synth = CommandSynth::new("kokoro", preset, String::new(), 1.0);
        let args = synth.build(&template, "界面不是中立的。", Path::new("/tmp/a.wav"), None);
        let last = args
            .iter()
            .rposition(|arg| arg == "--lang")
            .expect("a language flag");
        assert_eq!(
            args.get(last + 1).map(String::as_str),
            Some("ja"),
            "the reader's own flag has to come last: {args:?}"
        );
    }

    /// An engine with nothing to say about language contributes no argument at
    /// all: a bare `--lang` would swallow the flag after it.
    #[test]
    fn an_engine_without_a_language_flag_passes_none() {
        let synth = CommandSynth::new(
            "plain",
            spec("engine --out {out} {lang} --voice {voice}"),
            "F1".to_string(),
            1.0,
        );
        let args = synth.build(
            &synth.spec.synth.clone(),
            "文本",
            Path::new("/tmp/a.wav"),
            None,
        );
        assert_eq!(
            args,
            vec!["engine", "--out", "/tmp/a.wav", "--voice", "F1"],
            "an empty {{lang}} has to vanish rather than leave a hole: {args:?}"
        );
    }

    /// A configured language reads every page the same way.
    #[test]
    fn a_pinned_language_reads_every_page_the_same_way() {
        let preset = crate::voice::config::presets()["kokoro"].clone();
        let template = preset.synth.clone();
        let synth = CommandSynth::new("kokoro", preset, String::new(), 1.0).in_language("en");
        let args = synth.build(&template, "界面不是中立的。", Path::new("/tmp/a.wav"), None);
        assert_eq!(window(&args, "--lang"), Some("en-us"), "{args:?}");
        assert_eq!(window(&args, "--voice"), Some("af_heart"), "{args:?}");
    }

    /// A typo falls back to the model default rather than turning detection on.
    #[test]
    fn a_language_nobody_has_an_entry_for_falls_back_to_the_model_default() {
        let preset = crate::voice::config::presets()["kokoro"].clone();
        let template = preset.synth.clone();
        let synth = CommandSynth::new("kokoro", preset, String::new(), 1.0).in_language("zn");
        let args = synth.build(&template, "界面不是中立的。", Path::new("/tmp/a.wav"), None);
        assert_eq!(window(&args, "--lang"), Some("cmn"), "{args:?}");
    }

    /// A resident engine is started once, so its model is a startup argument
    /// rather than part of each sentence. Leaving `{model}` literal here makes
    /// config.yaml lie: the fallback uses the edit, while read-aloud does not.
    #[test]
    fn a_resident_engine_starts_with_the_model_from_its_config() {
        let dir = std::env::temp_dir().join(format!(
            "readio-resident-model-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");

        let python = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join("python3"))
            .find(|path| path.is_file())
            .expect("python3 is needed to exercise the worker protocol");
        let launcher = dir.join("model-tts");
        std::fs::write(&launcher, format!("#!{}\n", python.to_string_lossy())).expect("launcher");
        let worker = dir.join("worker.py");
        std::fs::write(
            &worker,
            r#"import argparse
import json
import sys

ap = argparse.ArgumentParser()
ap.add_argument("--model", required=True)
args = ap.parse_args()
if args.model != "local/qwen-for-my-mac":
    print(json.dumps({"ready": False, "error": f"wrong model: {args.model}"}), flush=True)
    raise SystemExit(1)
print(json.dumps({"ready": True}), flush=True)
for line in sys.stdin:
    print(json.dumps({"ok": True}), flush=True)
"#,
        )
        .expect("worker");

        let spec = EngineSpec {
            synth: format!("{} {{text}} {{out}}", launcher.to_string_lossy()),
            serve: format!("{{python}} {} --model {{model}}", worker.to_string_lossy()),
            model: "local/qwen-for-my-mac".to_string(),
            ..EngineSpec::default()
        };

        let resident = resident_for("custom", &spec).expect("resident command");
        resident
            .warm()
            .expect("the configured model should reach the worker");
    }

    /// The value after a flag, for tests that care what an engine was told.
    fn window<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        let at = args.iter().position(|arg| arg == flag)?;
        args.get(at + 1).map(String::as_str)
    }

    /// Windows has no `afplay`, so the default player is a PowerShell one-liner.
    /// `C:\Users\John Doe\...` is a perfectly ordinary path there, so the language
    /// has to reach PowerShell as one argument with the path quoted inside it.
    /// Checked on every platform because the template is what ships, and nobody
    /// runs the Windows tests.
    #[test]
    fn the_windows_player_template_survives_a_path_with_spaces() {
        let template = crate::voice::config::presets()["kokoro"].play.clone();
        let windows =
            "powershell -NoProfile -Command \"(New-Object Media.SoundPlayer '{file}').PlaySync()\"";
        let args = split_args(windows);
        assert_eq!(
            args.len(),
            4,
            "the language must stay one argument: {args:?}"
        );

        let clip = r"C:\Users\John Doe\.readio\speech\1.wav";
        let language = args[3].replace("{file}", clip);
        assert_eq!(
            language,
            format!("(New-Object Media.SoundPlayer '{clip}').PlaySync()"),
            "the path has to end up quoted inside the language"
        );

        // And the platform readio was built for gets a player at all.
        assert!(!template.is_empty(), "every preset needs a play command");
    }
}
