//! Read-aloud: local speech engines, playback, and the clock that lets the
//! visible text follow the audio.
//!
//! readio does not bundle a model. It shells out to whatever engine you point
//! it at, which keeps the binary small and means a better model next month is a
//! config edit rather than a new release. See [`config`] for the file and
//! `docs/tts.md` for which small models are worth pointing it at.
//!
//! The pieces:
//!
//! - [`config::TtsConfig`] — `~/.readio/tts.toml`: engine, voice, rate.
//! - [`Synthesizer`] — turn text into an audio file, then play it. Implemented
//!   by [`command::CommandSynth`] for real engines and by a fake in tests.
//! - [`device`] — the output-device whitelist, so switching headphones cannot
//!   spill a sentence into the room.
//! - [`Speaker`] — a worker thread that synthesizes ahead, plays in order, and
//!   reports [`SpeechEvent`]s so the reader can highlight the sentence being
//!   spoken and pace the text to the audio.

pub mod command;
pub mod config;
pub mod device;
pub mod sentence;
pub mod wav;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread::JoinHandle;

use anyhow::Result;

pub use sentence::Utterance as Sentence;

/// A rendered audio clip.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub path: std::path::PathBuf,
    /// Duration in milliseconds, as reported by the audio file itself.
    pub ms: u64,
}

/// Something that can turn text into audio and play it.
pub trait Synthesizer: Send {
    /// Render `text` into `out`, returning the clip with its real duration.
    fn synthesize(&self, text: &str, out: &Path) -> Result<Clip>;

    /// Play a clip, blocking until it finishes or `cancel` is set.
    fn play(&self, clip: &Clip, cancel: &AtomicBool) -> Result<()>;

    /// Short human-readable name, shown in the status line.
    fn describe(&self) -> String;
}

/// What the speaker reports back to the app.
#[derive(Debug, Clone, PartialEq)]
pub enum SpeechEvent {
    /// Audio for a sentence started playing.
    Started {
        /// Sentence id, assigned by the caller.
        id: u64,
        /// Byte range of the sentence inside the passage being read.
        range: (usize, usize),
        /// Characters in the sentence, for pacing the text.
        chars: usize,
        /// Clip duration in milliseconds.
        ms: u64,
    },
    /// Audio for a sentence finished.
    Finished { id: u64 },
    /// Everything queued has been spoken.
    Idle,
    /// Synthesis or playback failed; the app falls back to timed reading.
    Failed { message: String },
}

/// A queued sentence.
struct Job {
    id: u64,
    text: String,
    range: (usize, usize),
}

enum Command {
    Speak(Job),
    Flush,
    Stop,
}

/// Background speech worker: synthesizes ahead of playback and reports progress.
pub struct Speaker {
    commands: Sender<Command>,
    events: Receiver<SpeechEvent>,
    cancel: Arc<AtomicBool>,
    /// Sentences queued but not yet picked up by the worker. Lets the worker
    /// tell "nothing left to say" from "the next clip is still coming" without
    /// consuming a command off the channel.
    queued: Arc<AtomicUsize>,
    worker: Option<JoinHandle<()>>,
    engine: String,
    /// Sentences handed over but not yet reported finished.
    pending: usize,
}

impl Speaker {
    /// Start a worker around `synth`, writing clips under `scratch`.
    pub fn spawn(synth: Box<dyn Synthesizer>, scratch: std::path::PathBuf) -> Self {
        let engine = synth.describe();
        let (commands, command_rx) = channel::<Command>();
        let (event_tx, events) = channel::<SpeechEvent>();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let queued = Arc::new(AtomicUsize::new(0));
        let worker_queued = Arc::clone(&queued);

        let worker = std::thread::Builder::new()
            .name("readio-tts".to_string())
            .spawn(move || {
                run(
                    synth,
                    scratch,
                    command_rx,
                    event_tx,
                    worker_cancel,
                    worker_queued,
                )
            })
            .ok();

        Self {
            commands,
            events,
            cancel,
            queued,
            worker,
            engine,
            pending: 0,
        }
    }

    pub fn engine(&self) -> &str {
        &self.engine
    }

    /// Queue a sentence. `range` is its position in the passage.
    pub fn speak(&mut self, id: u64, text: &str, range: (usize, usize)) {
        if text.trim().is_empty() {
            return;
        }
        self.pending += 1;
        self.queued.fetch_add(1, Ordering::SeqCst);
        let _ = self.commands.send(Command::Speak(Job {
            id,
            text: text.to_string(),
            range,
        }));
    }

    /// Drop everything queued and silence the current clip.
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        let _ = self.commands.send(Command::Flush);
        self.pending = 0;
        self.queued.store(0, Ordering::SeqCst);
        // Drain stale events so a later poll does not see the old run.
        while self.events.try_recv().is_ok() {}
        self.cancel.store(false, Ordering::SeqCst);
    }

    /// Non-blocking read of everything that happened since the last call.
    pub fn poll(&mut self) -> Vec<SpeechEvent> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(event) => {
                    if let SpeechEvent::Finished { .. } = event {
                        self.pending = self.pending.saturating_sub(1);
                    }
                    out.push(event);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        out
    }

    /// True while sentences are queued or playing.
    pub fn busy(&self) -> bool {
        self.pending > 0
    }
}

impl Drop for Speaker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        let _ = self.commands.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Worker loop: synthesize one clip ahead, play in order, report events.
fn run(
    synth: Box<dyn Synthesizer>,
    scratch: std::path::PathBuf,
    commands: Receiver<Command>,
    events: Sender<SpeechEvent>,
    cancel: Arc<AtomicBool>,
    queued: Arc<AtomicUsize>,
) {
    let _ = std::fs::create_dir_all(&scratch);
    let mut sequence = 0u64;

    while let Ok(command) = commands.recv() {
        let job = match command {
            Command::Speak(job) => {
                queued
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                        Some(n.saturating_sub(1))
                    })
                    .ok();
                job
            }
            Command::Flush => continue,
            Command::Stop => break,
        };
        if cancel.load(Ordering::SeqCst) {
            continue;
        }

        sequence += 1;
        let out = scratch.join(format!("utt-{sequence}.wav"));
        let clip = match synth.synthesize(&job.text, &out) {
            Ok(clip) => clip,
            Err(err) => {
                let _ = events.send(SpeechEvent::Failed {
                    message: format!("{err:#}"),
                });
                continue;
            }
        };
        if cancel.load(Ordering::SeqCst) {
            let _ = std::fs::remove_file(&clip.path);
            continue;
        }

        let _ = events.send(SpeechEvent::Started {
            id: job.id,
            range: job.range,
            chars: job.text.chars().count(),
            ms: clip.ms,
        });
        let played = synth.play(&clip, &cancel);
        let _ = std::fs::remove_file(&clip.path);

        match played {
            Ok(()) => {
                let _ = events.send(SpeechEvent::Finished { id: job.id });
            }
            Err(err) => {
                let _ = events.send(SpeechEvent::Finished { id: job.id });
                if !cancel.load(Ordering::SeqCst) {
                    let _ = events.send(SpeechEvent::Failed {
                        message: format!("{err:#}"),
                    });
                }
            }
        }

        // Nothing else waiting: tell the app it can resume timed reading.
        if queued.load(Ordering::SeqCst) == 0 {
            let _ = events.send(SpeechEvent::Idle);
        }
    }
}
