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
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};

use crate::i18n::tf;

/// The worker script, carried in the binary.
///
/// Shipping it as a file beside the binary would mean readio's installer has
/// somewhere to put it and every reader has the same somewhere. It does not:
/// readio is a single file people copy onto a path. So the script travels
/// inside it and is written out next to the config, where a curious reader can
/// read it and an unhappy one can edit it.
const WORKER: &str = include_str!("worker.py");

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
    /// `None` until the first sentence, and again after a failure, which is
    /// what makes the next sentence a retry rather than a second corpse.
    session: Mutex<Option<Session>>,
}

struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

/// One sentence, as the worker expects it.
pub struct Request<'a> {
    pub text: &'a str,
    pub out: &'a Path,
    pub voice: &'a str,
    pub lang: &'a str,
    pub speed: f32,
}

impl Resident {
    pub fn new(argv: Vec<String>) -> Self {
        Self {
            argv,
            session: Mutex::new(None),
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
        let mut guard = self.session.lock().expect("resident lock");
        if guard.is_none() {
            *guard = Some(self.start()?);
        }
        let session = guard.as_mut().expect("just started");

        let job = serde_json::json!({
            "text": request.text,
            "out": request.out.to_string_lossy(),
            "voice": request.voice,
            "lang": request.lang,
            "speed": request.speed,
        });

        // A write or read that fails here means the worker is gone — killed by
        // the system, or crashed on something the reply never got to describe.
        // Drop it, so the next sentence starts a new one.
        let outcome = Self::exchange(session, &job.to_string());
        if outcome.is_err() {
            *guard = None;
        }
        outcome
    }

    /// One request, one reply.
    ///
    /// The read blocks. By this point the worker has already loaded its model
    /// and answered once, so the failure it is exposed to is the worker dying
    /// mid-sentence — which closes the pipe and ends the read rather than
    /// hanging on it.
    fn exchange(session: &mut Session, job: &str) -> Result<()> {
        writeln!(session.stdin, "{job}")?;
        session.stdin.flush()?;

        let mut line = String::new();
        if session.stdout.read_line(&mut line)? == 0 {
            bail!(tf("tts.worker_gone", &[&""]));
        }
        let reply: serde_json::Value = serde_json::from_str(line.trim())?;
        if reply.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
            return Ok(());
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
            .ok_or_else(|| anyhow!(tf("tts.worker_no_command", &[&""])))?;

        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The worker's own diagnostics go to the terminal readio is drawing
            // on, which would corrupt the frame. Nothing it says on stderr is
            // load-bearing: what matters comes back as JSON.
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| anyhow!(tf("tts.worker_start", &[&program, &err])))?;

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
                bail!(tf("tts.worker_slow", &[&READY_TIMEOUT.as_secs()]));
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                bail!(tf("tts.worker_gone", &[&""]));
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

/// Write the worker script out, and return where it landed.
///
/// Rewritten whenever it differs from the one in this binary, so upgrading
/// readio upgrades the script, while an edit survives until then — which is the
/// same bargain the config file offers.
pub fn worker_script(dir: &Path) -> Result<PathBuf> {
    let path = dir.join("kokoro_worker.py");
    let current = std::fs::read_to_string(&path).ok();
    if current.as_deref() != Some(WORKER) {
        std::fs::create_dir_all(dir)?;
        // Written beside its destination and renamed into place: a script
        // half-written when the machine went down is a syntax error the reader
        // would have to diagnose from a Python traceback.
        let staging = dir.join(format!("kokoro_worker.py.{}", std::process::id()));
        std::fs::write(&staging, WORKER)?;
        std::fs::rename(&staging, &path)?;
    }
    Ok(path)
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
    if program.contains('/') {
        let path = PathBuf::from(program);
        return path.is_file().then_some(path);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_is_written_once_and_rewritten_when_it_changes() {
        let home = tempdir();
        let first = worker_script(&home).expect("write");
        assert!(first.exists(), "the worker script should land on disk");

        std::fs::write(&first, "# someone has been editing").expect("edit");
        let again = worker_script(&home).expect("rewrite");
        assert_eq!(
            std::fs::read_to_string(&again).expect("read"),
            WORKER,
            "a script that no longer matches the binary is replaced by it"
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
