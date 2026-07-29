//! The turn state machine.
//!
//! A turn is a queue of steps — think, call a tool, stream a passage, record an
//! event — advanced by `pump` once per frame. Nothing here knows about books:
//! the reading logic in `app::flow` composes steps, and this module only owns
//! their timing.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::stream::{Chunk, Pacer, split_phrases};
use crate::ui::block::{Block, ContextInfo, Event, PlanItem, Tool};
use crate::ui::scrollback::Scrollback;

#[derive(Debug, Clone)]
pub enum Step {
    /// Reasoning text, streamed fast then folded away.
    Think(String),
    /// A tool call that stays "running" for `ms` before completing.
    Tool {
        tool: Tool,
        ms: u64,
    },
    /// Book content, streamed at reading speed, with whatever the book
    /// emphasised inside it.
    Say {
        text: String,
        emphasis: Vec<crate::book::Emphasis>,
    },
    /// An illustration, shown whole.
    Image {
        path: std::path::PathBuf,
        alt: String,
    },
    /// An instant system note.
    Note(String),
    Event(Event),
    Plan {
        title: String,
        items: Vec<PlanItem>,
        hidden: usize,
    },
    Context(ContextInfo),
    /// Commit a new reading position once the preceding steps have played.
    Advance {
        chapter: usize,
        para: usize,
    },
}

impl Step {
    /// Text with nothing emphasised in it — a reply readio wrote itself, rather
    /// than a passage out of a book.
    pub fn say(text: String) -> Self {
        Step::Say {
            text,
            emphasis: Vec::new(),
        }
    }
}

/// Things the turn asks the app to do as steps complete.
#[derive(Debug, Clone, Copy)]
pub enum Effect {
    Advance { chapter: usize, para: usize },
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    Thinking,
    Passage,
}

enum Active {
    Stream {
        id: u64,
        kind: StreamKind,
        queue: VecDeque<Chunk>,
        started: Instant,
        /// Characters of this stream already on screen, counted the way the
        /// speech ranges count them — newlines included — so the two agree.
        released: usize,
        /// The passage as it was handed over, kept whole. The queue holds only
        /// what is left, and read-aloud entered mid-passage needs the original
        /// to work out which sentence the reader has got to.
        source: String,
    },
    Tool {
        id: u64,
        started: Instant,
        until: Instant,
    },
}

pub struct Turn {
    steps: VecDeque<Step>,
    active: Option<Active>,
    /// Pacer for book content.
    reading: Pacer,
    /// Pacer for reasoning, which runs faster than reading.
    thinking: Pacer,
    started: Option<Instant>,
    /// Duration of the most recently finished turn, kept for its summary line.
    last_ms: u64,
    /// Characters of book content streamed during this turn.
    pub turn_chars: usize,
    /// Characters streamed since the process started.
    pub session_chars: usize,
    /// Tallest an illustration may be drawn, from `config.yaml`.
    pub image_rows: u16,
    /// Draw illustrations at all. When off, a picture reads as its caption.
    pub images: bool,
    /// Passage that just began streaming, for the reader-aloud queue.
    spoken: Option<(u64, String)>,
    /// How far into the current passage the text may go, in characters.
    ///
    /// Read-aloud sets this to the end of the sentence being spoken. Without it
    /// the reveal is only *nudged* towards the audio — the pacer is told the
    /// clip's characters per second once the clip starts — and a voice that
    /// reads Chinese at four characters a second cannot catch a reveal that
    /// spent the whole synthesis wait running at forty-six.
    reveal_limit: Option<usize>,
    /// Whether a passage starting from now is going to be read aloud, and so
    /// should wait for its first clip instead of streaming into the silence.
    voiced: bool,
    /// When the reader pressed pause. Everything in flight keeps its place; the
    /// clocks are wound forward by this much on the way back, so a tool call
    /// paused for a minute does not come back claiming it took a minute.
    paused: Option<Instant>,
}

impl Turn {
    pub fn new(cps: f32) -> Self {
        Self {
            steps: VecDeque::new(),
            active: None,
            reading: Pacer::new(cps),
            thinking: Pacer::new(cps * 2.6),
            started: None,
            last_ms: 0,
            turn_chars: 0,
            session_chars: 0,
            image_rows: 16,
            images: true,
            spoken: None,
            reveal_limit: None,
            voiced: false,
            paused: None,
        }
    }

    /// Hold everything where it is. `false` means there was nothing in flight.
    pub fn pause(&mut self) -> bool {
        if !self.busy() || self.paused.is_some() {
            return false;
        }
        self.paused = Some(Instant::now());
        true
    }

    /// Carry on from exactly where the pause caught it.
    pub fn resume(&mut self) {
        let Some(at) = self.paused.take() else {
            return;
        };
        // The pacer runs on frame deltas and simply stopped receiving them, so
        // only the wall-clock deadlines need moving.
        let slept = at.elapsed();
        if let Some(Active::Tool { started, until, .. }) = self.active.as_mut() {
            *started += slept;
            *until += slept;
        }
        if let Some(Active::Stream { started, .. }) = self.active.as_mut() {
            *started += slept;
        }
        if let Some(started) = self.started.as_mut() {
            *started += slept;
        }
    }

    pub fn paused(&self) -> bool {
        self.paused.is_some()
    }

    /// Skip the theatre: finish the current thinking or tool wait now, and let
    /// the rest of the turn stream at whatever speed the caller set.
    ///
    /// Without this, pressing enter during the two seconds a `Read` call spends
    /// spinning does nothing at all, which reads as a dead keyboard.
    pub fn rush(&mut self) {
        if let Some(Active::Tool { until, .. }) = self.active.as_mut() {
            *until = Instant::now();
        }
    }

    pub fn set_cps(&mut self, cps: f32) {
        self.reading.set_cps(cps);
        self.thinking.set_cps(cps * 2.6);
    }

    pub fn cps(&self) -> f32 {
        self.reading.cps
    }

    /// Stop the passage reveal at `chars` characters in. `None` lets it run at
    /// whatever pace it was given.
    pub fn hold_reveal(&mut self, limit: Option<usize>) {
        self.reveal_limit = limit;
    }

    /// Whether something is currently holding the reveal back.
    pub fn held(&self) -> bool {
        self.reveal_limit.is_some()
    }

    /// Tell the turn that passages from here on are read aloud, so each one
    /// waits for its own audio before showing a character.
    ///
    /// The alternative — arming the hold from the app once the passage has been
    /// handed to the speech worker — leaves one frame in which the old pace is
    /// still in force, and one frame of a 4000 characters-per-second rush is a
    /// visible flash of the paragraph before it snaps back.
    pub fn set_voiced(&mut self, voiced: bool) {
        self.voiced = voiced;
    }

    /// The passage streaming right now: which block it is, and how many of its
    /// characters have been shown.
    pub fn streaming_passage(&self) -> Option<(u64, usize)> {
        match &self.active {
            Some(Active::Stream {
                id,
                kind: StreamKind::Passage,
                released,
                ..
            }) => Some((*id, *released)),
            _ => None,
        }
    }

    /// The passage on its way out, whole, with how much of it has been shown.
    ///
    /// Read-aloud can be entered in the middle of a paragraph, and that
    /// paragraph has to come under the voice with everything else — otherwise
    /// the reader watches the rest of it scroll past in the silence of the mode
    /// they just left.
    pub fn passage_in_flight(&self) -> Option<(u64, &str, usize)> {
        match &self.active {
            Some(Active::Stream {
                id,
                kind: StreamKind::Passage,
                released,
                source,
                ..
            }) => Some((*id, source.as_str(), *released)),
            _ => None,
        }
    }

    pub fn busy(&self) -> bool {
        self.active.is_some() || !self.steps.is_empty()
    }

    /// Whether any speech is still to be handed over in this turn.
    ///
    /// A passage goes to the voice whole, at the moment it starts streaming, so
    /// once the queue holds no more of them the voice has everything this turn
    /// is ever going to give it — and whatever it says next has to come from
    /// somewhere else.
    pub fn more_to_say(&self) -> bool {
        self.steps
            .iter()
            .any(|step| matches!(step, Step::Say { .. }))
    }

    /// Where this turn will leave the reader, while it is still running.
    ///
    /// The `Advance` step sits at the back of the queue for the whole turn,
    /// which makes it the honest answer to "what comes after this" — and that
    /// is what read-aloud needs in order to have the next paragraph's opening
    /// sentence rendered before the current one runs out of sound.
    pub fn advancing_to(&self) -> Option<(usize, usize)> {
        self.steps.iter().rev().find_map(|step| match step {
            Step::Advance { chapter, para } => Some((*chapter, *para)),
            _ => None,
        })
    }

    pub fn enqueue(&mut self, steps: Vec<Step>) {
        if self.started.is_none() {
            self.started = Some(Instant::now());
            self.turn_chars = 0;
        }
        self.steps.extend(steps);
    }

    /// Stop everything, leaving finished blocks in place.
    pub fn interrupt(&mut self, sb: &mut Scrollback) -> bool {
        let was_busy = self.busy();
        self.steps.clear();
        self.spoken = None;
        self.reveal_limit = None;
        match self.active.take() {
            Some(Active::Stream { id, started, .. }) => {
                sb.finish(id, Some(elapsed_ms(started)));
                sb.remove_if_empty(id);
            }
            Some(Active::Tool { id, started, .. }) => {
                sb.finish(id, Some(elapsed_ms(started)));
            }
            None => {}
        }
        self.started = None;
        was_busy
    }

    /// Advance by `dt_ms`, mutating the scrollback. Returns queued effects.
    pub fn pump(&mut self, sb: &mut Scrollback, dt_ms: f32) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.paused.is_some() {
            // Paused means paused: no text released, no tool completing behind
            // the reader's back, and the position not advanced.
            return effects;
        }

        loop {
            // 1. Service whatever is currently running.
            match self.active.as_mut() {
                Some(Active::Stream {
                    id,
                    kind,
                    queue,
                    started,
                    released,
                    ..
                }) => {
                    let pacer = match kind {
                        StreamKind::Thinking => &mut self.thinking,
                        StreamKind::Passage => &mut self.reading,
                    };
                    // Reasoning is readio's own voice and nobody speaks it, so
                    // only book content answers to the hold.
                    let ceiling = match kind {
                        StreamKind::Passage => self
                            .reveal_limit
                            .map(|limit| limit.saturating_sub(*released)),
                        StreamKind::Thinking => None,
                    };
                    let text = pacer.pump(queue, dt_ms, ceiling);
                    if !text.is_empty() {
                        *released += text.chars().count();
                        let counted = text.chars().filter(|c| *c != '\n').count();
                        if *kind == StreamKind::Passage {
                            self.turn_chars += counted;
                            self.session_chars += counted;
                        }
                        sb.push_chunk(*id, &text);
                    }
                    if queue.is_empty() {
                        sb.finish(*id, Some(elapsed_ms(*started)));
                        self.active = None;
                        continue;
                    }
                    break;
                }
                Some(Active::Tool { id, started, until }) => {
                    if Instant::now() >= *until {
                        sb.finish(*id, Some(elapsed_ms(*started)));
                        self.active = None;
                        continue;
                    }
                    break;
                }
                None => {}
            }

            // 2. Nothing running: start the next step.
            let Some(step) = self.steps.pop_front() else {
                if let Some(started) = self.started.take() {
                    self.last_ms = elapsed_ms(started);
                    effects.push(Effect::Finished);
                }
                break;
            };

            match step {
                Step::Think(text) => {
                    let id = sb.push_running(Block::thinking());
                    self.active = Some(Active::Stream {
                        id,
                        kind: StreamKind::Thinking,
                        queue: split_phrases(&text),
                        started: Instant::now(),
                        released: 0,
                        // Nobody speaks readio's own reasoning, so there is
                        // nothing here to line a voice up against.
                        source: String::new(),
                    });
                    break;
                }
                Step::Say { text, emphasis } => {
                    let id = sb.push_running(Block::passage(emphasis));
                    self.spoken = Some((id, text.clone()));
                    // A passage about to be read aloud starts held shut: the
                    // first characters belong to the first clip, and that clip
                    // does not exist yet.
                    self.reveal_limit = self.voiced.then_some(0);
                    self.active = Some(Active::Stream {
                        id,
                        kind: StreamKind::Passage,
                        queue: split_phrases(&text),
                        started: Instant::now(),
                        released: 0,
                        source: text,
                    });
                    break;
                }
                Step::Tool { tool, ms } => {
                    let id = sb.push_running(Block::Tool(tool));
                    let now = Instant::now();
                    self.active = Some(Active::Tool {
                        id,
                        started: now,
                        until: now + Duration::from_millis(ms),
                    });
                    break;
                }
                Step::Image { path, alt } => {
                    if self.images {
                        sb.push(Block::image(path, alt, self.image_rows));
                    } else if !alt.trim().is_empty() {
                        sb.push(Block::System(alt));
                    }
                }
                Step::Note(text) => {
                    sb.push(Block::System(text));
                }
                Step::Event(event) => {
                    sb.push(Block::Event(event));
                }
                Step::Plan {
                    title,
                    items,
                    hidden,
                } => {
                    sb.push(Block::Plan {
                        title,
                        items,
                        hidden,
                    });
                }
                Step::Context(info) => {
                    sb.push(Block::Context(info));
                }
                Step::Advance { chapter, para } => {
                    effects.push(Effect::Advance { chapter, para });
                }
            }
        }

        effects
    }

    /// Duration of the turn that just finished, for its summary line.
    pub fn elapsed_ms(&self) -> u64 {
        self.started.map(elapsed_ms).unwrap_or(self.last_ms)
    }

    /// Claim the passage that started streaming since the last call, so the app
    /// can hand it to the speech worker.
    pub fn take_spoken(&mut self) -> Option<(u64, String)> {
        self.spoken.take()
    }
}

fn elapsed_ms(since: Instant) -> u64 {
    since.elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::scrollback::Scrollback;

    fn passage(text: &str) -> Step {
        Step::Say {
            text: text.to_string(),
            emphasis: Vec::new(),
        }
    }

    /// Fast enough to empty any of these passages in a single frame, which is
    /// the point: everything these tests hold back is held back by the hold.
    fn turn(voiced: bool) -> (Turn, Scrollback) {
        let mut turn = Turn::new(4_000.0);
        turn.set_voiced(voiced);
        (turn, Scrollback::new())
    }

    fn shown(turn: &Turn) -> usize {
        turn.streaming_passage()
            .map(|(_, chars)| chars)
            .unwrap_or(0)
    }

    /// The whole of the bug this exists for: read-aloud used to stream the text
    /// at the reading speed and only *nudge* it towards the audio once a clip
    /// started. Chinese is spoken at about four characters a second and the
    /// reveal ran at forty-six, so by the time the first clip existed the
    /// paragraph was already on screen — and with auto-advance on, so was the
    /// rest of the chapter.
    #[test]
    fn a_passage_waiting_for_its_voice_shows_nothing() {
        let (mut turn, mut sb) = turn(true);
        turn.enqueue(vec![passage("第一句。第二句。第三句。")]);
        for _ in 0..120 {
            turn.pump(&mut sb, 16.0);
        }
        assert_eq!(shown(&turn), 0, "not one character before the first clip");
        assert!(turn.busy(), "the passage is waiting, not finished");
    }

    /// Nothing past the sentence being spoken, and everything up to it.
    #[test]
    fn the_reveal_stops_where_the_voice_is() {
        let (mut turn, mut sb) = turn(true);
        turn.enqueue(vec![passage("第一句。第二句。第三句。")]);
        turn.pump(&mut sb, 16.0);

        turn.hold_reveal(Some(4));
        for _ in 0..60 {
            turn.pump(&mut sb, 16.0);
        }
        assert_eq!(shown(&turn), 4, "「第一句。」 and not a character more");

        turn.hold_reveal(Some(8));
        for _ in 0..60 {
            turn.pump(&mut sb, 16.0);
        }
        assert_eq!(shown(&turn), 8, "the second sentence, once it is spoken");
    }

    /// The hold is lifted when the voice stops for any reason, and the rest of
    /// the passage has to be able to land: sentence ranges skip blank lines and
    /// code fences, so a passage can outlive its last clip.
    #[test]
    fn lifting_the_hold_lets_the_rest_of_the_passage_land() {
        let (mut turn, mut sb) = turn(true);
        turn.enqueue(vec![passage("第一句。第二句。")]);
        turn.pump(&mut sb, 16.0);
        turn.hold_reveal(None);
        for _ in 0..60 {
            turn.pump(&mut sb, 16.0);
        }
        assert!(!turn.busy(), "the passage should have finished");
    }

    /// Silent reading is untouched: no voice, no hold.
    #[test]
    fn a_passage_nobody_is_speaking_streams_straight_away() {
        let (mut turn, mut sb) = turn(false);
        turn.enqueue(vec![passage("第一句。第二句。")]);
        // The first frame starts the stream; the second is the first one that
        // can release anything.
        turn.pump(&mut sb, 16.0);
        turn.pump(&mut sb, 16.0);
        assert!(
            shown(&turn) > 0,
            "silent reading must not wait for anything"
        );
    }

    /// Reasoning is readio's own voice and nothing speaks it, so a hold left
    /// over from a passage must not freeze the thinking line too.
    #[test]
    fn a_hold_meant_for_a_passage_does_not_stop_the_thinking() {
        let (mut turn, mut sb) = turn(true);
        turn.enqueue(vec![Step::Think("正在检索相关段落。".to_string())]);
        turn.hold_reveal(Some(0));
        for _ in 0..60 {
            turn.pump(&mut sb, 16.0);
        }
        assert!(!turn.busy(), "thinking answers to no clip");
    }
}
