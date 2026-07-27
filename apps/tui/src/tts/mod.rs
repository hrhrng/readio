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
//! - [`Speaker`] — two threads, one rendering and one playing, so the next
//!   sentence is already a wav file by the time the current one ends. It
//!   reports [`SpeechEvent`]s so the reader can highlight the sentence being
//!   spoken and pace the text to the audio.
//!
//! Why two threads: synthesis is fast (13× realtime for kokoro on an M4) but
//! not instant, and doing it between clips puts a hole of exactly that length
//! into every sentence boundary. The renderer runs up to `tts.prefetch` clips
//! ahead of the player, which is the same trick the web player uses with its
//! prefetch window.

pub mod command;
pub mod config;
pub mod device;
pub mod sentence;
pub mod wav;

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Condvar, Mutex, MutexGuard};
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
pub trait Synthesizer: Send + Sync {
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

/// The playback speeds `^r` steps through.
///
/// The same five the web player offers, so the two readio apps feel the same in
/// the hand; `/rate` still takes any value between 0.5 and 3.
pub const SPEEDS: [f32; 5] = [0.75, 1.0, 1.25, 1.5, 2.0];

/// The next speed up, wrapping round to the slowest.
///
/// Values off the ladder — typed into `/rate` or edited into the config — snap
/// to the next one above them, so the cycle behaves predictably from anywhere.
pub fn next_speed(current: f32) -> f32 {
    SPEEDS
        .iter()
        .copied()
        .find(|speed| *speed > current + 0.001)
        .unwrap_or(SPEEDS[0])
}

/// A speed as an audiobook app writes it: `1×`, `1.25×`, `0.75×`.
pub fn speed_label(speed: f32) -> String {
    let mut text = format!("{speed:.2}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    format!("{text}×")
}

/// A queued sentence.
struct Job {
    id: u64,
    text: String,
    range: (usize, usize),
    /// Which run of the speaker this belongs to. [`Speaker::stop`] bumps the
    /// era and both threads discard anything older, which is how a speed change
    /// throws away clips that were rendered at the old speed.
    era: u64,
}

/// A sentence already rendered to a wav, waiting its turn to be played.
struct Rendered {
    job: Job,
    clip: Clip,
}

enum Command {
    Speak(Job),
    Stop,
}

/// The buffer between the renderer and the player.
///
/// A bounded channel would be shorter to write but impossible to flush: the
/// renderer would sit blocked on a full channel holding a clip nobody wants any
/// more. With a deque behind a condvar, `stop()` can take the lock, delete the
/// stale wavs and wake both threads.
struct Pipeline {
    queue: Mutex<VecDeque<Rendered>>,
    /// A clip arrived, or the speaker is shutting down.
    ready: Condvar,
    /// A clip left the queue, or the era changed.
    room: Condvar,
    /// How many clips may wait ahead of the one playing: `tts.prefetch`.
    capacity: usize,
    era: AtomicU64,
    closed: AtomicBool,
}

impl Pipeline {
    fn new(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
            room: Condvar::new(),
            capacity,
            era: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        }
    }

    /// A poisoned lock here would mean a panicking speech thread; the queue is
    /// plain data, so carrying on with it is better than taking the app down.
    fn lock(&self) -> MutexGuard<'_, VecDeque<Rendered>> {
        self.queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn era(&self) -> u64 {
        self.era.load(Ordering::SeqCst)
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn is_stale(&self, era: u64) -> bool {
        self.era() != era
    }

    /// Throw away everything rendered but unplayed and invalidate what is still
    /// being rendered. Returns the new era.
    fn flush(&self) -> u64 {
        let era = self.era.fetch_add(1, Ordering::SeqCst) + 1;
        let mut queue = self.lock();
        for rendered in queue.drain(..) {
            let _ = std::fs::remove_file(&rendered.clip.path);
        }
        drop(queue);
        self.ready.notify_all();
        self.room.notify_all();
        era
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.ready.notify_all();
        self.room.notify_all();
    }

    /// Block until the player has room for one more clip. `false` means this job
    /// is no longer wanted.
    fn wait_for_room(&self, era: u64) -> bool {
        let mut queue = self.lock();
        while queue.len() >= self.capacity {
            if self.is_closed() || self.is_stale(era) {
                return false;
            }
            queue = self
                .room
                .wait(queue)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        !self.is_closed() && !self.is_stale(era)
    }

    /// Hand a finished clip to the player. `false` means it was dropped.
    fn push(&self, rendered: Rendered) -> bool {
        let mut queue = self.lock();
        if self.is_closed() || self.is_stale(rendered.job.era) {
            let _ = std::fs::remove_file(&rendered.clip.path);
            return false;
        }
        queue.push_back(rendered);
        drop(queue);
        self.ready.notify_one();
        true
    }

    /// The next clip to play, or `None` once the speaker is closed and drained.
    fn pop(&self) -> Option<Rendered> {
        let mut queue = self.lock();
        loop {
            if let Some(rendered) = queue.pop_front() {
                drop(queue);
                self.room.notify_one();
                return Some(rendered);
            }
            if self.is_closed() {
                return None;
            }
            queue = self
                .ready
                .wait(queue)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

/// Background speech: one thread rendering, one playing, `prefetch` clips of
/// slack between them.
pub struct Speaker {
    commands: Sender<Command>,
    events: Receiver<SpeechEvent>,
    cancel: Arc<AtomicBool>,
    pipeline: Arc<Pipeline>,
    /// Sentences accepted but not yet finished playing. Zero means the reader
    /// can go back to timed reading.
    inflight: Arc<AtomicUsize>,
    render: Option<JoinHandle<()>>,
    player: Option<JoinHandle<()>>,
    engine: String,
    /// Sentences handed over but not yet reported finished.
    pending: usize,
}

impl Speaker {
    /// Start the two threads. `prefetch` is how many sentences may be rendered
    /// ahead of the one being played.
    pub fn spawn(
        synth: Box<dyn Synthesizer>,
        scratch: std::path::PathBuf,
        prefetch: usize,
    ) -> Self {
        let synth: Arc<dyn Synthesizer> = Arc::from(synth);
        let engine = synth.describe();
        let (commands, command_rx) = channel::<Command>();
        let (event_tx, events) = channel::<SpeechEvent>();
        let cancel = Arc::new(AtomicBool::new(false));
        let pipeline = Arc::new(Pipeline::new(prefetch.max(1)));
        let inflight = Arc::new(AtomicUsize::new(0));

        let render = std::thread::Builder::new()
            .name("readio-tts-render".to_string())
            .spawn({
                let synth = Arc::clone(&synth);
                let pipeline = Arc::clone(&pipeline);
                let inflight = Arc::clone(&inflight);
                let events = event_tx.clone();
                move || render_loop(synth, scratch, command_rx, events, pipeline, inflight)
            })
            .ok();

        let player = std::thread::Builder::new()
            .name("readio-tts-play".to_string())
            .spawn({
                let pipeline = Arc::clone(&pipeline);
                let inflight = Arc::clone(&inflight);
                let cancel = Arc::clone(&cancel);
                move || play_loop(synth, event_tx, pipeline, inflight, cancel)
            })
            .ok();

        Self {
            commands,
            events,
            cancel,
            pipeline,
            inflight,
            render,
            player,
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
        self.inflight.fetch_add(1, Ordering::SeqCst);
        let _ = self.commands.send(Command::Speak(Job {
            id,
            text: text.to_string(),
            range,
            era: self.pipeline.era(),
        }));
    }

    /// Drop everything queued and silence the current clip.
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.pipeline.flush();
        self.pending = 0;
        self.inflight.store(0, Ordering::SeqCst);
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
        self.pipeline.close();
        let _ = self.commands.send(Command::Stop);
        if let Some(render) = self.render.take() {
            let _ = render.join();
        }
        if let Some(player) = self.player.take() {
            let _ = player.join();
        }
    }
}

/// One sentence is no longer coming: keep the in-flight count honest.
fn forget(inflight: &AtomicUsize) {
    inflight
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
            Some(n.saturating_sub(1))
        })
        .ok();
}

/// Render sentences into wavs, up to `prefetch` ahead of the player.
fn render_loop(
    synth: Arc<dyn Synthesizer>,
    scratch: std::path::PathBuf,
    commands: Receiver<Command>,
    events: Sender<SpeechEvent>,
    pipeline: Arc<Pipeline>,
    inflight: Arc<AtomicUsize>,
) {
    let _ = std::fs::create_dir_all(&scratch);
    let mut sequence = 0u64;

    while let Ok(command) = commands.recv() {
        let job = match command {
            Command::Speak(job) => job,
            Command::Stop => break,
        };
        if pipeline.is_stale(job.era) || !pipeline.wait_for_room(job.era) {
            forget(&inflight);
            continue;
        }

        sequence += 1;
        let out = scratch.join(format!("utt-{sequence}.wav"));
        match synth.synthesize(&job.text, &out) {
            Ok(clip) => {
                if !pipeline.push(Rendered { job, clip }) {
                    forget(&inflight);
                }
            }
            Err(err) => {
                let _ = events.send(SpeechEvent::Failed {
                    message: format!("{err:#}"),
                });
                forget(&inflight);
            }
        }
    }
    pipeline.close();
}

/// Play rendered clips in order and report where the audio is.
fn play_loop(
    synth: Arc<dyn Synthesizer>,
    events: Sender<SpeechEvent>,
    pipeline: Arc<Pipeline>,
    inflight: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
) {
    while let Some(Rendered { job, clip }) = pipeline.pop() {
        if pipeline.is_stale(job.era) {
            let _ = std::fs::remove_file(&clip.path);
            forget(&inflight);
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
        let _ = events.send(SpeechEvent::Finished { id: job.id });
        if let Err(err) = played
            && !cancel.load(Ordering::SeqCst)
        {
            let _ = events.send(SpeechEvent::Failed {
                message: format!("{err:#}"),
            });
        }

        forget(&inflight);
        // Nothing else waiting: tell the app it can resume timed reading.
        if inflight.load(Ordering::SeqCst) == 0 {
            let _ = events.send(SpeechEvent::Idle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn the_speed_ladder_cycles_and_wraps() {
        assert_eq!(next_speed(1.0), 1.25);
        assert_eq!(next_speed(1.25), 1.5);
        assert_eq!(next_speed(1.5), 2.0);
        assert_eq!(next_speed(2.0), 0.75, "the top wraps round to the bottom");
        assert_eq!(next_speed(0.75), 1.0);
    }

    #[test]
    fn a_speed_off_the_ladder_snaps_to_the_next_one_up() {
        assert_eq!(next_speed(0.5), 0.75);
        assert_eq!(next_speed(1.1), 1.25);
        assert_eq!(next_speed(2.7), 0.75, "past the top, wrap");
    }

    #[test]
    fn speeds_read_like_an_audiobook_app() {
        assert_eq!(speed_label(1.0), "1×");
        assert_eq!(speed_label(1.5), "1.5×");
        assert_eq!(speed_label(1.25), "1.25×");
        assert_eq!(speed_label(0.75), "0.75×");
        assert_eq!(speed_label(2.0), "2×");
    }

    /// Records when each synthesis and each playback began and ended, so a test
    /// can tell a pipeline from a queue.
    struct Fake {
        log: Arc<Mutex<Vec<(&'static str, Instant)>>>,
        synth: Duration,
        play: Duration,
    }

    impl Fake {
        fn note(&self, what: &'static str) {
            self.log
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push((what, Instant::now()));
        }
    }

    impl Synthesizer for Fake {
        fn synthesize(&self, _text: &str, out: &Path) -> Result<Clip> {
            self.note("render-start");
            std::thread::sleep(self.synth);
            std::fs::write(out, b"")?;
            self.note("render-end");
            Ok(Clip {
                path: out.to_path_buf(),
                ms: self.play.as_millis() as u64,
            })
        }

        fn play(&self, _clip: &Clip, _cancel: &AtomicBool) -> Result<()> {
            self.note("play-start");
            std::thread::sleep(self.play);
            self.note("play-end");
            Ok(())
        }

        fn describe(&self) -> String {
            "fake".to_string()
        }
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("readio-tts-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// Drain events until `wanted` sentences have finished, or time runs out.
    fn wait_for_finishes(speaker: &mut Speaker, wanted: usize) -> usize {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut done = 0;
        while done < wanted && Instant::now() < deadline {
            for event in speaker.poll() {
                if let SpeechEvent::Finished { .. } = event {
                    done += 1;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        done
    }

    /// The point of `tts.prefetch`: sentence two is rendered while sentence one
    /// is still playing. Before the pipeline existed, every sentence boundary
    /// held a gap exactly as long as synthesis took.
    #[test]
    fn the_next_sentence_is_rendered_while_this_one_plays() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let fake = Fake {
            log: Arc::clone(&log),
            synth: Duration::from_millis(60),
            play: Duration::from_millis(140),
        };
        let dir = scratch("overlap");
        let mut speaker = Speaker::spawn(Box::new(fake), dir.clone(), 2);
        for (n, text) in ["one.", "two.", "three."].iter().enumerate() {
            speaker.speak(n as u64, text, (0, text.len()));
        }
        assert_eq!(
            wait_for_finishes(&mut speaker, 3),
            3,
            "all three should play"
        );

        let log = log.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let at = |what: &str, nth: usize| {
            log.iter()
                .filter(|(kind, _)| *kind == what)
                .nth(nth)
                .map(|(_, when)| *when)
                .unwrap_or_else(|| panic!("no {what} #{nth} in {log:?}"))
        };
        assert!(
            at("render-end", 1) < at("play-end", 0),
            "sentence two must be ready before sentence one stops playing"
        );
        assert!(
            at("render-start", 1) < at("play-start", 0)
                || at("render-start", 1) < at("play-end", 0),
            "rendering must overlap playback, not follow it"
        );
        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Prefetch is a window, not a licence to render the whole chapter: a slow
    /// player must not have a hundred wavs piling up behind it.
    #[test]
    fn rendering_stays_within_the_prefetch_window() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let fake = Fake {
            log: Arc::clone(&log),
            synth: Duration::from_millis(10),
            play: Duration::from_millis(120),
        };
        let dir = scratch("window");
        let mut speaker = Speaker::spawn(Box::new(fake), dir.clone(), 2);
        for n in 0..6u64 {
            speaker.speak(n, "a sentence.", (0, 11));
        }
        // While the first clip is still playing, only the window may be ahead.
        std::thread::sleep(Duration::from_millis(90));
        let rendered = log
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|(kind, _)| *kind == "render-end")
            .count();
        assert!(
            rendered <= 3,
            "with prefetch 2, at most the playing clip plus two may be rendered, got {rendered}"
        );
        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A speed change throws away everything rendered at the old speed. The
    /// clips are gone and their files with them; the reader hears the new speed
    /// on the next sentence rather than after the queue drains.
    #[test]
    fn stopping_discards_clips_rendered_at_the_old_speed() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let fake = Fake {
            log: Arc::clone(&log),
            synth: Duration::from_millis(10),
            play: Duration::from_millis(200),
        };
        let dir = scratch("flush");
        let mut speaker = Speaker::spawn(Box::new(fake), dir.clone(), 4);
        for n in 0..5u64 {
            speaker.speak(n, "a sentence.", (0, 11));
        }
        std::thread::sleep(Duration::from_millis(80));
        speaker.stop();
        assert!(!speaker.busy(), "stop clears the queue");

        // Give the threads a moment, then check no wav was left behind.
        std::thread::sleep(Duration::from_millis(250));
        let leftovers = std::fs::read_dir(&dir)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or(0);
        assert_eq!(
            leftovers, 0,
            "discarded clips must not litter the scratch dir"
        );
        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
