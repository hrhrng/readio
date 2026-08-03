//! An engine that stays running.
//!
//! Most speech CLIs are built for "render this file overnight", not for "say
//! this sentence now". Driving one per sentence pays its whole startup every
//! time: for Kokoro that is a fresh Python, an ONNX session, and two lazy
//! imports inside the synthesis call — eight seconds to produce four seconds of
//! speech. An engine slower than speech cannot be caught up with. Rendering
//! ahead does not help, because the queue drains faster than it fills, so the
//! reading stalls at every sentence boundary no matter how deep the buffer.
//!
//! Kept resident, the same model answers in about a third of real time. The
//! startup is paid once, while the reader is still pressing enter.
//!
//! The protocol is a line of JSON in, a line of JSON out, over a pipe readio
//! owns. Not a port: a port needs a number nobody has, a health check, and a
//! story about what happens when two readios are open. A pipe has none of
//! those questions — the worker is this process's child, and it dies with it.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};

use crate::i18n::tf;

/// The worker scripts, carried in the binary, one per engine readio can keep
/// resident.
///
/// Shipping them as files beside the binary would mean readio's installer has
/// somewhere to put them and every reader has the same somewhere. It does not:
/// readio is a single file people copy onto a path. So the scripts travel
/// inside it and are written out next to the config, where a curious reader can
/// read one and an unhappy one can edit it.
///
/// Keyed by engine name rather than carried in [`crate::voice::config::EngineSpec`]
/// because which script drives an engine is not a setting: a reader who wants
/// their own worker points `serve:` at it directly, and one who does not should
/// never have to name readio's.
const WORKERS: &[(&str, &str)] = &[
    ("kokoro", include_str!("kokoro_worker.py")),
    ("moss", include_str!("moss_worker.py")),
    ("qwen", include_str!("qwen_worker.py")),
];

/// The worker readio ships for `engine`, if it ships one.
fn worker_source(engine: &str) -> Option<&'static str> {
    WORKERS
        .iter()
        .find(|(name, _)| *name == engine)
        .map(|(_, source)| *source)
}

/// How long to wait for a model to load before deciding it is not going to.
///
/// Generous on purpose: 300 MB of weights off a cold disk is slow, and a
/// reader who waited eight seconds a sentence until now will forgive fifteen
/// once. What this timeout is really for is the other case — a worker that
/// will never answer, which without it would hang read-aloud with no message.
const READY_TIMEOUT: Duration = Duration::from_secs(45);

/// A speech engine kept alive between sentences.
pub struct Resident {
    argv: Vec<String>,
    cooperative_cancel: bool,
    generation: AtomicU64,
    active_pid: AtomicU32,
    /// `None` until the first sentence, and again after a failure, which is
    /// what makes the next sentence a retry rather than a second corpse.
    session: Mutex<Option<Session>>,
}

struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

enum ExchangeOutcome {
    Rendered,
    Cancelled,
}

struct ActivePid<'a>(&'a AtomicU32);

impl Drop for ActivePid<'_> {
    fn drop(&mut self) {
        self.0.store(0, Ordering::SeqCst);
    }
}

/// One sentence, as the worker expects it.
pub struct Request<'a> {
    pub text: &'a str,
    pub out: &'a Path,
    pub voice: &'a str,
    pub lang: &'a str,
    pub speed: f32,
    pub params: &'a str,
}

impl Resident {
    pub fn new(argv: Vec<String>) -> Self {
        Self {
            argv,
            cooperative_cancel: false,
            generation: AtomicU64::new(0),
            active_pid: AtomicU32::new(0),
            session: Mutex::new(None),
        }
    }

    /// A worker that implements readio's cooperative SIGUSR1 contract.
    pub(crate) fn cancellable(argv: Vec<String>) -> Self {
        Self {
            argv,
            cooperative_cancel: true,
            generation: AtomicU64::new(0),
            active_pid: AtomicU32::new(0),
            session: Mutex::new(None),
        }
    }

    /// Interrupt the active request without unloading its resident model.
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        if !self.cooperative_cancel {
            return;
        }
        #[cfg(unix)]
        {
            let pid = self.active_pid.load(Ordering::SeqCst);
            if pid != 0 {
                // SAFETY: `pid` is read from this Resident's live Child. A
                // failed delivery merely means it exited between the load and
                // the signal; the exchange then observes the closed pipe.
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGUSR1);
                }
            }
        }
    }

    /// The program that will be run, for error messages and availability checks.
    pub fn program(&self) -> Option<&str> {
        self.argv.first().map(String::as_str)
    }

    /// Start the worker if it is not already up.
    ///
    /// Called once when read-aloud begins rather than lazily on the first
    /// sentence, so the model loads while the reader is still opening the book.
    pub fn warm(&self) -> Result<()> {
        let mut session = self.session.lock().expect("resident lock");
        if session.is_some() {
            return Ok(());
        }
        *session = Some(self.start()?);
        Ok(())
    }

    /// Render one sentence, starting the worker if this is the first.
    pub fn render(&self, request: Request<'_>) -> Result<()> {
        let generation = self.generation.load(Ordering::SeqCst);
        self.render_at(request, generation)
    }

    /// Render only while the caller's route generation is still current.
    pub(crate) fn render_at(&self, request: Request<'_>, generation: u64) -> Result<()> {
        if self.generation.load(Ordering::SeqCst) != generation {
            return Err(anyhow!("synthesis cancelled"));
        }
        let mut guard = self.session.lock().expect("resident lock");
        if guard.is_none() {
            *guard = Some(self.start()?);
        }
        let session = guard.as_mut().expect("just started");

        if self.generation.load(Ordering::SeqCst) != generation {
            return Err(anyhow!("synthesis cancelled"));
        }
        self.active_pid.store(session.child.id(), Ordering::SeqCst);
        let _active = ActivePid(&self.active_pid);
        if self.generation.load(Ordering::SeqCst) != generation {
            return Err(anyhow!("synthesis cancelled"));
        }

        let job = serde_json::json!({
            "text": request.text,
            "out": request.out.to_string_lossy(),
            "voice": request.voice,
            "lang": request.lang,
            "speed": request.speed,
            "params": request.params,
        });

        // A write or read that fails here means the worker is gone — killed by
        // the system, or crashed on something the reply never got to describe.
        // Drop it, so the next sentence starts a new one.
        match Self::exchange(session, &job.to_string()) {
            Ok(ExchangeOutcome::Rendered) => Ok(()),
            Ok(ExchangeOutcome::Cancelled) => Err(anyhow!("synthesis cancelled")),
            Err(err) => {
                *guard = None;
                Err(err)
            }
        }
    }

    /// One request, one reply.
    ///
    /// The read blocks. By this point the worker has already loaded its model
    /// and answered once, so the failure it is exposed to is the worker dying
    /// mid-sentence — which closes the pipe and ends the read rather than
    /// hanging on it.
    fn exchange(session: &mut Session, job: &str) -> Result<ExchangeOutcome> {
        writeln!(session.stdin, "{job}")?;
        session.stdin.flush()?;

        let mut line = String::new();
        if session.stdout.read_line(&mut line)? == 0 {
            bail!(tf("voice.worker_gone", &[&""]));
        }
        let reply: serde_json::Value = serde_json::from_str(line.trim())?;
        if reply.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
            return Ok(ExchangeOutcome::Rendered);
        }
        if reply.get("cancelled").and_then(serde_json::Value::as_bool) == Some(true) {
            return Ok(ExchangeOutcome::Cancelled);
        }
        let why = reply
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(line.trim());
        Err(anyhow!(why.to_string()))
    }

    /// Spawn the worker and wait for it to say it is ready.
    fn start(&self) -> Result<Session> {
        let (program, args) = self
            .argv
            .split_first()
            .ok_or_else(|| anyhow!(tf("voice.worker_no_command", &[&""])))?;

        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The worker's own diagnostics go to the terminal readio is drawing
            // on, which would corrupt the frame. Nothing it says on stderr is
            // load-bearing: what matters comes back as JSON.
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| anyhow!(tf("voice.worker_start", &[&program, &err])))?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        let mut reader = BufReader::new(stdout);

        // Read the ready line on a thread so that a worker which never answers
        // is a message rather than a frozen reader. The reader comes back
        // through the channel, because it is needed for every sentence after.
        let (tx, rx) = channel();
        thread::spawn(move || {
            let mut line = String::new();
            let read = reader.read_line(&mut line);
            let _ = tx.send((reader, read.map(|_| line)));
        });

        let (reader, line) = match rx.recv_timeout(READY_TIMEOUT) {
            Ok(pair) => pair,
            Err(RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                bail!(tf("voice.worker_slow", &[&READY_TIMEOUT.as_secs()]));
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                bail!(tf("voice.worker_gone", &[&""]));
            }
        };

        let line = line?;
        let hello: serde_json::Value = serde_json::from_str(line.trim()).unwrap_or_default();
        if hello.get("ready").and_then(serde_json::Value::as_bool) != Some(true) {
            let _ = child.kill();
            let why = hello
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(line.trim());
            bail!(why.to_string());
        }

        Ok(Session {
            child,
            stdin,
            stdout: reader,
        })
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        // Closing stdin ends the worker's read loop, which is how it gets to
        // exit on its own terms; the kill is for the case where it is busy
        // inside an inference and will not notice for another second.
        if let Ok(mut session) = self.session.lock()
            && let Some(mut session) = session.take()
        {
            drop(session.stdin);
            let _ = session.child.kill();
            let _ = session.child.wait();
        }
    }
}

/// Write out the worker script for `engine`, and return where it landed.
///
/// Rewritten whenever it differs from the one in this binary, so upgrading
/// readio upgrades the script, while an edit survives until then — which is the
/// same bargain the config file offers.
///
/// `None` when readio ships no worker for this engine, which is not a failure:
/// it is every engine that has nothing to keep resident, and the caller falls
/// back to the plain command line.
pub fn worker_script(dir: &Path, engine: &str) -> Result<Option<PathBuf>> {
    let Some(source) = worker_source(engine) else {
        return Ok(None);
    };
    let name = format!("{engine}_worker.py");
    let path = dir.join(&name);
    let current = std::fs::read_to_string(&path).ok();
    if current.as_deref() != Some(source) {
        std::fs::create_dir_all(dir)?;
        // Written beside its destination and renamed into place: a script
        // half-written when the machine went down is a syntax error the reader
        // would have to diagnose from a Python traceback.
        let staging = dir.join(format!("{name}.{}", std::process::id()));
        std::fs::write(&staging, source)?;
        std::fs::rename(&staging, &path)?;
    }
    Ok(Some(path))
}

/// The interpreter that already has this engine's package.
///
/// `uv tool install kokoro-tts` puts the package in an environment of its own
/// and leaves a launcher on the path whose first line points back into it.
/// Reading that line is how readio finds a Python with `kokoro_onnx` importable
/// without installing anything a second time — and without guessing at a
/// virtualenv layout that is not readio's to know.
pub fn interpreter_behind(program: &str) -> Option<PathBuf> {
    let launcher = which(program)?;
    let head = std::fs::read_to_string(&launcher).ok()?;
    let first = head.lines().next()?.strip_prefix("#!")?.trim();
    // `#!/usr/bin/env python3` names no particular Python, so it says nothing
    // about where the package is: better to admit that than to run a Python
    // that will fail on the import.
    let path = PathBuf::from(first.split_whitespace().next()?);
    (path.is_absolute() && path.exists() && !first.contains("env")).then_some(path)
}

/// `which`, without the crate.
fn which(program: &str) -> Option<PathBuf> {
    crate::voice::command::resolve(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_is_written_once_and_rewritten_when_it_changes() {
        let home = tempdir();
        let first = worker_script(&home, "kokoro")
            .expect("write")
            .expect("kokoro has a worker");
        assert!(first.exists(), "the worker script should land on disk");

        std::fs::write(&first, "# someone has been editing").expect("edit");
        let again = worker_script(&home, "kokoro")
            .expect("rewrite")
            .expect("still kokoro's");
        assert_eq!(
            std::fs::read_to_string(&again).expect("read"),
            worker_source("kokoro").expect("shipped"),
            "a script that no longer matches the binary is replaced by it"
        );
    }

    /// Each engine gets its own script, and an engine readio ships none for says
    /// so rather than being handed somebody else's — a Qwen `serve:` line running
    /// the Kokoro worker would fail on an import, several seconds later, with a
    /// message about the wrong model.
    #[test]
    fn every_resident_engine_gets_its_own_worker() {
        let home = tempdir();
        let mut seen = Vec::new();
        for (engine, _) in WORKERS {
            let path = worker_script(&home, engine)
                .expect("write")
                .expect("a shipped worker");
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("utf8")
                .to_string();
            assert_eq!(
                name,
                format!("{engine}_worker.py"),
                "the file should name the engine it drives"
            );
            let body = std::fs::read_to_string(&path).expect("read");
            assert_eq!(
                body,
                worker_source(engine).expect("shipped"),
                "{engine} got the wrong script"
            );
            seen.push(body);
        }
        assert!(
            seen.len() >= 2,
            "there should be more than one worker by now"
        );
        assert_ne!(seen[0], seen[1], "and they should not be the same script");

        assert!(
            worker_script(&home, "espeak")
                .expect("not an error")
                .is_none(),
            "an engine with nothing to keep resident gets no script"
        );
    }

    /// A shipped `serve:` line is a promise that readio knows how to start that
    /// engine. Listing a resident preset without carrying its worker silently
    /// drops it to the one-shot fallback, which is exactly the startup cost the
    /// preset claims to avoid.
    #[test]
    fn every_shipped_resident_preset_carries_its_worker() {
        let home = tempdir();
        for (name, spec) in crate::voice::config::presets() {
            if spec.serve.trim().is_empty() {
                continue;
            }
            assert!(
                worker_script(&home, &name)
                    .expect("worker lookup")
                    .is_some(),
                "{name} has a resident preset but no shipped worker"
            );
        }
    }

    /// Exercise Qwen's real worker process without downloading the model.
    ///
    /// The fake module deliberately writes through both Python's stdout and fd
    /// 1 while loading. Those are things mlx-audio itself does, and either one
    /// reaching the protocol pipe would replace the ready JSON with library
    /// chatter. A successful render also proves replies are flushed: otherwise
    /// this call waits forever despite the worker having finished.
    #[test]
    fn qwen_worker_keeps_library_noise_out_of_its_protocol() {
        let home = tempdir();
        let package = home.join("mlx_audio/tts");
        let model = home.join("configured-qwen");
        std::fs::create_dir_all(&package).expect("fake package");
        std::fs::create_dir_all(&model).expect("fake local model");
        std::fs::write(home.join("mlx_audio/__init__.py"), "").expect("package");
        std::fs::write(package.join("__init__.py"), "").expect("package");
        std::fs::write(
            package.join("utils.py"),
            format!(
                r#"import os
import numpy as np

print("python library noise")
os.write(1, b"native library noise\n")

class Segment:
    def __init__(self):
        self.audio = np.linspace(-0.25, 0.25, 240, dtype=np.float32)
        self.sample_rate = 24000

class Model:
    def generate(self, *, text, voice, lang_code, speed, verbose):
        if text != "。":
            assert voice == "vivian", voice
            assert lang_code == "english", lang_code
            assert speed == 1.25, speed
            assert verbose is False
        yield Segment()

def load_model(model):
    assert model == {model}, model
    return Model()
"#,
                model = serde_json::to_string(&model.to_string_lossy()).expect("json model path"),
            ),
        )
        .expect("fake mlx-audio");

        let worker = worker_script(&home, "qwen")
            .expect("write worker")
            .expect("qwen worker");
        let Some(python) = which("python3").or_else(|| which("python")) else {
            return;
        };
        let resident = Resident::new(vec![
            python.to_string_lossy().into_owned(),
            worker.to_string_lossy().into_owned(),
            "--model".to_string(),
            model.to_string_lossy().into_owned(),
        ]);

        resident
            .warm()
            .expect("ready JSON should be the first line");
        let wav = home.join("spoken.wav");
        resident
            .render(Request {
                text: "A configured sentence.",
                out: &wav,
                voice: "vivian",
                lang: "english",
                speed: 1.25,
                params: "",
            })
            .expect("one request should receive one successful reply");
        assert!(
            crate::voice::wav::duration_ms(&wav).expect("valid wav") > 0,
            "the worker should write playable audio"
        );
    }

    /// Cancelling Qwen must unwind one generation request, not terminate the
    /// Python process and reload 0.6B parameters. The fake model sleeps only
    /// for the requested sentence; warm-up and the replacement stay cheap, so
    /// the boot log tells those two behaviours apart directly.
    #[cfg(unix)]
    #[test]
    fn qwen_worker_cancels_one_request_and_keeps_the_model_loaded() {
        let home = tempdir();
        let package = home.join("mlx_audio/tts");
        let model = home.join("configured-qwen");
        let active = home.join("active");
        let boots = home.join("boots");
        std::fs::create_dir_all(&package).expect("fake package");
        std::fs::create_dir_all(&model).expect("fake local model");
        std::fs::write(home.join("mlx_audio/__init__.py"), "").expect("package");
        std::fs::write(package.join("__init__.py"), "").expect("package");
        std::fs::write(
            package.join("utils.py"),
            format!(
                r#"import time
import numpy as np

ACTIVE = {active}
BOOTS = {boots}

class Segment:
    def __init__(self):
        self.audio = np.linspace(-0.25, 0.25, 240, dtype=np.float32)
        self.sample_rate = 24000

class Model:
    def generate(self, *, text, voice, lang_code, speed, verbose):
        if text == "stale":
            with open(ACTIVE, "w", encoding="utf-8") as out:
                out.write("active")
            time.sleep(3)
        yield Segment()

def load_model(model):
    with open(BOOTS, "a", encoding="utf-8") as out:
        out.write("boot\n")
    return Model()
"#,
                active = serde_json::to_string(&active.to_string_lossy()).expect("active path"),
                boots = serde_json::to_string(&boots.to_string_lossy()).expect("boots path"),
            ),
        )
        .expect("fake mlx-audio");

        let worker = worker_script(&home, "qwen")
            .expect("write worker")
            .expect("qwen worker");
        let Some(python) = which("python3").or_else(|| which("python")) else {
            return;
        };
        let resident = std::sync::Arc::new(Resident::cancellable(vec![
            python.to_string_lossy().into_owned(),
            worker.to_string_lossy().into_owned(),
            "--model".to_string(),
            model.to_string_lossy().into_owned(),
        ]));
        resident.warm().expect("qwen ready");

        let stale_resident = std::sync::Arc::clone(&resident);
        let stale_out = home.join("stale.wav");
        let stale = std::thread::spawn(move || {
            stale_resident.render(Request {
                text: "stale",
                out: &stale_out,
                voice: "serena",
                lang: "chinese",
                speed: 1.0,
                params: "",
            })
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !active.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(active.exists(), "the slow qwen request never started");

        let cancelled_at = std::time::Instant::now();
        resident.cancel();
        assert!(
            stale.join().expect("render thread").is_err()
                && cancelled_at.elapsed() < Duration::from_millis(800),
            "the qwen request did not cancel promptly"
        );

        let replacement = home.join("replacement.wav");
        resident
            .render(Request {
                text: "replacement",
                out: &replacement,
                voice: "serena",
                lang: "chinese",
                speed: 1.0,
                params: "",
            })
            .expect("replacement render");
        assert!(replacement.exists(), "replacement produced no audio");
        assert_eq!(
            std::fs::read_to_string(&boots)
                .expect("boot log")
                .lines()
                .count(),
            1,
            "qwen reloaded its model after a cancelled request"
        );
    }

    /// Exercise MOSS's real worker protocol with a tiny stand-in model.
    ///
    /// This catches the two boundaries that matter without downloading weights:
    /// the embedded audiobook prompt must decode to the exact codec-token
    /// matrix the model expects, and the stereo float output must become an
    /// honest wav that readio can pace against.
    #[test]
    fn moss_worker_uses_its_embedded_voice_tokens_and_writes_stereo_wav() {
        let home = tempdir();
        let package = home.join("mlx_audio/tts");
        let model = home.join("configured-moss");
        let codec = home.join("configured-codec");
        std::fs::create_dir_all(&package).expect("fake package");
        std::fs::create_dir_all(&model).expect("fake local model");
        std::fs::create_dir_all(&codec).expect("fake local codec");
        std::fs::write(home.join("mlx_audio/__init__.py"), "").expect("package");
        std::fs::write(package.join("__init__.py"), "").expect("package");
        std::fs::write(
            package.join("utils.py"),
            format!(
                r#"import numpy as np

class Segment:
    def __init__(self):
        left = np.linspace(-0.25, 0.25, 480, dtype=np.float32)
        self.audio = np.column_stack((left, -left))
        self.sample_rate = 48000

class Model:
    def generate(self, text, *, prompt_audio_codes, mode, max_tokens, do_sample,
                 audio_temperature, audio_top_p, audio_top_k,
                 audio_repetition_penalty, audio_tokenizer_device,
                 audio_tokenizer_source):
        assert prompt_audio_codes.shape == (118, 16), prompt_audio_codes.shape
        assert prompt_audio_codes.dtype == np.int32, prompt_audio_codes.dtype
        assert int(prompt_audio_codes.min()) == 0
        assert int(prompt_audio_codes.max()) == 1023
        assert mode == "voice_clone"
        assert audio_tokenizer_source == {codec}
        if text != "。":
            assert audio_temperature == 0.7, audio_temperature
        yield Segment()

def load_model(path):
    assert path == {model}, path
    return Model()
"#,
                model = serde_json::to_string(&model.to_string_lossy()).expect("json model"),
                codec = serde_json::to_string(&codec.to_string_lossy()).expect("json codec"),
            ),
        )
        .expect("fake mlx-audio");

        let worker = worker_script(&home, "moss")
            .expect("write worker")
            .expect("moss worker");
        let Some(python) = which("python3").or_else(|| which("python")) else {
            return;
        };
        let resident = Resident::new(vec![
            python.to_string_lossy().into_owned(),
            worker.to_string_lossy().into_owned(),
            "--model".to_string(),
            model.to_string_lossy().into_owned(),
            "--codec".to_string(),
            codec.to_string_lossy().into_owned(),
        ]);

        resident.warm().expect("the fake model should warm");
        let wav = home.join("moss.wav");
        resident
            .render(Request {
                text: "夜色渐深。",
                out: &wav,
                voice: "audiobook",
                lang: "zh",
                speed: 1.0,
                params: "temperature=0.7",
            })
            .expect("one request should receive one successful reply");
        assert_eq!(
            crate::voice::wav::duration_ms(&wav).expect("valid wav"),
            10,
            "480 samples at 48 kHz is a 10 ms stereo clip"
        );
    }

    /// A remote model can be several gigabytes, and Hugging Face may need both
    /// the incoming file and its final cache entry while the download is in
    /// flight. Starting that download on a nearly full disk leaves a partial
    /// cache and can make the rest of the machine misbehave. The worker must
    /// reject it before `load_model` gets a chance to touch the network.
    #[test]
    fn qwen_worker_refuses_a_model_download_that_would_fill_disk() {
        let home = tempdir();
        let package = home.join("mlx_audio/tts");
        let hub = home.join("huggingface_hub");
        std::fs::create_dir_all(&package).expect("fake mlx package");
        std::fs::create_dir_all(&hub).expect("fake hub package");
        std::fs::write(home.join("mlx_audio/__init__.py"), "").expect("package");
        std::fs::write(package.join("__init__.py"), "").expect("package");
        std::fs::write(
            package.join("utils.py"),
            r#"def load_model(model):
    raise AssertionError("model loading must not start when disk space is insufficient")
"#,
        )
        .expect("fake mlx-audio");
        std::fs::write(
            hub.join("__init__.py"),
            r#"class File:
    def __init__(self, name, size):
        self.rfilename = name
        self.size = size

class Info:
    siblings = [File("model.safetensors", 3 * 1024**3)]

class HfApi:
    def model_info(self, model, files_metadata=False):
        assert model == "remote/qwen", model
        assert files_metadata is True
        return Info()

def try_to_load_from_cache(model, filename, cache_dir=None):
    return None
"#,
        )
        .expect("fake huggingface hub");
        std::fs::write(
            hub.join("constants.py"),
            format!(
                "HF_HUB_CACHE = {}\n",
                serde_json::to_string(&home.join("hub").to_string_lossy()).expect("json path")
            ),
        )
        .expect("fake hub constants");

        let worker = worker_script(&home, "qwen")
            .expect("write worker")
            .expect("qwen worker");
        let launcher = home.join("launch_full_disk.py");
        std::fs::write(
            &launcher,
            format!(
                r#"import collections
import runpy
import shutil
import sys

Disk = collections.namedtuple("Disk", "total used free")
shutil.disk_usage = lambda path: Disk(20 * 1024**3, 14 * 1024**3, 6 * 1024**3)
sys.path.insert(0, {home})
sys.argv = [{worker}, "--model", "remote/qwen"]
runpy.run_path({worker}, run_name="__main__")
"#,
                home = serde_json::to_string(&home.to_string_lossy()).expect("json home path"),
                worker =
                    serde_json::to_string(&worker.to_string_lossy()).expect("json worker path"),
            ),
        )
        .expect("launcher");

        let Some(python) = which("python3").or_else(|| which("python")) else {
            return;
        };
        let resident = Resident::new(vec![
            python.to_string_lossy().into_owned(),
            launcher.to_string_lossy().into_owned(),
        ]);
        let error = resident
            .warm()
            .expect_err("the download should be blocked before model loading");
        let message = format!("{error:#}");
        assert!(
            message.contains("disk space") && message.contains("remote/qwen"),
            "the reader needs an actionable reason, not a later download failure: {message}"
        );
        assert!(
            !message.contains("model loading must not start"),
            "the model loader ran before the space check: {message}"
        );
    }

    #[test]
    fn an_interpreter_is_only_reported_when_it_is_a_real_one() {
        let home = tempdir();
        let launcher = home.join("fake-tts");
        std::fs::write(&launcher, "#!/usr/bin/env python3\nprint(1)\n").expect("write");

        assert_eq!(
            interpreter_behind(launcher.to_str().expect("utf8")),
            None,
            "`env python3` names no particular environment, so it names none"
        );

        let python = home.join("python");
        std::fs::write(&python, "").expect("write");
        std::fs::write(
            &launcher,
            format!("#!{}\nprint(1)\n", python.to_string_lossy()),
        )
        .expect("write");
        assert_eq!(
            interpreter_behind(launcher.to_str().expect("utf8")),
            Some(python),
            "a launcher pointing into its own environment is exactly the one to use"
        );
    }

    #[test]
    fn a_worker_that_is_not_there_is_an_error_not_a_wait() {
        let resident = Resident::new(vec!["readio-no-such-worker".to_string()]);
        let err = resident.warm().expect_err("nothing to start");
        assert!(
            format!("{err}").contains("readio-no-such-worker"),
            "and it should name what it tried to run, got: {err}"
        );
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "readio-resident-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }
}
