//! Read-aloud: local speech engines, playback, and the clock that lets the
//! visible text follow the audio.
//!
//! readio does not bundle a model. It shells out to whatever engine you point
//! it at, which keeps the binary small and means a better model next month is a
//! config edit rather than a new release. See [`config`] for the presets and
//! the README's "Read-aloud" section for which small models are worth pointing
//! it at.
//!
//! The pieces:
//!
//! - [`config::TtsConfig`] — the `tts:` section of `~/.readio/config.yaml`:
//!   engine, voice, rate.
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
//! into every sentence boundary. The renderer runs up to `voice.prefetch` clips
//! ahead of the player, which is the same trick the web player uses with its
//! prefetch window.

pub mod command;
pub mod config;
pub mod device;
pub mod install;
mod output;
pub mod resident;
pub mod sentence;
pub mod wav;

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::Result;
use sha2::{Digest, Sha256};

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

    /// Get ready, before there is anything to say.
    ///
    /// An engine that keeps a model in memory has to load it at some point, and
    /// the worst point is the reader's first sentence. Called once by the
    /// render thread, so the loading happens while the book is being opened.
    fn warm(&self) -> Result<()> {
        Ok(())
    }

    /// Play a clip, blocking until it finishes or is cancelled.
    ///
    /// Pause is deliberately separate from cancellation. A pause keeps the
    /// player's audio cursor alive so resume continues at the same millisecond;
    /// cancellation throws the clip away for an interrupt or navigation.
    fn play(&self, clip: &Clip, control: &PlaybackControl) -> Result<()>;

    /// Stable description of everything that changes canonical synthesis.
    ///
    /// Playback controls are intentionally absent: one validated 1× WAV must
    /// survive pause, resume and every transport rate.
    fn cache_identity(&self) -> String {
        self.describe()
    }

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
    /// Synthesis or playback failed; read-aloud stops without changing modes.
    Failed { message: String },
}

/// The playback speeds `[` and `]` step through.
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

/// The next speed down, wrapping round to the fastest.
pub fn previous_speed(current: f32) -> f32 {
    SPEEDS
        .iter()
        .rev()
        .copied()
        .find(|speed| *speed < current - 0.001)
        .unwrap_or(*SPEEDS.last().expect("the speed ladder is not empty"))
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
    /// era and both threads discard anything older after navigation or stop.
    era: u64,
}

/// A sentence already rendered to a wav, waiting its turn to be played.
struct Rendered {
    job: Job,
    clip: Clip,
}

enum Command {
    Speak(Job),
    /// Render this sentence now and hold the clip until somebody asks for it.
    ///
    /// The one thing the pipeline cannot render ahead is the sentence it has
    /// not been given. Between two paragraphs readio spends a thinking line and
    /// a tool call before the next passage exists, and the engine spends
    /// another second after that — all of it in silence. This hands the opening
    /// sentence over early, while the last clip of the paragraph before it is
    /// still playing.
    Warm {
        text: String,
        era: u64,
    },
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
    /// How many clips may wait ahead of the one playing: `voice.prefetch`.
    capacity: usize,
    /// Live transport inputs for the rolling runway decision. Direct unit
    /// users that never render may leave this absent.
    transport: Option<TransportWindow>,
    era: AtomicU64,
    closed: AtomicBool,
}

struct TransportWindow {
    paused: Arc<AtomicBool>,
    rate: Arc<AtomicU32>,
    clock: Arc<PlaybackClock>,
}

/// Enough time to absorb ordinary sentence variance and one slower synthesis,
/// while the sentence cap and disk budget keep the controller bounded.
const PREFETCH_TARGET_WALL: Duration = Duration::from_secs(24);

impl Pipeline {
    fn new(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
            room: Condvar::new(),
            capacity,
            transport: None,
            era: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        }
    }

    fn with_transport(
        capacity: usize,
        paused: Arc<AtomicBool>,
        rate: Arc<AtomicU32>,
        clock: Arc<PlaybackClock>,
    ) -> Self {
        Self {
            transport: Some(TransportWindow {
                paused,
                rate,
                clock,
            }),
            ..Self::new(capacity)
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
        while self.window_full(&queue) {
            if self.is_closed() || self.is_stale(era) {
                return false;
            }
            let (next, _) = self
                .room
                .wait_timeout(queue, Duration::from_millis(100))
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            queue = next;
        }
        !self.is_closed() && !self.is_stale(era)
    }

    fn window_full(&self, queue: &VecDeque<Rendered>) -> bool {
        if queue.len() >= self.capacity {
            return true;
        }
        let Some(transport) = &self.transport else {
            return false;
        };
        // Pause freezes demand. A synthesis already in progress may still land
        // in `push`, but the controller does not start another one.
        if transport.paused.load(Ordering::SeqCst) {
            return true;
        }

        let queued_ms = queue
            .iter()
            .map(|rendered| rendered.clip.ms)
            .fold(0u64, u64::saturating_add);
        let active_ms = transport
            .clock
            .position()
            .filter(|position| !position.finished)
            .map(|position| {
                position
                    .duration
                    .saturating_sub(position.elapsed)
                    .as_millis()
                    .min(u64::MAX as u128) as u64
            })
            .unwrap_or(0);
        let source = Duration::from_millis(queued_ms.saturating_add(active_ms));
        let wall = source.div_f32(playback_rate(&transport.rate));
        wall >= PREFETCH_TARGET_WALL
    }

    fn transport_changed(&self) {
        self.room.notify_all();
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

/// Live controls for one clip.
///
/// Cancellation is tied to the queue era instead of a boolean that has to be
/// reset. Once `stop()` advances the era, an old player can never miss a brief
/// true/false pulse and carry on speaking. Pause is shared across eras because
/// it preserves, rather than invalidates, the clip currently on the air.
pub struct PlaybackControl {
    pipeline: Arc<Pipeline>,
    era: u64,
    paused: Arc<AtomicBool>,
    rate: Arc<AtomicU32>,
    clock: Arc<PlaybackClock>,
    events: Sender<SpeechEvent>,
    id: u64,
    range: (usize, usize),
    chars: usize,
    ms: u64,
}

impl PlaybackControl {
    pub fn cancelled(&self) -> bool {
        self.pipeline.is_closed() || self.pipeline.is_stale(self.era)
    }

    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Playback tempo, independent of the audio the model rendered.
    pub fn rate(&self) -> f32 {
        playback_rate(&self.rate)
    }

    /// Commit the only start that matters: the player has accepted the clip
    /// and its presentation timestamp is now advancing.
    pub fn started(&self) {
        if self.clock.start(
            self.id,
            self.range,
            self.ms,
            self.era,
            self.paused(),
            self.rate(),
        ) {
            let _ = self.events.send(SpeechEvent::Started {
                id: self.id,
                range: self.range,
                chars: self.chars,
                ms: self.ms,
            });
        }
    }

    /// Start a callback-driven clip whose position is measured in PCM frames.
    pub fn started_frames(&self, total_frames: u64, sample_rate: u32) {
        if self.clock.start_frames(
            self.id,
            self.range,
            total_frames,
            sample_rate,
            self.era,
            self.paused(),
        ) {
            let duration = frames_duration(total_frames, sample_rate);
            let _ = self.events.send(SpeechEvent::Started {
                id: self.id,
                range: self.range,
                chars: self.chars,
                ms: duration.as_millis().min(u64::MAX as u128) as u64,
            });
        }
    }

    /// Publish an absolute audio-callback-boundary frame position.
    pub fn presented_frames(&self, frames: u64) {
        self.clock.present_frames(self.era, frames);
    }

    /// The player has actually stopped advancing its audio pointer.
    pub fn pause_applied(&self) {
        self.clock.pause(self.era);
    }

    /// The player has actually resumed its audio pointer.
    pub fn resume_applied(&self) {
        self.clock.resume(self.era);
    }

    fn finished(&self) {
        self.clock.finish(self.era);
    }
}

/// One sample from the global read-aloud presentation timeline.
///
/// Every visual consumer receives this same position. `elapsed` advances only
/// while the player says it is advancing; synthesis, process startup and pause
/// are deliberately absent from the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackPosition {
    pub id: u64,
    pub range: (usize, usize),
    pub elapsed: Duration,
    pub duration: Duration,
    pub paused: bool,
    pub finished: bool,
}

struct ClockClip {
    id: u64,
    range: (usize, usize),
    era: u64,
    source: ClockSource,
    duration: Duration,
    finished: bool,
}

enum ClockSource {
    /// Compatibility path for a configured OS player command, which cannot
    /// report frames and therefore advances from a monotonic start instant.
    Estimated {
        elapsed: Duration,
        running_since: Option<Instant>,
        rate: f32,
    },
    /// An embedded audio callback is authoritative. Wall time never advances
    /// this source; only an absolute presented-frame report can move it.
    Frames {
        elapsed: Duration,
        sample_rate: u32,
        paused: bool,
    },
}

impl ClockClip {
    fn elapsed(&self) -> Duration {
        match self.source {
            ClockSource::Estimated {
                elapsed,
                running_since,
                rate,
            } => running_since
                .map(|started| elapsed + started.elapsed().mul_f32(rate))
                .unwrap_or(elapsed),
            ClockSource::Frames { elapsed, .. } => elapsed,
        }
        .min(self.duration)
    }

    fn paused(&self) -> bool {
        match self.source {
            ClockSource::Estimated { running_since, .. } => running_since.is_none(),
            ClockSource::Frames { paused, .. } => paused,
        }
    }

    fn advancing(&self) -> bool {
        !self.finished && !self.paused()
    }
}

#[derive(Default)]
struct PlaybackClock {
    clip: Mutex<Option<ClockClip>>,
    changed: Condvar,
}

impl PlaybackClock {
    fn lock(&self) -> MutexGuard<'_, Option<ClockClip>> {
        self.clip
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn start(
        &self,
        id: u64,
        range: (usize, usize),
        ms: u64,
        era: u64,
        paused: bool,
        rate: f32,
    ) -> bool {
        let mut slot = self.lock();
        if slot
            .as_ref()
            .is_some_and(|clip| clip.era == era && !clip.finished)
        {
            return false;
        }
        *slot = Some(ClockClip {
            id,
            range,
            era,
            source: ClockSource::Estimated {
                elapsed: Duration::ZERO,
                running_since: (!paused).then(Instant::now),
                rate,
            },
            duration: Duration::from_millis(ms),
            finished: false,
        });
        drop(slot);
        self.changed.notify_all();
        true
    }

    fn start_frames(
        &self,
        id: u64,
        range: (usize, usize),
        total_frames: u64,
        sample_rate: u32,
        era: u64,
        paused: bool,
    ) -> bool {
        let mut slot = self.lock();
        if sample_rate == 0
            || slot
                .as_ref()
                .is_some_and(|clip| clip.era == era && !clip.finished)
        {
            return false;
        }
        *slot = Some(ClockClip {
            id,
            range,
            era,
            source: ClockSource::Frames {
                elapsed: Duration::ZERO,
                sample_rate,
                paused,
            },
            duration: frames_duration(total_frames, sample_rate),
            finished: false,
        });
        drop(slot);
        self.changed.notify_all();
        true
    }

    fn present_frames(&self, era: u64, frames: u64) {
        let mut slot = self.lock();
        let Some(clip) = slot.as_mut().filter(|clip| clip.era == era) else {
            return;
        };
        let ClockSource::Frames {
            elapsed,
            sample_rate,
            ..
        } = &mut clip.source
        else {
            return;
        };
        *elapsed = frames_duration(frames, *sample_rate).min(clip.duration);
    }

    fn pause(&self, era: u64) {
        let mut slot = self.lock();
        let Some(clip) = slot.as_mut().filter(|clip| clip.era == era) else {
            return;
        };
        match &mut clip.source {
            ClockSource::Estimated {
                elapsed,
                running_since,
                rate,
            } => {
                if let Some(started) = running_since.take() {
                    *elapsed = (*elapsed + started.elapsed().mul_f32(*rate)).min(clip.duration);
                }
            }
            ClockSource::Frames { paused, .. } => *paused = true,
        }
        drop(slot);
        self.changed.notify_all();
    }

    fn resume(&self, era: u64) {
        let mut slot = self.lock();
        let Some(clip) = slot.as_mut().filter(|clip| clip.era == era) else {
            return;
        };
        if !clip.finished {
            match &mut clip.source {
                ClockSource::Estimated { running_since, .. } => {
                    if running_since.is_none() {
                        *running_since = Some(Instant::now());
                    }
                }
                ClockSource::Frames { paused, .. } => *paused = false,
            }
        }
        drop(slot);
        self.changed.notify_all();
    }

    fn set_rate(&self, rate: f32) {
        let mut slot = self.lock();
        let Some(clip) = slot.as_mut() else {
            return;
        };
        if let ClockSource::Estimated {
            elapsed,
            running_since,
            rate: current,
        } = &mut clip.source
        {
            if let Some(started) = running_since.as_mut() {
                *elapsed = (*elapsed + started.elapsed().mul_f32(*current)).min(clip.duration);
                *started = Instant::now();
            }
            *current = rate;
        }
        drop(slot);
        self.changed.notify_all();
    }

    fn finish(&self, era: u64) {
        let mut slot = self.lock();
        let Some(clip) = slot.as_mut().filter(|clip| clip.era == era) else {
            return;
        };
        match &mut clip.source {
            ClockSource::Estimated {
                elapsed,
                running_since,
                ..
            } => {
                *elapsed = clip.duration;
                *running_since = None;
            }
            ClockSource::Frames {
                elapsed, paused, ..
            } => {
                *elapsed = clip.duration;
                *paused = false;
            }
        }
        clip.finished = true;
        drop(slot);
        self.changed.notify_all();
    }

    fn clear(&self) {
        *self.lock() = None;
        self.changed.notify_all();
    }

    fn position(&self) -> Option<PlaybackPosition> {
        let slot = self.lock();
        let clip = slot.as_ref()?;
        Some(PlaybackPosition {
            id: clip.id,
            range: clip.range,
            elapsed: clip.elapsed(),
            duration: clip.duration,
            paused: clip.paused() && !clip.finished,
            finished: clip.finished,
        })
    }

    fn wait_until_paused(&self) {
        let mut slot = self.lock();
        while slot.as_ref().is_some_and(ClockClip::advancing) {
            slot = self
                .changed
                .wait(slot)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn wait_until_running(&self) {
        let mut slot = self.lock();
        while slot
            .as_ref()
            .is_some_and(|clip| clip.paused() && !clip.finished)
        {
            slot = self
                .changed
                .wait(slot)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

fn frames_duration(frames: u64, sample_rate: u32) -> Duration {
    if sample_rate == 0 {
        return Duration::ZERO;
    }
    let nanos = (frames as u128 * 1_000_000_000u128) / sample_rate as u128;
    Duration::from_nanos(nanos.min(u64::MAX as u128) as u64)
}

fn playback_rate(rate: &AtomicU32) -> f32 {
    f32::from_bits(rate.load(Ordering::Relaxed)).clamp(0.5, 3.0)
}

/// Background speech: one thread rendering, one playing, `prefetch` clips of
/// slack between them.
pub struct Speaker {
    commands: Sender<Command>,
    events: Receiver<SpeechEvent>,
    paused: Arc<AtomicBool>,
    rate: Arc<AtomicU32>,
    clock: Arc<PlaybackClock>,
    pipeline: Arc<Pipeline>,
    /// Sentences accepted but not yet finished playing. Zero means the reader
    /// can go back to timed reading.
    inflight: Arc<AtomicUsize>,
    render: Option<JoinHandle<()>>,
    player: Option<JoinHandle<()>>,
    engine: String,
    /// Sentences handed over but not yet reported finished.
    pending: usize,
    /// Whether a clip is on the air right now, as opposed to queued behind one.
    sounding: bool,
}

impl Speaker {
    /// Start the two threads. `prefetch` is the sentence hard cap around the
    /// playback-aware wall-clock runway target.
    pub fn spawn(
        synth: Box<dyn Synthesizer>,
        scratch: std::path::PathBuf,
        prefetch: usize,
    ) -> Self {
        let cache = scratch.join("cache-v1");
        Self::spawn_cached(synth, scratch, cache, prefetch)
    }

    /// Start a speaker with scratch and reusable audio in separate roots.
    pub fn spawn_cached(
        synth: Box<dyn Synthesizer>,
        scratch: std::path::PathBuf,
        cache: std::path::PathBuf,
        prefetch: usize,
    ) -> Self {
        let synth: Arc<dyn Synthesizer> = Arc::from(synth);
        let engine = synth.describe();
        let (commands, command_rx) = channel::<Command>();
        let (event_tx, events) = channel::<SpeechEvent>();
        let paused = Arc::new(AtomicBool::new(false));
        let rate = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        let clock = Arc::new(PlaybackClock::default());
        let pipeline = Arc::new(Pipeline::with_transport(
            prefetch.max(1),
            Arc::clone(&paused),
            Arc::clone(&rate),
            Arc::clone(&clock),
        ));
        let inflight = Arc::new(AtomicUsize::new(0));

        let render = std::thread::Builder::new()
            .name("readio-tts-render".to_string())
            .spawn({
                let synth = Arc::clone(&synth);
                let pipeline = Arc::clone(&pipeline);
                let inflight = Arc::clone(&inflight);
                let events = event_tx.clone();
                move || {
                    render_loop(
                        synth, scratch, cache, command_rx, events, pipeline, inflight,
                    )
                }
            })
            .ok();

        let player = std::thread::Builder::new()
            .name("readio-tts-play".to_string())
            .spawn({
                let pipeline = Arc::clone(&pipeline);
                let inflight = Arc::clone(&inflight);
                let paused = Arc::clone(&paused);
                let rate = Arc::clone(&rate);
                let clock = Arc::clone(&clock);
                move || play_loop(synth, event_tx, pipeline, inflight, paused, rate, clock)
            })
            .ok();

        Self {
            commands,
            events,
            paused,
            rate,
            clock,
            pipeline,
            inflight,
            render,
            player,
            engine,
            pending: 0,
            sounding: false,
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

    /// Render a sentence ahead of being asked to say it.
    ///
    /// For the paragraph boundary, where the pipeline has nothing to work on:
    /// the clip is rendered into a slot of its own and handed over the instant
    /// the same sentence is spoken for real. It costs one render either way, so
    /// a wrong guess costs nothing but the work — and it does not eat into
    /// `voice.prefetch`, which is about the sentences of a passage already begun.
    pub fn prerender(&self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        let _ = self.commands.send(Command::Warm {
            text: text.to_string(),
            era: self.pipeline.era(),
        });
    }

    /// Freeze the current clip at its audio cursor, leaving the queue intact.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        self.clock.wait_until_paused();
        self.pipeline.transport_changed();
    }

    /// Continue the same clip from the audio cursor held by [`Speaker::pause`].
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.clock.wait_until_running();
        self.pipeline.transport_changed();
    }

    /// Change tempo at the live playback cursor. Rendered and prefetched clips
    /// remain valid because model synthesis is always canonical 1× audio.
    pub fn set_rate(&self, rate: f32) {
        let rate = rate.clamp(0.5, 3.0);
        self.rate.store(rate.to_bits(), Ordering::Relaxed);
        self.clock.set_rate(rate);
        self.pipeline.transport_changed();
    }

    /// The single presentation timestamp sampled by text and highlighting.
    pub fn position(&self) -> Option<PlaybackPosition> {
        self.clock.position()
    }

    /// Drop everything queued and silence the current clip.
    pub fn stop(&mut self) {
        self.pipeline.flush();
        self.paused.store(false, Ordering::SeqCst);
        self.clock.clear();
        self.pending = 0;
        self.sounding = false;
        self.inflight.store(0, Ordering::SeqCst);
        // Drain stale events so a later poll does not see the old run.
        while self.events.try_recv().is_ok() {}
    }

    /// Non-blocking read of everything that happened since the last call.
    pub fn poll(&mut self) -> Vec<SpeechEvent> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(event) => {
                    match event {
                        SpeechEvent::Started { .. } => self.sounding = true,
                        SpeechEvent::Finished { .. } => {
                            self.pending = self.pending.saturating_sub(1);
                            self.sounding = false;
                        }
                        _ => {}
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

    /// Sentences handed over that have not finished playing, the current one
    /// included. This is the voice's runway: while it is more than one, there
    /// is a whole clip still to come after this one, and the reader can be sent
    /// looking for the next passage without any risk of overtaking the sound.
    pub fn runway(&self) -> usize {
        self.pending
    }

    /// Whether the reader can hear something this instant.
    ///
    /// Narrower than [`Speaker::runway`] on purpose: a sentence handed over is
    /// runway from the moment it is accepted, but it is not *sound* until the
    /// engine has finished rendering it. The difference between the two is
    /// exactly the dead air prefetch exists to remove, so this is what the
    /// pacing tests count.
    pub fn sounding(&self) -> bool {
        self.sounding && !self.paused.load(Ordering::SeqCst)
    }
}

impl Drop for Speaker {
    fn drop(&mut self) {
        self.pipeline.flush();
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
    cache_dir: std::path::PathBuf,
    commands: Receiver<Command>,
    events: Sender<SpeechEvent>,
    pipeline: Arc<Pipeline>,
    inflight: Arc<AtomicUsize>,
) {
    let _ = std::fs::create_dir_all(&scratch);
    let cache = SpeechCache::new(cache_dir, synth.cache_identity());
    let mut sequence = 0u64;
    // One sentence rendered before it was asked for, and the text it says. One
    // slot, because there is only ever one next paragraph.
    let mut warm: Option<(String, Clip)> = None;

    // Before the first sentence, not with it: an engine that keeps a model in
    // memory takes a few seconds to load one, and this thread exists precisely
    // so that the wait happens somewhere the reader is not.
    if let Err(err) = synth.warm() {
        let _ = events.send(SpeechEvent::Failed {
            message: format!("{err:#}"),
        });
        pipeline.close();
        return;
    }

    while let Ok(command) = commands.recv() {
        let job = match command {
            Command::Speak(job) => job,
            Command::Warm { text, era } => {
                if pipeline.is_stale(era) || !pipeline.wait_for_room(era) {
                    continue;
                }
                if warm.as_ref().is_none_or(|(had, _)| *had != text) {
                    sequence += 1;
                    let out = scratch.join(format!("warm-{sequence}.wav"));
                    // A failed warm-up is a slower boundary, not an error: if
                    // the engine is really gone, the sentence itself will say
                    // so a moment later, in front of the reader.
                    if let Ok(clip) = cache.render(synth.as_ref(), &text, &out)
                        && let Some((_, stale)) = warm.replace((text, clip))
                    {
                        let _ = std::fs::remove_file(&stale.path);
                    }
                }
                continue;
            }
            Command::Stop => break,
        };
        if pipeline.is_stale(job.era) || !pipeline.wait_for_room(job.era) {
            forget(&inflight);
            continue;
        }

        // Rendered a moment ago, under the paragraph before this one.
        let rendered = match warm.take_if(|(had, _)| *had == job.text) {
            Some((_, clip)) => Ok(clip),
            None => {
                sequence += 1;
                cache.render(
                    synth.as_ref(),
                    &job.text,
                    &scratch.join(format!("utt-{sequence}.wav")),
                )
            }
        };
        match rendered {
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
    if let Some((_, clip)) = warm.take() {
        let _ = std::fs::remove_file(&clip.path);
    }
    pipeline.close();
}

/// Content-addressed canonical WAVs. Playback always receives a disposable
/// scratch copy, so stopping or finishing a clip cannot evict the durable copy.
struct SpeechCache {
    dir: std::path::PathBuf,
    identity: String,
    recency: Mutex<HashMap<std::path::PathBuf, u64>>,
    sequence: AtomicU64,
}

const SPEECH_CACHE_MAX_ENTRIES: usize = 128;
const SPEECH_CACHE_MAX_BYTES: u64 = 256 * 1024 * 1024;
static SPEECH_CACHE_TMP: AtomicU64 = AtomicU64::new(0);

impl SpeechCache {
    fn new(dir: std::path::PathBuf, identity: String) -> Self {
        Self {
            dir,
            identity,
            recency: Mutex::new(HashMap::new()),
            sequence: AtomicU64::new(0),
        }
    }

    fn path(&self, text: &str) -> std::path::PathBuf {
        let mut digest = Sha256::new();
        digest.update(b"readio-speech-v1\0");
        digest.update(self.identity.as_bytes());
        digest.update(b"\0");
        digest.update(text.as_bytes());
        self.dir.join(format!("{:x}.wav", digest.finalize()))
    }

    fn render(&self, synth: &dyn Synthesizer, text: &str, out: &Path) -> Result<Clip> {
        let cached = self.path(text);
        if let Ok(ms) = wav::duration_ms(&cached) {
            self.touch(&cached);
            std::fs::copy(&cached, out)?;
            return Ok(Clip {
                path: out.to_path_buf(),
                ms,
            });
        }
        let _ = std::fs::remove_file(&cached);

        let rendered = synth.synthesize(text, out)?;
        // Trust neither an engine's return value nor the mere existence of its
        // output. Only a WAV the same parser used by playback accepts is cached.
        let ms = wav::duration_ms(&rendered.path)?;
        std::fs::create_dir_all(&self.dir)?;
        let tmp = cached.with_extension(format!(
            "{}.{}.tmp",
            std::process::id(),
            SPEECH_CACHE_TMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::copy(&rendered.path, &tmp)?;
        if let Err(err) = std::fs::rename(&tmp, &cached) {
            let _ = std::fs::remove_file(&tmp);
            // Another reader may have won the same content-addressed entry.
            if !cached.exists() {
                return Err(err.into());
            }
        }
        self.touch(&cached);
        let _ = self.prune();
        Ok(Clip {
            path: rendered.path,
            ms,
        })
    }

    fn touch(&self, path: &Path) {
        let tick = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        self.recency
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(path.to_path_buf(), tick);
    }

    fn prune(&self) -> std::io::Result<()> {
        let remembered = self
            .recency
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let mut entries = std::fs::read_dir(&self.dir)?
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().is_none_or(|ext| ext != "wav") {
                    return None;
                }
                let metadata = entry.metadata().ok()?;
                let disk_age = metadata
                    .modified()
                    .ok()
                    .and_then(|when| when.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|age| age.as_millis().min(u64::MAX as u128) as u64)
                    .unwrap_or(0);
                Some((
                    remembered
                        .get(&path)
                        .copied()
                        .map_or((0, disk_age), |tick| (1, tick)),
                    metadata.len(),
                    path,
                ))
            })
            .collect::<Vec<_>>();
        let mut bytes = entries
            .iter()
            .map(|(_, len, _)| *len)
            .fold(0u64, u64::saturating_add);
        entries.sort_by_key(|(age, _, _)| *age);

        let mut count = entries.len();
        for (_, len, path) in entries {
            if count <= SPEECH_CACHE_MAX_ENTRIES && bytes <= SPEECH_CACHE_MAX_BYTES {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                count = count.saturating_sub(1);
                bytes = bytes.saturating_sub(len);
                self.recency
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .remove(&path);
            }
        }
        Ok(())
    }
}

/// Play rendered clips in order and report where the audio is.
fn play_loop(
    synth: Arc<dyn Synthesizer>,
    events: Sender<SpeechEvent>,
    pipeline: Arc<Pipeline>,
    inflight: Arc<AtomicUsize>,
    paused: Arc<AtomicBool>,
    rate: Arc<AtomicU32>,
    clock: Arc<PlaybackClock>,
) {
    while let Some(Rendered { job, clip }) = pipeline.pop() {
        if pipeline.is_stale(job.era) {
            let _ = std::fs::remove_file(&clip.path);
            forget(&inflight);
            continue;
        }

        let control = PlaybackControl {
            pipeline: Arc::clone(&pipeline),
            era: job.era,
            paused: Arc::clone(&paused),
            rate: Arc::clone(&rate),
            clock: Arc::clone(&clock),
            events: events.clone(),
            id: job.id,
            range: job.range,
            chars: job.text.chars().count(),
            ms: clip.ms,
        };
        let played = synth.play(&clip, &control);
        control.finished();
        let _ = std::fs::remove_file(&clip.path);
        let _ = events.send(SpeechEvent::Finished { id: job.id });
        if let Err(err) = played
            && !control.cancelled()
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
    use std::path::PathBuf;
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
    fn the_speed_ladder_steps_down_and_wraps() {
        assert_eq!(previous_speed(1.25), 1.0);
        assert_eq!(previous_speed(1.0), 0.75);
        assert_eq!(previous_speed(0.75), 2.0);
        assert_eq!(previous_speed(1.1), 1.0);
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
        start: Duration,
        synth: Duration,
        play: Duration,
        control_tick: Duration,
        /// While this stays true, `play` does not return. Parking the player
        /// inside a clip lets a test reason about the pipeline's shape without
        /// depending on how fast the machine happens to be.
        hold: Option<Arc<AtomicBool>>,
    }

    impl Fake {
        fn new(log: &Arc<Mutex<Vec<(&'static str, Instant)>>>, synth: u64, play: u64) -> Self {
            Self {
                log: Arc::clone(log),
                start: Duration::ZERO,
                synth: Duration::from_millis(synth),
                play: Duration::from_millis(play),
                control_tick: Duration::from_millis(2),
                hold: None,
            }
        }

        fn starting_after(mut self, ms: u64) -> Self {
            self.start = Duration::from_millis(ms);
            self
        }

        fn checking_controls_every(mut self, ms: u64) -> Self {
            self.control_tick = Duration::from_millis(ms);
            self
        }

        fn parked(mut self, hold: &Arc<AtomicBool>) -> Self {
            self.hold = Some(Arc::clone(hold));
            self
        }

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
            std::fs::write(out, wav::silence(self.play.as_millis() as u64, 24_000))?;
            self.note("render-end");
            Ok(Clip {
                path: out.to_path_buf(),
                ms: self.play.as_millis() as u64,
            })
        }

        fn play(&self, _clip: &Clip, control: &PlaybackControl) -> Result<()> {
            std::thread::sleep(self.start);
            self.note("play-start");
            control.started();
            let mut remaining = self.play;
            let mut sampled = Instant::now();
            let mut paused = control.paused();
            while !remaining.is_zero() && !control.cancelled() {
                if control.paused() != paused {
                    paused = control.paused();
                    if paused {
                        control.pause_applied();
                    } else {
                        control.resume_applied();
                    }
                }
                let now = Instant::now();
                if !paused {
                    remaining = remaining.saturating_sub(now.saturating_duration_since(sampled));
                }
                sampled = now;
                std::thread::sleep(self.control_tick);
            }
            if let Some(hold) = &self.hold {
                // The deadline is a safety net, not part of the contract: a bug
                // should fail an assertion rather than hang the suite, and it is
                // short because a failing run has to wait it out once per clip.
                let deadline = Instant::now() + Duration::from_secs(2);
                while hold.load(Ordering::SeqCst)
                    && !control.cancelled()
                    && Instant::now() < deadline
                {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            self.note("play-end");
            Ok(())
        }

        fn describe(&self) -> String {
            "fake".to_string()
        }
    }

    type Log = Arc<Mutex<Vec<(&'static str, Instant)>>>;

    /// How many events of one kind have been logged.
    fn count(log: &Log, what: &str) -> usize {
        log.lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|(kind, _)| *kind == what)
            .count()
    }

    /// Wait until at least `wanted` events of one kind are logged, then report
    /// the count. Gives up on a deadline so a stall shows up as a failed
    /// assertion instead of a hung test.
    fn count_at_least(log: &Log, what: &str, wanted: usize) -> usize {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let seen = count(log, what);
            if seen >= wanted || Instant::now() >= deadline {
                return seen;
            }
            std::thread::sleep(Duration::from_millis(2));
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

    /// The point of `voice.prefetch`: sentence two is rendered while sentence one
    /// is still playing. Before the pipeline existed, every sentence boundary
    /// held a gap exactly as long as synthesis took.
    #[test]
    fn the_next_sentence_is_rendered_while_this_one_plays() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let hold = Arc::new(AtomicBool::new(true));
        let fake = Fake::new(&log, 20, 1).parked(&hold);
        let dir = scratch("overlap");
        let mut speaker = Speaker::spawn(Box::new(fake), dir.clone(), 2);
        for (n, text) in ["one.", "two.", "three."].iter().enumerate() {
            speaker.speak(n as u64, text, (0, text.len()));
        }

        assert_eq!(
            count_at_least(&log, "play-start", 1),
            1,
            "sentence one should start playing"
        );
        assert_eq!(
            count_at_least(&log, "render-end", 2),
            2,
            "sentence two should render while sentence one is held open"
        );
        assert_eq!(
            count(&log, "play-end"),
            0,
            "rendering sentence two waited for sentence one to finish"
        );

        hold.store(false, Ordering::SeqCst);
        assert_eq!(
            wait_for_finishes(&mut speaker, 3),
            3,
            "all three should play"
        );
        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A sentence rendered before it was asked for is not rendered again.
    ///
    /// This is what covers the boundary between two paragraphs: the opening
    /// sentence of the next one is handed over while the last clip of this one
    /// is still playing, and when the app finally asks for it in earnest the
    /// clip is already sitting there. The engine's work moves; nothing else
    /// does.
    #[test]
    fn a_sentence_rendered_ahead_is_not_rendered_twice() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let dir = scratch("warm");
        let mut speaker = Speaker::spawn(Box::new(Fake::new(&log, 60, 20)), dir.clone(), 2);

        speaker.prerender("one.");
        assert_eq!(
            count_at_least(&log, "render-end", 1),
            1,
            "the warm-up should have rendered the sentence on its own"
        );

        speaker.speak(0, "one.", (0, 4));
        assert_eq!(
            wait_for_finishes(&mut speaker, 1),
            1,
            "it should have played"
        );
        assert_eq!(
            count(&log, "render-start"),
            1,
            "the sentence was rendered a second time: the clip waiting for it went unused"
        );

        // And the slot answers for its own text only, or a warm-up would put
        // the wrong words in the reader's ear.
        speaker.speak(1, "two.", (4, 8));
        assert_eq!(wait_for_finishes(&mut speaker, 1), 1, "the second sentence");
        assert_eq!(
            count(&log, "render-start"),
            2,
            "a different sentence has to be rendered"
        );

        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Playback may consume a scratch copy, but a validated canonical WAV is
    /// retained for the next occurrence. Playback rate is deliberately changed
    /// between requests: including it in the cache key would couple transport
    /// state back into synthesis and make this render twice.
    #[test]
    fn a_validated_clip_is_reused_across_playback_rates() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let dir = scratch("cache-rate");
        let mut speaker = Speaker::spawn(Box::new(Fake::new(&log, 20, 40)), dir.clone(), 2);

        speaker.speak(1, "same sentence.", (0, 14));
        assert_eq!(wait_for_finishes(&mut speaker, 1), 1, "first playback");
        speaker.set_rate(2.0);
        speaker.speak(2, "same sentence.", (0, 14));
        assert_eq!(wait_for_finishes(&mut speaker, 1), 1, "cached playback");

        assert_eq!(
            count(&log, "render-start"),
            1,
            "playback rate entered the synthesis cache key, or no durable cache was used"
        );
        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A long book must not turn prefetch into an unbounded second audiobook
    /// on disk. The current and queued scratch clips are separate; this counts
    /// only reusable content-addressed entries.
    #[test]
    fn speech_cache_has_a_hard_entry_budget() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let dir = scratch("cache-budget");
        let mut speaker = Speaker::spawn(Box::new(Fake::new(&log, 0, 1)), dir.clone(), 8);
        for n in 0..130u64 {
            let text = format!("unique sentence {n}.");
            speaker.speak(n, &text, (0, text.len()));
        }
        assert_eq!(wait_for_finishes(&mut speaker, 130), 130);

        let entries = std::fs::read_dir(dir.join("cache-v1"))
            .expect("speech cache")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "wav"))
            .count();
        assert!(
            entries <= 128,
            "the reusable speech cache grew to {entries} entries"
        );

        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Prefetch is a window, not a licence to render the whole chapter: a slow
    /// player must not have a hundred wavs piling up behind it.
    ///
    /// The player is parked inside the first clip for the duration of the
    /// assertion. Nothing can be popped while it is parked, so the window is a
    /// fact about the pipeline rather than about the clock — an earlier version
    /// slept for 90 ms and counted, and failed on a loaded CI machine that had
    /// simply got further along by the time it woke up.
    #[test]
    fn rendering_stays_within_the_prefetch_window() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let hold = Arc::new(AtomicBool::new(true));
        let dir = scratch("window");
        let mut speaker = Speaker::spawn(
            Box::new(Fake::new(&log, 10, 0).parked(&hold)),
            dir.clone(),
            2,
        );
        for n in 0..6u64 {
            let text = format!("sentence {n}.");
            speaker.speak(n, &text, (0, text.len()));
        }

        // One clip in the player, `prefetch` queued behind it, and there the
        // render thread must stop: pushing a fourth needs room, and room needs
        // a pop the parked player cannot make.
        let rendered = count_at_least(&log, "render-end", 3);
        assert_eq!(
            rendered, 3,
            "with prefetch 2, the playing clip plus two is the whole window"
        );
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(
            count(&log, "render-end"),
            3,
            "a parked player must not let a fourth clip through"
        );

        hold.store(false, Ordering::SeqCst);
        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }

    struct RunwayFake {
        log: Log,
        hold: Arc<AtomicBool>,
        media_ms: u64,
    }

    impl Synthesizer for RunwayFake {
        fn synthesize(&self, _text: &str, out: &Path) -> Result<Clip> {
            self.log
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(("render-start", Instant::now()));
            std::fs::write(out, wav::silence(self.media_ms, 24_000))?;
            Ok(Clip {
                path: out.to_path_buf(),
                ms: self.media_ms,
            })
        }

        fn play(&self, _clip: &Clip, control: &PlaybackControl) -> Result<()> {
            control.started();
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.hold.load(Ordering::SeqCst)
                && !control.cancelled()
                && Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(())
        }

        fn describe(&self) -> String {
            format!("runway-fake-{}", self.media_ms)
        }
    }

    fn rendered_runway_at(rate: f32, tag: &str) -> (usize, Arc<AtomicBool>, Speaker, PathBuf) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let hold = Arc::new(AtomicBool::new(true));
        let dir = scratch(tag);
        let mut speaker = Speaker::spawn(
            Box::new(RunwayFake {
                log: Arc::clone(&log),
                hold: Arc::clone(&hold),
                media_ms: 10_000,
            }),
            dir.clone(),
            8,
        );
        speaker.set_rate(rate);
        for n in 0..10 {
            speaker.speak(n, &format!("sentence {n}."), (0, 10));
        }

        let minimum = if rate > 1.0 { 5 } else { 3 };
        assert_eq!(count_at_least(&log, "render-start", minimum), minimum);
        std::thread::sleep(Duration::from_millis(120));
        (count(&log, "render-start"), hold, speaker, dir)
    }

    /// The controller optimises wall-clock runway from the live media cursor.
    /// Ten seconds of canonical audio buys ten seconds at 1× but only five at
    /// 2×, so the faster transport must render farther ahead without simply
    /// filling the sentence cap in both cases.
    #[test]
    fn prefetch_window_uses_playback_runway_and_rate() {
        let (normal, normal_hold, normal_speaker, normal_dir) =
            rendered_runway_at(1.0, "runway-normal");
        let (fast, fast_hold, fast_speaker, fast_dir) = rendered_runway_at(2.0, "runway-fast");

        assert_eq!(normal, 3, "1× should stop near the 24s runway target");
        assert_eq!(
            fast, 5,
            "2× needs twice as much source audio for that target"
        );
        assert!(fast < 9, "the local optimum ignored the sentence hard cap");

        normal_hold.store(false, Ordering::SeqCst);
        fast_hold.store(false, Ordering::SeqCst);
        drop(normal_speaker);
        drop(fast_speaker);
        let _ = std::fs::remove_dir_all(normal_dir);
        let _ = std::fs::remove_dir_all(fast_dir);
    }

    #[test]
    fn pause_holds_the_live_clip_without_rendering_it_again() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let dir = scratch("pause");
        let mut speaker = Speaker::spawn(Box::new(Fake::new(&log, 10, 240)), dir.clone(), 2);
        speaker.speak(7, "a sentence.", (4, 15));
        assert_eq!(count_at_least(&log, "play-start", 1), 1);

        speaker.pause();
        std::thread::sleep(Duration::from_millis(320));
        assert_eq!(
            count(&log, "play-end"),
            0,
            "the audio cursor advanced while playback was paused"
        );
        assert!(speaker.busy(), "pause must preserve the live sentence");

        speaker.resume();
        assert_eq!(wait_for_finishes(&mut speaker, 1), 1);
        assert_eq!(
            count(&log, "render-start"),
            1,
            "resume rendered the same sentence a second time"
        );
        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The app may identify the next paragraph while paused, but demand is
    /// frozen. The candidate stays queued and only becomes synthesis work when
    /// the transport resumes.
    #[test]
    fn pause_does_not_expand_the_boundary_prefetch_window() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let dir = scratch("paused-prefetch");
        let speaker = Speaker::spawn(Box::new(Fake::new(&log, 20, 20)), dir.clone(), 8);

        speaker.pause();
        speaker.prerender("next paragraph.");
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            count(&log, "render-start"),
            0,
            "a paused transport expanded its prefetch window"
        );

        speaker.resume();
        assert_eq!(count_at_least(&log, "render-end", 1), 1);
        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The UI clock must begin when the player does, not when a rendered file
    /// merely leaves the queue. Process startup is normally small, but it is
    /// still real time in which advancing tokens or highlights would put the
    /// page ahead of the sound.
    #[test]
    fn playback_started_is_reported_only_when_the_player_really_starts() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let dir = scratch("player-start");
        let fake = Fake::new(&log, 10, 80).starting_after(180);
        let mut speaker = Speaker::spawn(Box::new(fake), dir.clone(), 1);
        speaker.speak(5, "a sentence.", (3, 14));
        assert_eq!(count_at_least(&log, "render-end", 1), 1);

        std::thread::sleep(Duration::from_millis(70));
        assert!(
            !speaker
                .poll()
                .iter()
                .any(|event| matches!(event, SpeechEvent::Started { .. })),
            "the global playback clock started while the OS player was still starting"
        );

        assert_eq!(count_at_least(&log, "play-start", 1), 1);
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut started = false;
        while !started && Instant::now() < deadline {
            started = speaker
                .poll()
                .iter()
                .any(|event| matches!(event, SpeechEvent::Started { .. }));
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(started, "the real player start was never reported");
        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Returning from pause before the player has frozen means the UI and the
    /// audio cursor record two different pause instants. The global clock's
    /// pause boundary is the player's acknowledgement, even when that player
    /// takes a noticeable polling interval to apply it.
    #[test]
    fn pause_returns_only_after_the_global_clock_is_frozen() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let dir = scratch("pause-ack");
        let fake = Fake::new(&log, 5, 600).checking_controls_every(120);
        let mut speaker = Speaker::spawn(Box::new(fake), dir.clone(), 1);
        speaker.speak(8, "a sentence.", (0, 11));
        assert_eq!(count_at_least(&log, "play-start", 1), 1);

        speaker.pause();
        let position = speaker.position().expect("the active playback clock");
        assert!(
            position.paused,
            "pause returned while the global playback clock was still advancing"
        );

        speaker.resume();
        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A pause requested while the OS player is starting belongs to that clip,
    /// even though there is no audio cursor to acknowledge yet. Starting the
    /// global clock as running would let text move under a paused reader and
    /// makes resume jump by the whole process-start delay.
    #[test]
    fn a_clip_that_starts_during_pause_enters_the_clock_paused() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let dir = scratch("pause-during-start");
        let fake = Fake::new(&log, 5, 400).starting_after(180);
        let mut speaker = Speaker::spawn(Box::new(fake), dir.clone(), 1);
        speaker.speak(13, "a sentence.", (0, 11));
        assert_eq!(count_at_least(&log, "render-end", 1), 1);

        speaker.pause();
        assert_eq!(count_at_least(&log, "play-start", 1), 1);
        let first = speaker.position().expect("the started clip");
        assert!(first.paused, "the clip ignored the pending pause");
        std::thread::sleep(Duration::from_millis(60));
        let later = speaker.position().expect("the same clip");
        assert_eq!(
            later.elapsed, first.elapsed,
            "the clock advanced while the player was paused"
        );

        speaker.resume();
        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Once a device callback owns the timeline, wall time is irrelevant. The
    /// position is exactly the number of frames the output layer has reported,
    /// and remains stable between callbacks even if the UI thread sleeps.
    #[test]
    fn a_frame_clock_advances_only_when_audio_frames_are_presented() {
        let clock = PlaybackClock::default();
        assert!(clock.start_frames(21, (2, 9), 48_000, 48_000, 7, false));
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(
            clock.position().expect("frame clock").elapsed,
            Duration::ZERO,
            "wall time advanced a callback-driven clock"
        );

        clock.present_frames(7, 12_000);
        assert_eq!(
            clock.position().expect("frame clock").elapsed,
            Duration::from_millis(250)
        );
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(
            clock.position().expect("frame clock").elapsed,
            Duration::from_millis(250),
            "the clock integrated a second, independent timer"
        );
    }

    /// A real stop throws away unplayed scratch clips. Reusable canonical WAVs
    /// remain in the bounded cache; changing playback speed does not call this.
    #[test]
    fn stopping_discards_unplayed_scratch_clips() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let fake = Fake::new(&log, 10, 200);
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
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| entry.path().is_file())
                    .count()
            })
            .unwrap_or(0);
        assert_eq!(
            leftovers, 0,
            "discarded clips must not litter the scratch dir"
        );
        drop(speaker);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
