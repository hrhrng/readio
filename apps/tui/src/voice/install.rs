//! Installing a speech engine from inside readio.
//!
//! readio ships no model and no bundled installer, and it is not about to grow
//! a package manager. What it does know is the one fact that turns "find the
//! project, read its README, work out which Python you have" into a keypress:
//! the PyPI distribution behind each preset. Everything else — which of `uv`,
//! `pipx` and `pip` this machine has, whether the package pins a Python — is
//! decided here, at the moment of installing, because the answer differs per
//! machine and goes stale in a release.
//!
//! Nothing runs through a shell. Every command is an argv, so a package name is
//! a package name even when it contains something exciting.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Instant;

use crate::app::expand_tilde;
use crate::voice::config::EngineSpec;

/// One command in an install, with the reason it was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub argv: Vec<String>,
    /// Where the output of a download goes, so the directory can be made first.
    pub creates: Option<PathBuf>,
    /// Resolve the Python interpreter from this installed launcher before
    /// running `argv`. Used for Hugging Face downloads that need the package
    /// environment installed by the preceding step.
    pub interpreter_for: Option<String>,
}

impl Step {
    /// The command as a reader would type it. Shown before anything runs, so
    /// that "install it for me" and "tell me what you would run" are the same
    /// feature.
    pub fn line(&self) -> String {
        self.argv.join(" ")
    }
}

/// Everything readio would do to make one engine work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub engine: String,
    /// `uv`, `pipx` or `pip`: which installer this machine turned out to have.
    pub via: &'static str,
    pub steps: Vec<Step>,
}

impl Plan {
    pub fn lines(&self) -> Vec<String> {
        self.steps.iter().map(Step::line).collect()
    }
}

/// Why an engine cannot be installed by readio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// The engine is a server or something else readio has no recipe for.
    NoRecipe { docs: String },
    /// There is a package, but nothing on this machine to install it with.
    NoInstaller,
}

/// Disk-space facts shown before a model download begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    pub download_bytes: u64,
    pub required_bytes: u64,
    pub available_bytes: u64,
}

impl Space {
    pub fn enough(self) -> bool {
        self.available_bytes >= self.required_bytes
    }
}

/// Conservative footprints for the built-in local models.
///
/// `required` includes extraction/cache duplication and a working reserve, not
/// just the bytes crossing the network. Unknown custom engines still receive a
/// filesystem check, but have no size estimate to enforce.
fn footprint(engine: &str) -> (u64, u64) {
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;
    match engine {
        // 276 MiB model + 42 MiB codec. The MOSS worker uses the same five-GiB
        // reserve when Hugging Face materialises a cold cache.
        "moss" => (318 * MIB, 5 * GIB + 636 * MIB),
        "kokoro" => (350 * MIB, GIB),
        "qwen" => (3560 * MIB, 8 * GIB),
        "piper" => (30 * MIB, 128 * MIB),
        "supertonic" => (400 * MIB, GIB),
        _ => (0, 0),
    }
}

/// Check the filesystem that holds readio's home before a download.
///
/// `df -Pk` is available on both supported host families (macOS and Linux) and
/// reports bytes without adding another platform-specific native dependency.
pub fn space(engine: &str) -> Option<Space> {
    let home = crate::paths::home();
    let probe = home
        .ancestors()
        .find(|path| path.exists())
        .unwrap_or_else(|| std::path::Path::new("."));
    let output = Command::new("df")
        .args(["-Pk"])
        .arg(probe)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let body = String::from_utf8_lossy(&output.stdout);
    let available_kib = body
        .lines()
        .rfind(|line| !line.trim().is_empty())?
        .split_whitespace()
        .nth(3)?
        .parse::<u64>()
        .ok()?;
    let (download_bytes, required_bytes) = footprint(engine);
    Some(Space {
        download_bytes,
        required_bytes,
        available_bytes: available_kib.saturating_mul(1024),
    })
}

pub fn bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Work out how to install `spec` on this machine.
///
/// An engine named under `system` is not a Python package at all and goes to
/// the machine's own package manager; everything else is a wheel.
///
/// For wheels the order is deliberate. `uv tool` and `pipx` both give a CLI its
/// own environment, which is what these packages are — command-line tools, not
/// libraries to import. Plain `pip install --user` is last because on any
/// modern distribution or Homebrew Python it now refuses outright
/// (PEP 668, "externally managed environment"), and a refusal the reader has
/// to decode is worse than saying up front that nothing suitable is here.
pub fn plan(engine: &str, spec: &EngineSpec) -> Result<Plan, Blocked> {
    if spec.pip.is_empty() && spec.system.is_empty() {
        return Err(Blocked::NoRecipe {
            docs: spec.docs.clone(),
        });
    }

    let (via, mut argv) = if !spec.system.is_empty() {
        // Not everything readio can drive is a Python package. espeak-ng is a C
        // program, and the machine's own package manager is the only sensible
        // way to get it: no wheel, no environment, nothing to keep on a path.
        if have("brew") {
            ("brew", vec!["brew".to_string(), "install".into()])
        } else if have("apt-get") {
            // Non-interactive because this runs inside readio, where there is
            // no prompt to answer, and with sudo because apt needs it.
            (
                "apt",
                vec![
                    "sudo".to_string(),
                    "apt-get".into(),
                    "install".into(),
                    "-y".into(),
                ],
            )
        } else {
            return Err(Blocked::NoInstaller);
        }
    } else if have("uv") {
        let mut argv = vec!["uv".into(), "tool".into(), "install".into()];
        if !spec.python.is_empty() {
            argv.push("--python".into());
            argv.push(spec.python.clone());
        }
        ("uv", argv)
    } else if have("pipx") {
        let mut argv = vec!["pipx".into(), "install".into()];
        if !spec.python.is_empty() {
            argv.push("--python".into());
            argv.push(format!("python{}", spec.python));
        }
        ("pipx", argv)
    } else if let Some(pip) = ["pip3", "pip"].iter().find(|p| have(p)) {
        (
            "pip",
            vec![(*pip).to_string(), "install".into(), "--user".into()],
        )
    } else {
        return Err(Blocked::NoInstaller);
    };
    argv.push(if spec.system.is_empty() {
        spec.pip.clone()
    } else {
        spec.system.clone()
    });

    let runtime_ready = {
        let program = program_of(spec);
        !program.is_empty() && locate(&program).is_some()
    };
    let mut steps = if runtime_ready {
        Vec::new()
    } else {
        vec![Step {
            argv,
            creates: None,
            interpreter_for: None,
        }]
    };

    // Voice models the engine does not ship. `curl` is assumed rather than
    // checked: it is on every macOS and effectively every Linux, and a missing
    // one shows up in the output as plainly as any other failure.
    for fetch in &spec.fetch {
        let to = expand_tilde(&fetch.to);
        steps.push(Step {
            argv: vec![
                "curl".into(),
                "-fL".into(),
                "--progress-bar".into(),
                "-o".into(),
                to.to_string_lossy().into_owned(),
                fetch.url.clone(),
            ],
            creates: Some(to),
            interpreter_for: None,
        });
    }

    let hf_models: &[&str] = match engine {
        "moss" => &[
            "mlx-community/MOSS-TTS-Nano-100M",
            "mlx-community/MOSS-Audio-Tokenizer-Nano",
        ],
        "qwen" => &["mlx-community/Qwen3-TTS-12Hz-0.6B-CustomVoice-bf16"],
        _ => &[],
    };
    if !hf_models.is_empty() {
        let models = hf_models
            .iter()
            .map(|model| format!("{model:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        let code = format!(
            "from huggingface_hub import snapshot_download\n\
             [snapshot_download(model_id=m) for m in [{models}]]"
        );
        steps.push(Step {
            argv: vec!["python".into(), "-c".into(), code],
            creates: None,
            interpreter_for: Some(program_of(spec)),
        });
    }

    Ok(Plan {
        engine: engine.to_string(),
        via,
        steps,
    })
}

/// Is `program` on the PATH?
fn have(program: &str) -> bool {
    which(program).is_some()
}

/// The program an engine's command line starts with: the one thing that has to
/// be on the machine for that engine to say anything.
pub fn program_of(spec: &EngineSpec) -> String {
    crate::voice::command::split_args(&spec.synth)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Whether an engine could speak right now.
///
/// Both halves matter. Piper's command can be installed and still have no voice
/// to speak with — the model is a separate download — and an engine that fails
/// on its first sentence is indistinguishable, to the reader, from one that was
/// never installed.
pub fn is_ready(spec: &EngineSpec) -> bool {
    let program = program_of(spec);
    if program.is_empty() || locate(&program).is_none() {
        return false;
    }
    spec.fetch
        .iter()
        .all(|fetch| expand_tilde(&fetch.to).exists())
}

/// Whether both the runtime and the model weights are already local.
pub fn model_is_ready(engine: &str, spec: &EngineSpec) -> bool {
    if engine == "openai" {
        // `curl` being installed says nothing about the server behind it. This
        // preset is external configuration, not a downloaded local model.
        return false;
    }
    if !is_ready(spec) {
        return false;
    }
    match engine {
        "moss" => {
            hf_cached("mlx-community/MOSS-TTS-Nano-100M")
                && hf_cached("mlx-community/MOSS-Audio-Tokenizer-Nano")
        }
        "qwen" => hf_cached("mlx-community/Qwen3-TTS-12Hz-0.6B-CustomVoice-bf16"),
        _ => true,
    }
}

fn hf_cached(model: &str) -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let snapshots = home
        .join(".cache/huggingface/hub")
        .join(format!("models--{}", model.replace('/', "--")))
        .join("snapshots");
    std::fs::read_dir(snapshots)
        .ok()
        .is_some_and(|mut entries| entries.next().is_some())
}

/// Resolve a program against PATH, the way a shell would.
pub fn which(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let path = PathBuf::from(program);
        return path.is_file().then_some(path);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(program);
        candidate.is_file().then_some(candidate)
    })
}

/// Where a just-installed program is, PATH or no PATH.
///
/// `uv tool install` and `pipx install` both drop their commands in
/// `~/.local/bin`, and `pip install --user` uses `~/Library/Python/3.x/bin` on
/// macOS. None of those is necessarily on the PATH readio inherited, and an
/// install that ends in "command not found" is not an install. So the places
/// those tools are known to write to are searched too, and what turns up is
/// recorded in the config as an absolute path — which beats telling a reader to
/// edit their shell profile and start again.
pub fn locate(program: &str) -> Option<PathBuf> {
    if let Some(found) = which(program) {
        return Some(found);
    }
    let mut dirs = vec![expand_tilde("~/.local/bin")];
    if let Ok(entries) = std::fs::read_dir(expand_tilde("~/Library/Python")) {
        for entry in entries.flatten() {
            dirs.push(entry.path().join("bin"));
        }
    }
    dirs.into_iter().find_map(|dir| {
        let candidate = dir.join(program);
        candidate.is_file().then_some(candidate)
    })
}

/// What a running install has to say for itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// One line of output, already trimmed.
    Line(String),
    /// A step finished; `step` is its index in the plan.
    Step { step: usize, ok: bool },
    /// The whole thing is over.
    Done { ok: bool, ms: u64 },
}

/// An install running on its own thread.
///
/// It has to be a thread: an install is tens of seconds of network, and a
/// reader who cannot scroll their book while a package downloads would rightly
/// call that a hang. The app polls this every frame, exactly as it polls the
/// speech worker.
pub struct Running {
    rx: Receiver<Progress>,
    pub plan: Plan,
    pub finished: bool,
}

impl Running {
    pub fn start(plan: Plan) -> Self {
        let (tx, rx) = channel();
        let steps = plan.steps.clone();
        // Detached on purpose: killing a half-finished `pip install` leaves a
        // broken environment behind, which is worse than letting it end.
        std::thread::Builder::new()
            .name("readio-install".into())
            .spawn(move || {
                let started = Instant::now();
                let mut ok = true;
                for (index, step) in steps.iter().enumerate() {
                    if let Some(target) = &step.creates
                        && let Some(dir) = target.parent()
                    {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let step_ok = run_step(step, &tx);
                    let _ = tx.send(Progress::Step {
                        step: index,
                        ok: step_ok,
                    });
                    if !step_ok {
                        ok = false;
                        break;
                    }
                }
                let _ = tx.send(Progress::Done {
                    ok,
                    ms: started.elapsed().as_millis() as u64,
                });
            })
            .expect("spawn install thread");
        Self {
            rx,
            plan,
            finished: false,
        }
    }

    /// Everything the install has said since the last frame.
    pub fn poll(&mut self) -> Vec<Progress> {
        let mut out = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(progress) => {
                    if matches!(progress, Progress::Done { .. }) {
                        self.finished = true;
                    }
                    out.push(progress);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.finished = true;
                    break;
                }
            }
        }
        out
    }
}

/// Run one command, forwarding its output line by line.
fn run_step(step: &Step, tx: &std::sync::mpsc::Sender<Progress>) -> bool {
    let Some((program, args)) = step.argv.split_first() else {
        return false;
    };
    let resolved;
    let program = if let Some(launcher) = step.interpreter_for.as_deref() {
        let Some(python) = interpreter_for(launcher) else {
            let _ = tx.send(Progress::Line(format!(
                "cannot find the Python environment behind {launcher}"
            )));
            return false;
        };
        resolved = python.to_string_lossy().into_owned();
        &resolved
    } else {
        program
    };
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(err) => {
            let _ = tx.send(Progress::Line(format!("{program}: {err}")));
            return false;
        }
    };

    // Both streams matter and both must be drained, or a chatty installer fills
    // a pipe buffer and blocks forever waiting for someone to read it.
    let mut readers = Vec::new();
    for stream in [
        child.stdout.take().map(Reader::Out),
        child.stderr.take().map(Reader::Err),
    ]
    .into_iter()
    .flatten()
    {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || match stream {
            Reader::Out(out) => forward(out, &tx),
            Reader::Err(err) => forward(err, &tx),
        }));
    }

    let status = child.wait();
    for reader in readers {
        let _ = reader.join();
    }
    matches!(status, Ok(status) if status.success())
}

fn interpreter_for(launcher: &str) -> Option<PathBuf> {
    let launcher = locate(launcher)?;
    let head = std::fs::read_to_string(launcher).ok()?;
    let first = head.lines().next()?.strip_prefix("#!")?.trim();
    let path = PathBuf::from(first.split_whitespace().next()?);
    (path.is_absolute() && path.exists() && !first.contains("env")).then_some(path)
}

/// Forward a subprocess's output, one screen line at a time.
///
/// Split on `\r` as well as `\n`, because a progress bar is a stream of
/// carriage returns and nothing else: reading whole lines would show a
/// download's progress in one lump after it had finished, which is the one
/// moment it is of no use.
fn forward(mut stream: impl Read, tx: &std::sync::mpsc::Sender<Progress>) {
    let mut buf = [0u8; 4096];
    let mut line: Vec<u8> = Vec::new();
    loop {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                for &byte in &buf[..read] {
                    if byte == b'\n' || byte == b'\r' {
                        emit(&mut line, tx);
                    } else {
                        line.push(byte);
                    }
                }
            }
        }
    }
    emit(&mut line, tx);
}

fn emit(line: &mut Vec<u8>, tx: &std::sync::mpsc::Sender<Progress>) {
    let text = clean(&String::from_utf8_lossy(&std::mem::take(line)));
    if !text.is_empty() {
        let _ = tx.send(Progress::Line(text));
    }
}

/// Everything a subprocess says that is not a word.
///
/// An installer's output is not data readio asked for, and it reaches a terminal
/// that readio is drawing on. Escape sequences in it would move the cursor,
/// repaint the frame or worse, so they never make it as far as the screen: what
/// survives is text.
fn clean(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            // CSI runs until a letter; anything else is a two-character escape.
            if chars.peek() == Some(&'[') {
                chars.next();
                for ch in chars.by_ref() {
                    if ch.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                chars.next();
            }
            continue;
        }
        if ch.is_control() {
            continue;
        }
        out.push(ch);
    }
    let out = out.trim_end();
    // One line of a thousand characters is a page, not a line.
    match out.char_indices().nth(LINE_LIMIT) {
        Some((cut, _)) => format!("{}…", &out[..cut]),
        None => out.to_string(),
    }
}

/// Longest line of output kept, in characters.
const LINE_LIMIT: usize = 160;

enum Reader {
    Out(std::process::ChildStdout),
    Err(std::process::ChildStderr),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::config::presets;

    #[test]
    fn a_server_has_nothing_to_install() {
        let presets = presets();
        let blocked = plan("openai", &presets["openai"]).expect_err("a server is not a package");
        assert!(
            matches!(blocked, Blocked::NoRecipe { docs } if docs.contains("http")),
            "and it should point at where to read about it"
        );
    }

    #[test]
    fn every_local_engine_knows_its_package() {
        for (name, spec) in presets() {
            if name == "openai" {
                continue;
            }
            assert!(
                !spec.pip.is_empty() || !spec.system.is_empty(),
                "{name} can be installed but readio does not know what to install"
            );
            assert!(
                !spec.docs.is_empty(),
                "{name} has no page to send someone to when the install fails"
            );
        }
    }

    /// The plan is worked out from what is on the machine, so this asserts the
    /// shape rather than the exact tool: CI has `pip`, a developer's laptop
    /// usually has `uv`, and both are correct.
    #[test]
    fn a_plan_installs_the_package_and_then_fetches_what_it_needs() {
        let presets = presets();
        let Ok(plan) = plan("piper", &presets["piper"]) else {
            // No installer at all on this machine: nothing to assert, and
            // failing here would only mean "CI has no Python".
            return;
        };
        assert!(matches!(plan.via, "uv" | "pipx" | "pip"));
        let package = plan
            .steps
            .iter()
            .find(|step| step.line().contains("piper-tts"));
        assert!(
            package.is_some() || locate(&program_of(&presets["piper"])).is_some(),
            "the plan may skip only a runtime that is already installed: {:?}",
            plan.lines()
        );
        assert_eq!(
            plan.steps.len(),
            if package.is_some() { 3 } else { 2 },
            "piper needs its voice fetched as well: {:?}",
            plan.lines()
        );
        assert!(
            plan.steps.iter().any(|step| step.line().contains(".onnx")),
            "and the voice is the model itself: {}",
            plan.lines().join("\n")
        );
        assert!(
            plan.steps
                .iter()
                .find(|step| step.line().contains(".onnx"))
                .is_some_and(|step| step.creates.is_some()),
            "a download has to say where it lands, or the directory is never made"
        );
    }

    #[test]
    fn a_pinned_python_reaches_the_command_line() {
        let presets = presets();
        let Ok(plan) = plan("kokoro", &presets["kokoro"]) else {
            return;
        };
        let Some(line) = plan
            .steps
            .iter()
            .find(|step| step.argv.last().is_some_and(|arg| arg == "kokoro-tts"))
            .map(Step::line)
        else {
            assert!(
                locate(&program_of(&presets["kokoro"])).is_some(),
                "the install step may be absent only when the runtime is already here"
            );
            return;
        };
        if plan.via == "pip" {
            return; // plain pip has no way to pick an interpreter.
        }
        assert!(
            line.contains("3.12"),
            "kokoro-tts refuses to install on 3.13, so the pin has to be passed: {line}"
        );
    }

    #[test]
    fn which_finds_something_that_is_certainly_there() {
        assert!(which("sh").is_some(), "every unix has a shell");
        assert!(which("readio-definitely-not-installed").is_none());
    }

    #[test]
    fn model_downloads_are_checked_against_working_space_not_just_archive_size() {
        let (download, required) = footprint("moss");
        assert!(download > 300 * 1024 * 1024);
        assert!(
            required > download * 2,
            "cache materialisation and a reserve need more than the wire bytes"
        );
        assert!(
            !Space {
                download_bytes: download,
                required_bytes: required,
                available_bytes: required - 1,
            }
            .enough()
        );
    }

    /// An installer's output lands in a terminal readio is drawing on, so what
    /// comes back has to be text and nothing else.
    #[test]
    fn output_arrives_as_text_and_never_as_an_escape_sequence() {
        assert_eq!(
            clean("\u{1b}[32mInstalled\u{1b}[0m 3 packages"),
            "Installed 3 packages"
        );
        assert_eq!(clean("done\u{7}\u{8}"), "done");
        assert_eq!(clean("  Resolved 14 packages  "), "  Resolved 14 packages");
        let long = clean(&"#".repeat(400));
        assert!(
            long.chars().count() <= LINE_LIMIT + 1,
            "one line of a thousand characters is a page: {}",
            long.chars().count()
        );
    }
}
