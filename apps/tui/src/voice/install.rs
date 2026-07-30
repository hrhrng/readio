//! Installing a speech engine from inside readio.
//!
//! readio ships no model or Python in its small native binary. When a Python
//! model is requested it bootstraps a pinned, checksum-verified uv binary into
//! the user's cache. uv then owns the Python and tool environments below that
//! cache, while retaining its ordinary shared download cache. The result needs
//! no system Python and does not contaminate one when it exists.
//!
//! Nothing runs through a shell. Every command is an argv, so a package name is
//! a package name even when it contains something exciting.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Instant;

use sha2::{Digest, Sha256};

use crate::app::expand_tilde;
use crate::voice::config::EngineSpec;

/// One command in an install, with the reason it was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub argv: Vec<String>,
    /// Environment belonging to this command. Runtime location is explicit,
    /// while cache variables are intentionally absent so uv and Hugging Face
    /// can reuse the reader's standard caches.
    pub env: Vec<(String, String)>,
    /// Where the output of a download goes, so the directory can be made first.
    pub creates: Option<PathBuf>,
    /// Expected digest for a downloaded executable archive.
    pub sha256: Option<String>,
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
        self.env
            .iter()
            .map(|(key, value)| format!("{key}={}", shell_word(value)))
            .chain(self.argv.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn shell_word(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "/._-:".contains(ch))
    {
        return word.to_string();
    }
    format!("'{}'", word.replace('\'', "'\"'\"'"))
}

/// Everything readio would do to make one engine work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub engine: String,
    /// The managed runtime, or the system package manager for a native engine.
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
    /// A native engine needs a system package manager this machine lacks.
    NoInstaller,
    /// Readio has no verified uv build for this OS and CPU combination.
    UnsupportedRuntime { platform: String },
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

/// Human-facing catalog facts. These describe the model rather than its local
/// state, so the Voice workspace can show them before anything is downloaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelProfile {
    pub scale_zh: &'static str,
    pub scale_en: &'static str,
    pub language_zh: &'static str,
    pub language_en: &'static str,
    pub download_bytes: u64,
}

pub fn profile(engine: &str) -> Option<ModelProfile> {
    let (download_bytes, _) = footprint(engine);
    let (scale_zh, scale_en, language_zh, language_en) = match engine {
        "moss" => ("120M 参数", "120M parameters", "中文", "Mandarin Chinese"),
        "kokoro" => ("82M 参数", "82M parameters", "英语", "English"),
        "qwen" => ("0.6B 参数", "0.6B parameters", "中文", "Mandarin Chinese"),
        "piper" => (
            "约 7–32M 参数",
            "about 7–32M parameters",
            "随音色（中文 / 英语）",
            "voice-specific (Chinese / English)",
        ),
        "supertonic" => (
            "99M 参数",
            "99M parameters",
            "多语言（偏英语）",
            "multilingual (English strongest)",
        ),
        "espeak" => (
            "非神经模型",
            "non-neural",
            "多语言（机械音）",
            "multilingual (robotic)",
        ),
        "openai" | "server" => (
            "远端服务",
            "remote service",
            "由服务配置",
            "configured by the service",
        ),
        _ => return None,
    };
    Some(ModelProfile {
        scale_zh,
        scale_en,
        language_zh,
        language_en,
        download_bytes,
    })
}

/// Check the filesystem that holds readio's home before a download.
///
/// `df -Pk` is available on both supported host families (macOS and Linux) and
/// reports bytes without adding another platform-specific native dependency.
pub fn space(engine: &str) -> Option<Space> {
    // MOSS/Qwen weights and Supertonic's self-fetched weights live in the
    // user's cache; direct model files live with readio's data. Probe the
    // filesystem that will actually receive the large part of the download.
    let destination = if matches!(engine, "moss" | "qwen" | "supertonic") {
        hf_cache_dir()
    } else {
        crate::paths::home()
    };
    let probe = destination
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
/// the machine's package manager. Every wheel uses the same readio-owned uv,
/// tool directory and managed Python directory. No cache directory is set:
/// uv's user cache is deliberately shared with the rest of the machine.
pub fn plan(engine: &str, spec: &EngineSpec) -> Result<Plan, Blocked> {
    if spec.pip.is_empty() && spec.system.is_empty() {
        return Err(Blocked::NoRecipe {
            docs: spec.docs.clone(),
        });
    }

    let runtime_ready = {
        let program = program_of(spec);
        !program.is_empty() && locate(&program).is_some()
    };
    let mut steps = Vec::new();
    let via = if !spec.system.is_empty() {
        // Not everything readio can drive is a Python package. espeak-ng is a C
        // program, and the machine's own package manager is the only sensible
        // way to get it: no wheel, no environment, nothing to keep on a path.
        let (via, mut argv) = if have("brew") {
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
        };
        argv.push(spec.system.clone());
        if !runtime_ready {
            steps.push(Step {
                argv,
                env: Vec::new(),
                creates: None,
                sha256: None,
                interpreter_for: None,
            });
        }
        via
    } else {
        if !runtime_ready {
            let uv = uv_program();
            if !uv.is_file() {
                steps.extend(uv_bootstrap()?);
            }
            let mut argv = vec![
                uv.to_string_lossy().into_owned(),
                "tool".into(),
                "install".into(),
                "--managed-python".into(),
            ];
            if !spec.python.is_empty() {
                argv.push("--python".into());
                argv.push(spec.python.clone());
            }
            argv.push(spec.pip.clone());
            steps.push(Step {
                argv,
                env: runtime_env(),
                creates: None,
                sha256: None,
                interpreter_for: None,
            });
        }
        "readio uv"
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
            env: Vec::new(),
            creates: Some(to),
            sha256: None,
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
            env: Vec::new(),
            creates: None,
            sha256: None,
            interpreter_for: Some(program_of(spec)),
        });
    }

    Ok(Plan {
        engine: engine.to_string(),
        via,
        steps,
    })
}

/// The runtime paths uv owns. Cache variables are conspicuously absent: the
/// global uv cache is content-addressed and safe to share, so a wheel or Python
/// archive already fetched by another project should be a cache hit here.
fn runtime_env() -> Vec<(String, String)> {
    [
        ("UV_TOOL_DIR", crate::paths::runtime_tools_dir()),
        ("UV_TOOL_BIN_DIR", crate::paths::runtime_bin_dir()),
        ("UV_PYTHON_INSTALL_DIR", crate::paths::runtime_python_dir()),
    ]
    .into_iter()
    .map(|(name, path)| (name.to_string(), path.to_string_lossy().into_owned()))
    .collect()
}

const UV_VERSION: &str = "0.12.0";

fn uv_program() -> PathBuf {
    crate::paths::runtime_dir()
        .join("uv")
        .join(UV_VERSION)
        .join("bin/uv")
}

struct UvRelease {
    target: &'static str,
    sha256: &'static str,
}

/// A fixed release per supported host. The digest comes from the matching
/// `.sha256` asset published in astral-sh/uv's GitHub release.
fn uv_release() -> Option<UvRelease> {
    let release = match (
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::FAMILY,
    ) {
        ("macos", "aarch64", _) => UvRelease {
            target: "aarch64-apple-darwin",
            sha256: "2b9e582af54f84fa50c115427451a6c13e80f43b52f8282b8af5791077317bbf",
        },
        ("macos", "x86_64", _) => UvRelease {
            target: "x86_64-apple-darwin",
            sha256: "d41593beaefc54bab7d062af0ef6ca093bfb81d001d58ebbef39e44423f9c496",
        },
        ("linux", "aarch64", _) => UvRelease {
            target: if cfg!(target_env = "musl") {
                "aarch64-unknown-linux-musl"
            } else {
                "aarch64-unknown-linux-gnu"
            },
            sha256: if cfg!(target_env = "musl") {
                "936fbbf20188a2b1c66bce3dca3f4009a5c9cdf12bb2bbd084e71926f75d6a15"
            } else {
                "2c5d6e3092cc5223b10ff403880cc75121bf64e84644e7a0c69f643b0d89ac95"
            },
        },
        ("linux", "x86_64", _) => UvRelease {
            target: if cfg!(target_env = "musl") {
                "x86_64-unknown-linux-musl"
            } else {
                "x86_64-unknown-linux-gnu"
            },
            sha256: if cfg!(target_env = "musl") {
                "3340a9d8cffc4d801bc1a7459ebfaf5790c79400720d9b6963d806f058526684"
            } else {
                "eaf842262aa1c418d8ecc5605f02ee1ebfd369124fa48548e85f9481a47831a9"
            },
        },
        _ => return None,
    };
    Some(release)
}

fn uv_bootstrap() -> Result<Vec<Step>, Blocked> {
    let release = uv_release().ok_or_else(|| Blocked::UnsupportedRuntime {
        platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
    })?;
    let archive_name = format!("uv-{}.tar.gz", release.target);
    let archive = crate::paths::runtime_dir()
        .join("downloads")
        .join(&archive_name);
    let uv = uv_program();
    let bin = uv
        .parent()
        .expect("managed uv always has a bin directory")
        .to_path_buf();
    let url =
        format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{archive_name}");
    Ok(vec![
        Step {
            argv: vec![
                "curl".into(),
                "-fL".into(),
                "--progress-bar".into(),
                "-o".into(),
                archive.to_string_lossy().into_owned(),
                url,
            ],
            env: Vec::new(),
            creates: Some(archive.clone()),
            sha256: Some(release.sha256.to_string()),
            interpreter_for: None,
        },
        Step {
            argv: vec![
                "tar".into(),
                "-xzf".into(),
                archive.to_string_lossy().into_owned(),
                "-C".into(),
                bin.to_string_lossy().into_owned(),
                "--strip-components=1".into(),
            ],
            env: Vec::new(),
            creates: Some(uv),
            sha256: None,
            interpreter_for: None,
        },
    ])
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
    let snapshots = hf_cache_dir()
        .join(format!("models--{}", model.replace('/', "--")))
        .join("snapshots");
    std::fs::read_dir(snapshots)
        .ok()
        .is_some_and(|mut entries| entries.next().is_some())
}

/// Hugging Face's own cache precedence. The downloader inherits these same
/// variables, so readiness has to look in the same place it writes.
fn hf_cache_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("HF_HUB_CACHE") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("HF_HOME") {
        return PathBuf::from(path).join("hub");
    }
    if let Some(path) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(path).join("huggingface/hub");
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache/huggingface/hub")
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
/// Readio's managed bin directory comes first. Legacy locations remain
/// searchable so an existing installation made by an older release is not
/// thrown away merely because the installer has become self-contained.
pub fn locate(program: &str) -> Option<PathBuf> {
    if !program.contains('/') {
        let managed = crate::paths::runtime_bin_dir().join(program);
        if managed.is_file() {
            return Some(managed);
        }
    }
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
        // Detached on purpose: killing a half-finished tool install leaves a
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
        .envs(step.env.iter().map(|(key, value)| (key, value)))
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
    if !matches!(status, Ok(status) if status.success()) {
        return false;
    }
    if let Some(target) = step.creates.as_deref()
        && !target.exists()
    {
        let _ = tx.send(Progress::Line(format!(
            "{} did not create {}",
            step.argv.first().map(String::as_str).unwrap_or("command"),
            target.display()
        )));
        return false;
    }
    if let (Some(expected), Some(target)) = (step.sha256.as_deref(), step.creates.as_deref()) {
        match sha256(target) {
            Ok(actual) if actual == expected => {}
            Ok(actual) => {
                let _ = tx.send(Progress::Line(format!(
                    "checksum mismatch for {}: expected {expected}, got {actual}",
                    target.display()
                )));
                return false;
            }
            Err(err) => {
                let _ = tx.send(Progress::Line(format!(
                    "cannot verify {}: {err}",
                    target.display()
                )));
                return false;
            }
        }
    }
    true
}

fn sha256(path: &std::path::Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        digest.update(&buf[..read]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
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

    /// A Python-backed engine is still installable when the machine has no
    /// Python, pip or uv of its own. Readio owns the interpreter and tool
    /// environment, while uv's ordinary user cache remains shared with any
    /// other uv installation on the machine.
    #[test]
    fn python_engines_use_a_private_runtime_but_the_users_cache() {
        let mut spec = presets()["kokoro"].clone();
        // A name no real machine has makes the runtime install step mandatory,
        // without mutating PATH or depending on what CI happens to provide.
        spec.synth = "readio-runtime-contract-test --output {out} --voice {voice}".to_string();

        let plan = plan("runtime-contract", &spec)
            .expect("a Python model needs no system Python or package installer");
        assert_eq!(plan.via, "readio uv");

        let install = plan
            .steps
            .iter()
            .find(|step| step.argv.last().is_some_and(|arg| arg == "kokoro-tts"))
            .expect("the private runtime installs the model command");
        let line = install.line();
        let managed_uv = dirs::cache_dir()
            .unwrap_or_else(|| crate::paths::home().join("cache"))
            .join(format!("readio/runtime/uv/{UV_VERSION}/bin/uv"));
        assert!(
            line.contains(managed_uv.to_string_lossy().as_ref()),
            "the installer itself belongs to readio: {line}"
        );
        assert!(
            line.contains("UV_TOOL_DIR=")
                && line.contains("UV_TOOL_BIN_DIR=")
                && line.contains("UV_PYTHON_INSTALL_DIR="),
            "tools and interpreters must stay under readio's runtime: {line}"
        );
        assert!(
            line.contains("--managed-python") && line.contains("3.12"),
            "the requested Python is downloaded and managed by uv: {line}"
        );
        assert!(
            !line.contains("UV_CACHE_DIR")
                && !line.contains("HF_HOME")
                && !line.contains("HF_HUB_CACHE"),
            "user package and model caches must remain reusable: {line}"
        );

        if !managed_uv.exists() {
            assert!(
                plan.lines()
                    .iter()
                    .any(|line| line.contains("github.com/astral-sh/uv/releases/download/")),
                "a fresh machine needs a complete uv bootstrap: {:?}",
                plan.lines()
            );
        }
    }

    /// The runtime bootstrap is conditional, so this asserts the stable shape:
    /// one private tool environment followed by the model files it needs.
    #[test]
    fn a_plan_installs_the_package_and_then_fetches_what_it_needs() {
        let presets = presets();
        let plan = plan("piper", &presets["piper"]).expect("piper has a managed install");
        assert_eq!(plan.via, "readio uv");
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
            plan.steps
                .iter()
                .filter(|step| step.argv.first().is_some_and(|arg| arg == "curl")
                    && step.line().contains("huayan"))
                .count(),
            2,
            "piper needs both of its voice files: {:?}",
            plan.lines(),
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

    #[test]
    fn runtime_archives_are_verified_with_sha256() {
        let path = std::env::temp_dir().join(format!(
            "readio-sha256-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, b"abc").expect("fixture");
        assert_eq!(
            sha256(&path).expect("digest"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(path);
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
