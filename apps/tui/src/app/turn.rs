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

    pub fn busy(&self) -> bool {
        self.active.is_some() || !self.steps.is_empty()
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
                }) => {
                    let pacer = match kind {
                        StreamKind::Thinking => &mut self.thinking,
                        StreamKind::Passage => &mut self.reading,
                    };
                    let released = pacer.pump(queue, dt_ms);
                    if !released.is_empty() {
                        let counted = released.chars().filter(|c| *c != '\n').count();
                        if *kind == StreamKind::Passage {
                            self.turn_chars += counted;
                            self.session_chars += counted;
                        }
                        sb.push_chunk(*id, &released);
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
                    });
                    break;
                }
                Step::Say { text, emphasis } => {
                    let id = sb.push_running(Block::passage(emphasis));
                    self.spoken = Some((id, text.clone()));
                    self.active = Some(Active::Stream {
                        id,
                        kind: StreamKind::Passage,
                        queue: split_phrases(&text),
                        started: Instant::now(),
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
