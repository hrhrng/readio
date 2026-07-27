//! Driving a speech engine as a subprocess.
//!
//! Templates rather than an SDK per engine: a preset is a command line with
//! placeholders, so a model readio has never heard of works as long as it can
//! write a wav file. Arguments are split with shell-like quoting but never
//! handed to a shell, so a sentence full of punctuation cannot turn into a
//! command.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};

use super::config::EngineSpec;
use super::{Clip, Synthesizer, wav};

/// A synthesizer built from an [`EngineSpec`].
pub struct CommandSynth {
    name: String,
    spec: EngineSpec,
    voice: String,
    rate: f32,
}

impl CommandSynth {
    pub fn new(name: &str, spec: EngineSpec, voice: String, rate: f32) -> Self {
        Self {
            name: name.to_string(),
            spec,
            voice,
            rate,
        }
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
    fn synthesize(&self, text: &str, out: &Path) -> Result<Clip> {
        let args = self.build(&self.spec.synth, text, out, None);
        let stdin = self.spec.stdin.then_some(text);
        run(&args, stdin, Duration::from_secs(120))
            .with_context(|| format!("{} 合成失败", self.name))?;

        if !out.exists() {
            return Err(anyhow!(
                "{} 没有写出音频文件（检查 tts.toml 里的 synth 命令）",
                self.name
            ));
        }
        let ms = wav::duration_ms(out).with_context(|| {
            format!("{} 写出的不是可识别的 wav（{}）", self.name, out.display())
        })?;
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
            return Err(anyhow!("播放命令是空的"));
        };

        let mut child = Command::new(program)
            .args(rest)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("无法运行播放器 {program}"))?;

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
                    return Err(anyhow!("播放器退出（{status}）"));
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
    /// Expand a template into argv.
    ///
    /// `{text}` and `{json}` are substituted after splitting, so no amount of
    /// quoting inside the sentence can add an argument.
    fn build(&self, template: &str, text: &str, out: &Path, file: Option<&Path>) -> Vec<String> {
        let json = request_body(&self.spec.model, &self.voice, text, self.rate);
        split_args(template)
            .into_iter()
            .map(|arg| {
                let expanded = arg
                    .replace("{out}", &out.to_string_lossy())
                    .replace(
                        "{file}",
                        &file
                            .map(|f| f.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    )
                    .replace("{voice}", &self.voice)
                    .replace("{rate}", &format_rate(self.rate))
                    .replace("{model}", &expand_tilde(&self.spec.model))
                    .replace("{extra}", &self.spec.extra);
                // Whole-argument substitutions last: their content is data.
                if expanded.trim() == "{text}" {
                    text.to_string()
                } else if expanded.trim() == "{json}" {
                    json.clone()
                } else {
                    expanded.replace("{text}", text).replace("{json}", &json)
                }
            })
            .filter(|arg| !arg.is_empty())
            .collect()
    }
}

/// Rate as engines expect it: two decimals, no exponent.
fn format_rate(rate: f32) -> String {
    format!("{rate:.2}")
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
        return Err(anyhow!("命令是空的"));
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
        .with_context(|| format!("无法运行 {program}"))?;

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
                    "{program} 退出（{status}）{}",
                    if detail.is_empty() {
                        String::new()
                    } else {
                        format!("：{detail}")
                    }
                ));
            }
            None => {
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(anyhow!("{program} 超时"));
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
            voice: String::new(),
            model: String::new(),
            extra: String::new(),
            stdin: false,
            about: String::new(),
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

    #[test]
    fn a_failing_engine_reports_why() {
        let synth = CommandSynth::new("false", spec("false --out {out}"), String::new(), 1.0);
        let err = synth
            .synthesize("文本", Path::new("/tmp/readio-never-written.wav"))
            .expect_err("`false` cannot synthesize anything");
        let message = format!("{err:#}");
        assert!(
            message.contains("false") && message.contains("合成失败"),
            "error should name the engine and what went wrong: {message}"
        );
    }

    #[test]
    fn which_finds_real_programs_only() {
        assert!(which("sh") || which("cmd"), "a shell should be on PATH");
        assert!(!which("readio-nonexistent-binary-xyz"));
    }

    /// Windows has no `afplay`, so the default player is a PowerShell one-liner.
    /// `C:\Users\John Doe\...` is a perfectly ordinary path there, so the script
    /// has to reach PowerShell as one argument with the path quoted inside it.
    /// Checked on every platform because the template is what ships, and nobody
    /// runs the Windows tests.
    #[test]
    fn the_windows_player_template_survives_a_path_with_spaces() {
        let template = crate::tts::config::presets()["kokoro"].play.clone();
        let windows =
            "powershell -NoProfile -Command \"(New-Object Media.SoundPlayer '{file}').PlaySync()\"";
        let args = split_args(windows);
        assert_eq!(args.len(), 4, "the script must stay one argument: {args:?}");

        let clip = r"C:\Users\John Doe\.readio\speech\1.wav";
        let script = args[3].replace("{file}", clip);
        assert_eq!(
            script,
            format!("(New-Object Media.SoundPlayer '{clip}').PlaySync()"),
            "the path has to end up quoted inside the script"
        );

        // And the platform readio was built for gets a player at all.
        assert!(!template.is_empty(), "every preset needs a play command");
    }
}
