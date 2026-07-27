//! Application state, input dispatch, and layout.

pub mod flow;
pub mod turn;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block as UiBlock, Widget};

use crate::book::Book;
use crate::config::Config;
use crate::i18n::{t, tf};
use crate::library::{Library, Mode};
use crate::paths;
use crate::store::{Progress, Store, now_secs};
use crate::theme::theme;
use crate::tts::device::{self, Gate, Verdict};
use crate::tts::sentence::Unit;
use crate::tts::{Speaker, SpeechEvent, command::CommandSynth, sentence};
use crate::ui::block::{Block, Event, LibraryRow, Speaking};
use crate::ui::chrome::{self, Chrome};
use crate::ui::prompt::Prompt;
use crate::ui::scrollback::Scrollback;
use crate::util::human;

use flow::Pos;
use turn::{Effect, Turn};

/// How long a status-line notice stays up.
const NOTICE_TTL: Duration = Duration::from_secs(4);
/// Second Ctrl+C within this window quits.
const QUIT_WINDOW: Duration = Duration::from_secs(2);
/// Reading speed used when the reader presses Enter to skip ahead.
const RUSH_CPS: f32 = 2_400.0;

pub struct App {
    pub sb: Scrollback,
    pub prompt: Prompt,
    pub turn: Turn,
    /// The book being read. `None` while readio is showing the library.
    pub book: Option<Book>,
    pub pos: Pos,
    pub store: Store,
    pub library: Library,
    pub tick: u64,
    pub quit: bool,
    show_help: bool,
    notice: Option<(String, Instant)>,
    /// The next reading turn should explain that it restored progress.
    resumed: bool,
    /// Keep queueing turns until interrupted.
    auto: bool,
    /// Reader's configured speed, restored after a rush.
    base_cps: f32,
    rushing: bool,
    /// Library index of the book read last, offered on the empty prompt.
    resume: Option<usize>,
    last_frame: Instant,
    ctrl_c_at: Option<Instant>,
    /// Wall clock for this reading session, shown as the agent's uptime.
    started: Instant,
    /// Every setting, loaded from `~/.readio/config.yaml`.
    cfg: Config,
    /// Live synthesizer, spawned on demand.
    speaker: Option<Speaker>,
    /// Reveal speed derived from the last clip, restored when speech stops.
    speech_cps: Option<f32>,
    /// Scrollback entry currently highlighted as spoken.
    lit: Option<u64>,
    /// Passage handed to the speech worker, kept so a `Started` event can be
    /// turned back into words: the event carries a range, not the text.
    voiced: Option<(u64, String)>,
    /// The sentence being sounded, and the word cursor walking through it.
    cursor: Option<Cursor>,
    /// The worker has nothing left to say; acted on once the last clip's own
    /// duration has elapsed.
    idle: bool,
    /// Output-device whitelist. `None` when no whitelist is configured, which
    /// is also when nothing is ever probed.
    gate: Option<Gate>,
}

/// A sentence in flight: its audio clock and the words to walk through.
struct Cursor {
    /// Scrollback entry holding the passage.
    id: u64,
    /// Byte range of the sentence inside that passage.
    sentence: (usize, usize),
    /// Words or characters, in passage coordinates.
    units: Vec<Unit>,
    started: Instant,
    /// Clip duration, which is what the cursor's speed is derived from.
    ms: u64,
    /// Last unit shown, so the scrollback is only touched when it changes.
    shown: Option<(usize, usize)>,
    /// The worker says this clip is done. The highlight still waits for the
    /// clip's own duration: some players hand the audio to a daemon and return
    /// immediately, and the highlight has to follow the sound, not the process.
    done: bool,
}

impl App {
    /// Start up, either with a book already opened or on the library listing.
    ///
    /// `note` is the one-line import summary printed above the first turn, so
    /// the reader sees where their file went.
    pub fn new(library: Library, store: Store, opened: Option<Book>, note: Option<String>) -> Self {
        let (cfg, cfg_note) = Config::load();
        let cps = cfg.reading.speed;
        let mut app = Self {
            sb: Scrollback::new(),
            prompt: Prompt::new(),
            turn: Turn::new(cps),
            book: None,
            pos: Pos::default(),
            store,
            library,
            tick: 0,
            quit: false,
            show_help: false,
            notice: None,
            resumed: false,
            auto: false,
            base_cps: cps,
            rushing: false,
            resume: None,
            last_frame: Instant::now(),
            ctrl_c_at: None,
            started: Instant::now(),
            cfg,
            speaker: None,
            speech_cps: None,
            lit: None,
            voiced: None,
            cursor: None,
            idle: false,
            gate: None,
        };

        if app.cfg.tts.output.is_active() {
            app.gate = Some(Gate::new(app.cfg.tts.output.clone()));
        }
        app.turn.image_rows = app.cfg.images.max_rows;
        app.turn.images = app.cfg.images.enabled;
        if let Some(warning) = cfg_note {
            app.system(&warning);
        }
        if let Some(note) = note {
            app.system(&note);
        }
        match opened {
            Some(book) => app.adopt(book),
            None => app.show_library(),
        }
        app
    }

    // ── book lifecycle ───────────────────────────────────────────────────────

    /// Make `book` the open one, restoring its saved position.
    fn adopt(&mut self, book: Book) {
        self.save_progress();
        let saved = self.store.get(&book.id).cloned();
        let pos = saved
            .as_ref()
            .map(|p| Pos {
                chapter: p.chapter.min(book.chapters.len().saturating_sub(1)),
                para: p.para,
            })
            .unwrap_or_default();
        let pos = flow::normalize(&book, pos);

        self.resumed = saved.is_some() && (pos.chapter > 0 || pos.para > 0);
        self.pos = pos;
        self.library.touch(&book.id);
        self.book = Some(book);
        self.auto = false;
        self.resume = None;
        self.prompt.placeholder = t("prompt.reading").to_string();

        let book = self.book.as_ref().expect("just set");
        let steps = flow::welcome(book, pos, self.resumed);
        let plan = flow::plan(book, pos);
        self.turn.enqueue(steps);
        self.turn.enqueue(plan);
        self.begin_session();
    }

    /// Push the imported-books listing into the scrollback.
    fn show_library(&mut self) {
        let rows: Vec<LibraryRow> = self
            .library
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let progress = self
                    .store
                    .get(&entry.id)
                    .map(|p| {
                        if entry.chars == 0 {
                            0.0
                        } else {
                            (p.chars_read as f32 / entry.chars as f32).clamp(0.0, 1.0)
                        }
                    })
                    .unwrap_or(0.0);
                LibraryRow {
                    index: i + 1,
                    title: entry.title.clone(),
                    author: entry.author.clone(),
                    chars: entry.chars,
                    progress,
                    mode: entry.mode.label(),
                    current: self.book.as_ref().is_some_and(|open| open.id == entry.id),
                    missing: !entry.available(),
                }
            })
            .collect();

        let hint = if rows.is_empty() {
            tf("lib.empty_hint", &[&paths::display(&paths::books_dir())])
        } else {
            let mut hint = t("lib.pick_hint").to_string();
            // Continuing the last book is the common case, so make it a keypress.
            let recent = self
                .library
                .most_recent()
                .and_then(|entry| {
                    self.library
                        .entries
                        .iter()
                        .position(|e| e.id == entry.id)
                        .map(|i| (i + 1, entry.title.clone()))
                })
                .filter(|_| self.book.is_none());
            self.resume = recent.as_ref().map(|(index, _)| *index);
            if let Some((index, title)) = recent {
                hint = tf("lib.resume_hint", &[&title, &index]);
            }
            hint
        };

        self.prompt.placeholder = if self.book.is_some() {
            t("prompt.reading").to_string()
        } else if self.library.is_empty() {
            t("prompt.empty_library").to_string()
        } else {
            t("prompt.pick").to_string()
        };
        self.sb.push(Block::Library { rows, hint });
    }

    // ── frame ────────────────────────────────────────────────────────────────

    /// Advance timers and the turn machine. Called once per frame.
    pub fn on_tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32() * 1000.0;
        self.last_frame = now;
        self.tick = self.tick.wrapping_add(1);

        if let Some((_, at)) = &self.notice
            && at.elapsed() > NOTICE_TTL
        {
            self.notice = None;
        }

        let effects = self.turn.pump(&mut self.sb, dt.min(200.0));
        if let Some((id, text)) = self.turn.take_spoken() {
            self.enqueue_speech(id, &text);
        }
        self.poll_audio_device();
        self.pump_speech();
        self.advance_cursor();
        for effect in effects {
            match effect {
                Effect::Advance { chapter, para } => {
                    // Roll past a finished chapter immediately so the header and
                    // the saved position agree with what was just read.
                    if let Some(book) = self.book.as_ref() {
                        self.pos = flow::normalize(book, Pos { chapter, para });
                    }
                    self.resumed = false;
                    self.save_progress();
                }
                Effect::Finished => self.on_turn_finished(),
            }
        }
    }

    fn on_turn_finished(&mut self) {
        if self.rushing {
            self.turn.set_cps(self.base_cps);
            self.rushing = false;
        }
        let chars = self.turn.turn_chars;
        if chars > 0 {
            self.sb.push(Block::Event(Event::TurnComplete {
                ms: self.turn.elapsed_ms(),
                chars,
            }));
        }
        if self.auto && !self.at_end() {
            self.read_more();
        }
    }

    /// Path for the opt-in input trace, straight from the config file.
    pub fn input_log_path(&self) -> Option<PathBuf> {
        let raw = self.cfg.input_log.as_deref()?.trim();
        if raw.is_empty() {
            return None;
        }
        Some(expand_tilde(raw))
    }

    // ── audio output whitelist ───────────────────────────────────────────────

    /// Whether sound may leave the machine right now.
    fn audio_permitted(&self) -> bool {
        self.gate.as_ref().is_none_or(Gate::permits_audio)
    }

    /// Check the output device, and speak up the moment it changes.
    ///
    /// This is the whole point of the whitelist: headphones going to sleep
    /// silently re-routes audio to the built-in speakers, and the reader has to
    /// learn about it from readio rather than from the room.
    fn poll_audio_device(&mut self) {
        let Some(gate) = self.gate.as_mut() else {
            return;
        };
        let Some(verdict) = gate.poll() else {
            return;
        };
        match verdict {
            Verdict::Allowed { device } => {
                if let Some(device) = device {
                    self.notice(&tf("dev.resumed", &[&device.label()]));
                }
            }
            Verdict::Blocked { .. } | Verdict::Unknown { .. } => {
                self.silence();
                self.report_audio_block();
            }
            Verdict::Pending => {}
        }
    }

    /// Say which device the sound would have gone to, and what to do about it.
    ///
    /// Written as a block rather than a status-line notice on purpose: it names
    /// a device the reader has to recognise and offers two commands, and a
    /// message that disappears after four seconds cannot do either.
    fn report_audio_block(&mut self) {
        let Some(gate) = self.gate.as_ref() else {
            return;
        };
        let lines = match gate.verdict().clone() {
            Verdict::Blocked { device } => vec![
                tf(
                    "dev.muted_device",
                    &[&device.label(), &self.cfg.tts.output.rules()],
                ),
                tf("dev.muted_fix", &[&quoted(&device.name)]),
            ],
            Verdict::Unknown { reason } => vec![
                tf("dev.muted_unknown", &[&reason]),
                t("dev.muted_fix_plain").to_string(),
                t("dev.probe_hint").to_string(),
            ],
            Verdict::Pending => vec![t("dev.muted_fix_plain").to_string()],
            Verdict::Allowed { .. } => return,
        };
        self.sb.push(Block::Event(Event::Warning {
            title: t("dev.muted_title").to_string(),
            lines,
        }));
    }

    /// `/device [allow <n|name> | deny <n|name> | any | refresh]`
    fn device_command(&mut self, arg: &str) {
        let (verb, value) = match arg.split_once(char::is_whitespace) {
            Some((verb, value)) => (verb, value.trim()),
            None => (arg, ""),
        };
        match verb {
            "" | "list" | "ls" => self.show_devices(),
            "refresh" => {
                if let Some(gate) = self.gate.as_mut() {
                    gate.refresh_blocking();
                }
                self.show_devices();
            }
            "any" | "off" | "clear" => {
                self.cfg.tts.output.allow.clear();
                let _ = self.cfg.save();
                self.gate = None;
                self.system(t("dev.any"));
            }
            "allow" | "deny" => {
                let Some(name) = self.resolve_device(value) else {
                    return;
                };
                if verb == "allow" {
                    if self.cfg.tts.output.allow_device(&name) {
                        self.system(&tf("dev.allowed_added", &[&name]));
                    } else {
                        self.system(&tf("dev.allowed_exists", &[&name]));
                    }
                } else {
                    let before = self.cfg.tts.output.allow.len();
                    self.cfg
                        .tts
                        .output
                        .allow
                        .retain(|rule| !rule.eq_ignore_ascii_case(&name));
                    if self.cfg.tts.output.allow.len() == before {
                        self.system(&tf("dev.deny_missing", &[&name]));
                        return;
                    }
                    self.system(&tf("dev.denied", &[&name]));
                }
                let _ = self.cfg.save();
                self.apply_whitelist();
                self.show_devices();
            }
            _ => self.system(t("dev.usage")),
        }
    }

    /// Turn `2` or a device name into a whitelist rule.
    fn resolve_device(&mut self, value: &str) -> Option<String> {
        if value.is_empty() {
            self.system(t("dev.usage"));
            return None;
        }
        let Ok(index) = value.parse::<usize>() else {
            return Some(value.to_string());
        };
        match device::list(&self.cfg.tts.output.query) {
            Ok(devices) => match devices.get(index.saturating_sub(1)) {
                Some(device) => Some(device.name.clone()),
                None => {
                    self.system(&tf("dev.no_such", &[&index]));
                    None
                }
            },
            Err(err) => {
                self.report_probe_failure(&err);
                None
            }
        }
    }

    /// A machine with no queryable audio stack — a headless Linux box, say —
    /// still has to hear about the consequence: the whitelist keeps muting,
    /// because an output that cannot be named cannot be trusted. Saying only
    /// "cannot list audio outputs" would leave the reader stuck.
    fn report_probe_failure(&mut self, err: &anyhow::Error) {
        self.system(&tf("dev.probe_failed", &[&format!("{err:#}")]));
        self.system(t("dev.probe_hint"));
        if self.cfg.tts.output.is_active() {
            self.system(t("dev.probe_muted"));
        }
    }

    /// Rebuild the gate after the whitelist changed, keeping the verdict fresh
    /// without waiting for the next poll.
    fn apply_whitelist(&mut self) {
        if !self.cfg.tts.output.is_active() {
            self.gate = None;
            return;
        }
        match self.gate.as_mut() {
            Some(gate) => gate.set_allow(self.cfg.tts.output.allow.clone()),
            None => {
                let mut gate = Gate::new(self.cfg.tts.output.clone());
                gate.refresh_blocking();
                self.gate = Some(gate);
            }
        }
    }

    /// The output list, with the current device and the whitelist marked.
    fn show_devices(&mut self) {
        let devices = match device::list(&self.cfg.tts.output.query) {
            Ok(devices) => devices,
            Err(err) => {
                self.report_probe_failure(&err);
                return;
            }
        };
        let allow = self.cfg.tts.output.allow.clone();
        let mut lines = Vec::new();
        for (index, dev) in devices.iter().enumerate() {
            let mut line = format!("{:>2}. {}", index + 1, dev.label());
            if dev.is_default {
                line.push_str(t("dev.current"));
            }
            if device::allowed(dev, &allow) && !allow.is_empty() {
                line.push_str(t("dev.allowed_mark"));
            }
            lines.push(line);
        }
        lines.push(String::new());
        lines.push(if allow.is_empty() {
            t("dev.whitelist_empty").to_string()
        } else {
            tf("dev.whitelist", &[&self.cfg.tts.output.rules()])
        });
        lines.push(t("dev.hint").to_string());
        self.sb.push(Block::Event(Event::Warning {
            title: t("dev.title").to_string(),
            lines,
        }));
    }

    // ── read aloud ───────────────────────────────────────────────────────────

    /// Engine name while a sentence is actually sounding, for the status line.
    fn speaking_engine(&self) -> Option<&str> {
        match (&self.speaker, self.lit) {
            (Some(speaker), Some(_)) => Some(speaker.engine()),
            _ => None,
        }
    }

    /// Bring up the speech worker, or explain why it cannot start.
    fn start_speaker(&mut self) -> bool {
        if self.speaker.is_some() {
            return true;
        }
        let Some(spec) = self.cfg.active_engine().cloned() else {
            let names = self.cfg.engine_names().join(", ");
            self.system(&tf("tts.unknown_engine", &[&self.cfg.tts.engine, &names]));
            return false;
        };
        let synth = CommandSynth::new(
            &self.cfg.tts.engine,
            spec,
            self.cfg.active_voice(),
            self.cfg.tts.rate,
        );
        if !synth.is_available() {
            let program = synth.program().unwrap_or_default();
            let config = paths::display(&paths::config_file());
            self.system(&tf("tts.missing_binary", &[&program, &config]));
            return false;
        }
        self.speaker = Some(Speaker::spawn(Box::new(synth), paths::speech_dir()));
        true
    }

    /// Hand a passage to the speech worker, one sentence at a time.
    fn enqueue_speech(&mut self, id: u64, text: &str) {
        if !self.cfg.tts.enabled || !self.audio_permitted() {
            return;
        }
        if !self.start_speaker() {
            self.cfg.tts.enabled = false;
            return;
        }
        let Some(speaker) = self.speaker.as_mut() else {
            return;
        };
        let sentences = sentence::split(text);
        if sentences.is_empty() {
            return;
        }
        for utterance in sentences {
            speaker.speak(id, &utterance.text, utterance.range);
        }
        self.voiced = Some((id, text.to_string()));
    }

    /// Drain speech events: move the highlight and pace the text to the audio.
    fn pump_speech(&mut self) {
        let events = match self.speaker.as_mut() {
            Some(speaker) => speaker.poll(),
            None => return,
        };
        for event in events {
            match event {
                SpeechEvent::Started {
                    id,
                    range,
                    chars,
                    ms,
                } => {
                    if self.sb.set_speaking(id, Some(Speaking::new(range))) {
                        self.lit = Some(id);
                    }
                    self.cursor = self.build_cursor(id, range, ms);
                    // Reveal as fast as the clip is long, so the text lands on
                    // the last syllable instead of racing ahead of it.
                    if ms > 0 && chars > 0 {
                        let cps = (chars as f32 / (ms as f32 / 1000.0)).clamp(4.0, 400.0);
                        self.speech_cps = Some(cps);
                        if !self.rushing {
                            self.turn.set_cps(cps);
                        }
                    }
                }
                SpeechEvent::Finished { .. } => {
                    if let Some(cursor) = self.cursor.as_mut() {
                        cursor.done = true;
                    }
                }
                SpeechEvent::Idle => self.idle = true,
                SpeechEvent::Failed { message } => {
                    self.system(&tf("tts.failed", &[&message]));
                    self.system(t("tts.fallback"));
                    self.cfg.tts.enabled = false;
                    self.speaker = None;
                    self.speech_done();
                }
            }
        }
    }

    /// Words of the sentence being spoken, in passage coordinates.
    ///
    /// `None` when the passage is gone or the range no longer fits it — which
    /// happens if a passage is still streaming while its audio plays.
    fn build_cursor(&self, id: u64, range: (usize, usize), ms: u64) -> Option<Cursor> {
        let (voiced_id, text) = self.voiced.as_ref()?;
        if *voiced_id != id || range.1 > text.len() || range.0 >= range.1 {
            return None;
        }
        if !text.is_char_boundary(range.0) || !text.is_char_boundary(range.1) {
            return None;
        }
        let slice = &text[range.0..range.1];
        let units: Vec<Unit> = sentence::units(slice)
            .into_iter()
            .map(|unit| Unit {
                range: (unit.range.0 + range.0, unit.range.1 + range.0),
                chars: unit.chars,
            })
            .collect();
        if units.is_empty() || ms == 0 {
            return None;
        }
        Some(Cursor {
            id,
            sentence: range,
            units,
            started: Instant::now(),
            ms,
            shown: None,
            done: false,
        })
    }

    /// Walk the word highlight along with the audio. Called once per frame.
    fn advance_cursor(&mut self) {
        let Some(cursor) = self.cursor.as_mut() else {
            // Nothing playing: honour a pending "everything has been said".
            if self.idle {
                self.idle = false;
                self.speech_done();
            }
            return;
        };
        // A clip that has played out: drop the word, keep the sentence lit until
        // the next one starts, and only then admit to being idle.
        if cursor.done && cursor.started.elapsed().as_millis() as u64 >= cursor.ms {
            let (id, sentence) = (cursor.id, cursor.sentence);
            self.cursor = None;
            self.sb.set_speaking(id, Some(Speaking::new(sentence)));
            if self.idle {
                self.idle = false;
                self.speech_done();
            }
            return;
        }
        let progress = cursor.started.elapsed().as_millis() as f32 / cursor.ms as f32;
        let Some(unit) = sentence::unit_at(&cursor.units, progress) else {
            return;
        };
        if cursor.shown == Some(unit.range) {
            return;
        }
        cursor.shown = Some(unit.range);
        let state = Speaking {
            sentence: cursor.sentence,
            word: Some(unit.range),
        };
        let id = cursor.id;
        self.sb.set_speaking(id, Some(state));
    }

    /// Speech stopped: clear the highlight and go back to timed reading.
    fn speech_done(&mut self) {
        self.sb.clear_speaking();
        self.cursor = None;
        self.idle = false;
        self.lit = None;
        if self.speech_cps.take().is_some() && !self.rushing {
            self.turn.set_cps(self.base_cps);
        }
    }

    /// Ctrl+S and `/tts`: flip read-aloud on or off.
    fn toggle_speech(&mut self) {
        if self.cfg.tts.enabled {
            self.cfg.tts.enabled = false;
            self.silence();
            self.notice(t("tts.off"));
        } else {
            self.cfg.tts.enabled = true;
            if self.start_speaker() {
                let engine = self.speech_label();
                self.notice(&tf("tts.on", &[&engine]));
                // Turning speech on while routed to the wrong output should not
                // look like it worked.
                if !self.audio_permitted() {
                    self.report_audio_block();
                }
            } else {
                self.cfg.tts.enabled = false;
            }
        }
    }

    /// `engine · voice`, as shown when speech turns on.
    fn speech_label(&self) -> String {
        let voice = self.cfg.active_voice();
        if voice.is_empty() {
            self.cfg.tts.engine.clone()
        } else {
            format!("{} · {}", self.cfg.tts.engine, voice)
        }
    }

    /// Stop the current clip and forget everything queued.
    fn silence(&mut self) {
        if let Some(speaker) = self.speaker.as_mut() {
            speaker.stop();
        }
        self.speech_done();
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let th = theme();
        UiBlock::default()
            .style(Style::default().bg(th.bg_base).fg(th.text_primary))
            .render(area, frame.buffer_mut());

        let prompt_h = self.prompt.height(area.width);
        let [header, body, prompt, status] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(prompt_h),
            Constraint::Length(1),
        ])
        .areas(area);

        // The reading position only advances when a turn commits, so normalise
        // for display: a finished chapter should read as the next one.
        let shown = match self.book.as_ref() {
            Some(book) => flow::normalize(book, self.pos),
            None => Pos::default(),
        };
        let speaking = self.speaking_engine().map(str::to_string);
        let chrome = Chrome {
            book: self
                .book
                .as_ref()
                .map_or_else(|| t("chrome.library"), |b| b.title.as_str()),
            author: self.book.as_ref().and_then(|b| b.author.as_deref()),
            chapter_title: self
                .book
                .as_ref()
                .and_then(|b| b.chapter(shown.chapter))
                .map_or("—", |c| c.title.as_str()),
            chapter_index: self
                .book
                .as_ref()
                .map_or(0, |b| shown.chapter.min(b.chapters.len().saturating_sub(1))),
            chapter_total: self.book.as_ref().map_or(0, |b| b.chapters.len()),
            progress: self
                .book
                .as_ref()
                .map_or(0.0, |b| b.progress(self.pos.chapter, self.pos.para)),
            cps: self.turn.cps(),
            session_chars: self.turn.session_chars,
            busy: self.turn.busy(),
            tick: self.tick,
            notice: self.notice.as_ref().map(|(m, _)| m.as_str()),
            scrolled: self.sb.scrolled_away(),
            scroll_percent: self.sb.scroll_percent(),
            has_book: self.book.is_some(),
            library_count: self.library.len(),
            elapsed: self.started.elapsed(),
            speaking: speaking.as_deref(),
            audio_muted: self.cfg.tts.enabled && !self.audio_permitted(),
        };

        let buf = frame.buffer_mut();
        chrome::render_header(header, buf, &chrome);
        self.sb.render(inset(body), buf, self.tick);
        self.prompt.render(prompt, buf, self.turn.busy());
        chrome::render_status(status, buf, &chrome);
        if self.show_help {
            chrome::render_help(area, buf);
        }
    }

    // ── input ────────────────────────────────────────────────────────────────

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        // Any key dismisses the help overlay, but a quit request still counts:
        // being unable to leave because a panel is open would be maddening.
        if self.show_help {
            self.show_help = false;
            let quitting = ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d'));
            if !quitting {
                return;
            }
        }
        // Any key other than Ctrl+C resets the quit confirmation.
        if !(ctrl && matches!(key.code, KeyCode::Char('c'))) {
            self.ctrl_c_at = None;
        }

        match (key.code, ctrl) {
            (KeyCode::Char('c'), true) => self.on_ctrl_c(),
            (KeyCode::Char('d'), true) => self.quit = true,
            (KeyCode::Char('l'), true) => {
                self.sb.clear();
                self.notice(t("cmd.cleared"));
            }
            (KeyCode::Char('t'), true) => {
                self.sb.toggle_thinking_fold();
                self.notice(t("cmd.thinking_toggled"));
            }
            (KeyCode::Char('o'), true) => {
                self.sb.toggle_tool_fold();
                self.notice(if self.sb.expand_tools {
                    t("cmd.tools_expanded")
                } else {
                    t("cmd.tools_folded")
                });
            }
            (KeyCode::Char('s'), true) => self.toggle_speech(),
            (KeyCode::Char('p'), true) => self.prompt.history_prev(),
            (KeyCode::Char('n'), true) => self.prompt.history_next(),
            (KeyCode::Char('u'), true) => self.prompt.kill_to_start(),
            (KeyCode::Char('w'), true) => self.prompt.kill_word(),
            (KeyCode::Char('a'), true) => self.prompt.home(),
            (KeyCode::Char('e'), true) => self.prompt.end(),
            (KeyCode::Char(c), false) => self.prompt.insert_char(c),

            (KeyCode::Enter, _) => self.on_enter(),
            (KeyCode::Esc, _) => self.on_escape(),
            (KeyCode::Backspace, _) => self.prompt.backspace(),
            (KeyCode::Delete, _) => self.prompt.delete(),
            (KeyCode::Left, _) => self.prompt.left(),
            (KeyCode::Right, _) => self.prompt.right(),
            (KeyCode::Home, _) => {
                if self.prompt.is_empty() {
                    self.sb.to_top();
                } else {
                    self.prompt.home();
                }
            }
            (KeyCode::End, _) => {
                if self.prompt.is_empty() {
                    self.sb.to_bottom();
                } else {
                    self.prompt.end();
                }
            }
            (KeyCode::Up, _) => self.sb.scroll_lines(-2),
            (KeyCode::Down, _) => self.sb.scroll_lines(2),
            (KeyCode::PageUp, _) => self.sb.page(-1),
            (KeyCode::PageDown, _) => self.sb.page(1),
            _ => {}
        }
    }

    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.sb.scroll_lines(-3),
            MouseEventKind::ScrollDown => self.sb.scroll_lines(3),
            _ => {}
        }
    }

    pub fn on_paste(&mut self, text: String) {
        self.prompt.insert_str(&text);
    }

    fn on_ctrl_c(&mut self) {
        if self.turn.busy() {
            self.interrupt();
            return;
        }
        if !self.prompt.is_empty() {
            self.prompt.clear();
            return;
        }
        match self.ctrl_c_at {
            Some(at) if at.elapsed() < QUIT_WINDOW => self.quit = true,
            _ => {
                self.ctrl_c_at = Some(Instant::now());
                self.notice(t("cmd.quit_again"));
            }
        }
    }

    fn on_escape(&mut self) {
        if self.turn.busy() {
            self.interrupt();
        } else if !self.prompt.is_empty() {
            self.prompt.clear();
        } else if self.sb.scrolled_away() {
            self.sb.to_bottom();
        }
    }

    fn interrupt(&mut self) {
        self.auto = false;
        self.silence();
        if self.turn.interrupt(&mut self.sb) {
            self.sb.push(Block::Event(Event::Interrupted));
        }
        if self.rushing {
            self.turn.set_cps(self.base_cps);
            self.rushing = false;
        }
        self.save_progress();
    }

    fn on_enter(&mut self) {
        if self.prompt.is_empty() {
            // Enter during a turn means "get on with it".
            if self.turn.busy() {
                if !self.rushing {
                    self.rushing = true;
                    self.turn.set_cps(RUSH_CPS);
                }
                return;
            }
            self.sb.to_bottom();
            if self.book.is_some() {
                self.read_more();
            } else if let Some(index) = self.resume {
                self.open_entry(index);
            } else if self.library.is_empty() {
                self.system(t("cmd.library_empty"));
            } else {
                self.system(t("cmd.pick_first"));
                self.show_library();
            }
            return;
        }
        let line = self.prompt.take();
        self.sb.push(Block::User(line.clone()));
        self.sb.to_bottom();

        if let Some(rest) = line.strip_prefix('/') {
            self.command(rest.trim());
            return;
        }
        // A bare number picks from the library listing.
        if let Ok(index) = line.trim().parse::<usize>()
            && self.library.get(index).is_some()
        {
            self.open_entry(index);
            return;
        }
        match self.book.as_ref() {
            Some(book) => {
                let steps = flow::answer(book, &line);
                self.turn.enqueue(steps);
            }
            None => {
                self.system(t("cmd.no_book"));
                self.show_library();
            }
        }
    }

    // ── actions ──────────────────────────────────────────────────────────────

    fn read_more(&mut self) {
        let Some(book) = self.book.as_ref() else {
            self.system(t("cmd.no_book"));
            return;
        };
        if self.at_end() {
            self.sb.push(Block::Event(Event::BookComplete));
            self.auto = false;
            return;
        }
        let (steps, _next) = flow::continue_reading(book, self.pos, self.resumed);
        self.resumed = false;
        self.turn.enqueue(steps);
    }

    fn at_end(&self) -> bool {
        let Some(book) = self.book.as_ref() else {
            return true;
        };
        let last = book.chapters.len().saturating_sub(1);
        self.pos.chapter > last
            || (self.pos.chapter == last
                && self.pos.para >= book.chapter(last).map(|c| c.paras.len()).unwrap_or(0))
    }

    fn command(&mut self, line: &str) {
        let (cmd, arg) = match line.split_once(char::is_whitespace) {
            Some((c, a)) => (c, a.trim()),
            None => (line, ""),
        };
        match cmd {
            "" | "help" | "h" | "?" => self.show_help = true,
            "quit" | "q" | "exit" => self.quit = true,

            // ── library ──
            "lib" | "library" | "ls" => self.show_library(),
            "import" | "i" => {
                if arg.is_empty() {
                    self.system(t("cmd.usage_import"));
                } else {
                    self.import(arg);
                }
            }
            "open" | "o" => match arg.parse::<usize>() {
                Ok(index) => self.open_entry(index),
                Err(_) if arg.is_empty() => {
                    self.system(t("cmd.usage_open"));
                }
                Err(_) => self.import(arg),
            },
            "forget" => match arg.parse::<usize>() {
                Ok(index) => self.forget(index),
                Err(_) => self.system(t("cmd.usage_forget")),
            },
            "sample" => {
                let book = crate::book::sample::book();
                self.adopt(book);
                self.notice(t("cmd.sample_opened"));
            }

            // ── reading ──
            "toc" => {
                if let Some(book) = self.book.as_ref() {
                    let steps = flow::toc(book, self.pos.chapter);
                    self.turn.enqueue(steps);
                } else {
                    self.no_book();
                }
            }
            "plan" => {
                if let Some(book) = self.book.as_ref() {
                    let steps = flow::plan(book, self.pos);
                    self.turn.enqueue(steps);
                } else {
                    self.no_book();
                }
            }
            "context" | "ctx" => {
                if let Some(book) = self.book.as_ref() {
                    let args = flow::ContextArgs {
                        session_chars: self.turn.session_chars,
                        cps: self.turn.cps(),
                        elapsed: self.started.elapsed(),
                        engine: match (&self.speaker, self.cfg.tts.enabled) {
                            (Some(speaker), true) => Some(speaker.engine()),
                            _ => None,
                        },
                    };
                    let steps = flow::context(book, self.pos, args);
                    self.turn.enqueue(steps);
                } else {
                    self.no_book();
                }
            }
            "find" | "grep" | "search" => match (self.book.as_ref(), arg.is_empty()) {
                (None, _) => self.no_book(),
                (Some(_), true) => self.system(t("cmd.usage_find")),
                (Some(book), false) => {
                    let steps = flow::answer(book, arg);
                    self.turn.enqueue(steps);
                }
            },
            "goto" | "g" => {
                let total = self.book.as_ref().map_or(0, |b| b.chapters.len());
                match arg.parse::<usize>() {
                    Ok(n) if total > 0 && n >= 1 && n <= total => self.jump(n - 1),
                    _ if total == 0 => self.no_book(),
                    _ => self.system(&tf("cmd.usage_goto", &[&total])),
                }
            }
            "next" | "n" => {
                let total = self.book.as_ref().map_or(0, |b| b.chapters.len());
                if total == 0 {
                    self.no_book();
                } else if self.pos.chapter + 1 < total {
                    self.jump(self.pos.chapter + 1);
                } else {
                    self.system(t("cmd.last_chapter"));
                }
            }
            "prev" | "p" => {
                if self.book.is_none() {
                    self.no_book();
                } else if self.pos.chapter == 0 {
                    self.system(t("cmd.first_chapter"));
                } else {
                    self.jump(self.pos.chapter - 1);
                }
            }
            "speed" | "cps" => match arg.parse::<f32>() {
                Ok(v) if (4.0..=4000.0).contains(&v) => {
                    self.base_cps = v;
                    self.turn.set_cps(v);
                    self.cfg.reading.speed = v;
                    let _ = self.cfg.save();
                    self.system(&tf("cmd.speed_set", &[&format!("{v:.0}")]));
                }
                _ => self.system(t("cmd.usage_speed")),
            },
            "auto" => {
                if self.book.is_none() {
                    self.no_book();
                    return;
                }
                self.auto = !self.auto;
                if self.auto {
                    self.system(t("cmd.auto_on"));
                    if !self.turn.busy() {
                        self.read_more();
                    }
                } else {
                    self.system(t("cmd.auto_off"));
                }
            }
            "progress" => match self.book.as_ref() {
                None => self.no_book(),
                Some(book) => {
                    let read = book.chars_before(self.pos.chapter, self.pos.para);
                    let text = tf(
                        "cmd.progress",
                        &[
                            &book.title,
                            &format!(
                                "{:.1}",
                                book.progress(self.pos.chapter, self.pos.para) * 100.0
                            ),
                            &human(read),
                            &human(book.char_count()),
                            &(self.pos.chapter + 1),
                            &(self.pos.para + 1),
                        ],
                    );
                    self.system(&text);
                }
            },
            // ── read aloud ──
            "tts" | "speak" | "read" => self.tts_command(arg),
            "voice" => self.voice_command(arg),
            "rate" => self.rate_command(arg),
            "lang" | "language" => self.lang_command(arg),
            "device" | "devices" | "audio" | "output" => self.device_command(arg),

            other => self.system(&tf("cmd.unknown", &[&other])),
        }
    }

    /// `/tts [on | off | <engine> | test | config]`
    fn tts_command(&mut self, arg: &str) {
        match arg {
            "" => self.toggle_speech(),
            "on" => {
                if !self.cfg.tts.enabled {
                    self.toggle_speech();
                } else {
                    let engine = self.speech_label();
                    self.notice(&tf("tts.on", &[&engine]));
                }
            }
            "off" => {
                if self.cfg.tts.enabled {
                    self.toggle_speech();
                } else {
                    self.notice(t("tts.off"));
                }
            }
            "config" => {
                let path = paths::display(&paths::config_file());
                self.system(&tf("tts.config_at", &[&path]));
                self.system(&tf("tts.engines", &[&self.cfg.engine_names().join(", ")]));
            }
            "test" => {
                let line = t("tts.test_line");
                self.system(&tf("tts.testing", &[&line]));
                self.cfg.tts.enabled = true;
                if self.start_speaker() {
                    let id = self.sb.push(Block::passage_text(line));
                    self.enqueue_speech(id, line);
                } else {
                    self.cfg.tts.enabled = false;
                }
            }
            "list" | "engines" => {
                self.system(&tf("tts.engines", &[&self.cfg.engine_names().join(", ")]));
            }
            name if self.cfg.spec(name).is_some() => {
                self.silence();
                self.speaker = None;
                self.cfg.tts.engine = name.to_string();
                self.cfg.tts.enabled = true;
                let _ = self.cfg.save();
                if self.start_speaker() {
                    let engine = self.speech_label();
                    self.system(&tf("tts.engine_set", &[&engine]));
                } else {
                    self.cfg.tts.enabled = false;
                }
            }
            other if other.starts_with('-') || other.starts_with('/') => {
                self.system(t("tts.usage"));
            }
            other => {
                let names = self.cfg.engine_names().join(", ");
                self.system(&tf("tts.unknown_engine", &[&other, &names]));
            }
        }
    }

    /// `/voice <name>` — engines name their voices differently, so this is free
    /// text handed straight to the engine.
    fn voice_command(&mut self, arg: &str) {
        if arg.is_empty() {
            self.system(t("tts.usage_voice"));
            return;
        }
        self.cfg.tts.voice = arg.to_string();
        let _ = self.cfg.save();
        self.silence();
        self.speaker = None;
        self.system(&tf("tts.voice_set", &[&arg]));
    }

    /// `/rate <0.5-3.0>` — speech tempo, which then drives the reveal speed.
    fn rate_command(&mut self, arg: &str) {
        match arg.parse::<f32>() {
            Ok(v) if (0.5..=3.0).contains(&v) => {
                self.cfg.tts.rate = v;
                let _ = self.cfg.save();
                self.silence();
                self.speaker = None;
                self.system(&tf("tts.rate_set", &[&format!("{v:.2}")]));
            }
            _ => self.system(t("tts.usage_rate")),
        }
    }

    /// `/lang zh | en | auto`
    fn lang_command(&mut self, arg: &str) {
        match arg {
            "" => self.system(t("cmd.usage_lang")),
            other => match crate::i18n::Lang::parse(other) {
                Some(lang) => {
                    crate::i18n::set(lang);
                    self.cfg.language = lang;
                    let _ = self.cfg.save();
                    // Cached lines were rendered in the old language.
                    self.sb.invalidate();
                    self.system(&tf("cmd.lang_set", &[&lang.code()]));
                }
                None => self.system(t("cmd.usage_lang")),
            },
        }
    }

    fn jump(&mut self, chapter: usize) {
        self.pos = Pos { chapter, para: 0 };
        self.resumed = false;
        self.save_progress();
        let title = self
            .book
            .as_ref()
            .and_then(|b| b.chapter(chapter))
            .map(|c| c.title.clone())
            .unwrap_or_default();
        self.system(&tf("cmd.jumped", &[&(chapter + 1), &title]));
        self.read_more();
    }

    /// Open library entry `index` (one-based, as shown in the listing).
    fn open_entry(&mut self, index: usize) {
        let Some(entry) = self.library.get(index).cloned() else {
            self.system(&tf("lib.no_such_entry", &[&index, &self.library.len()]));
            return;
        };
        // A copied Markdown book left its pictures behind; the origin is still
        // the right place to look for them.
        let assets = entry
            .origin
            .as_deref()
            .and_then(|origin| origin.parent())
            .map(|dir| dir.to_path_buf());
        match Book::load_from(Some(&entry.path), assets.as_deref()) {
            Ok(book) => {
                self.adopt(book);
                self.notice(&tf("lib.opened", &[&entry.title]));
            }
            Err(err) => {
                self.sb.push(Block::Event(Event::Failed {
                    message: tf(
                        "lib.open_failed",
                        &[
                            &entry.title,
                            &paths::display(&entry.path),
                            &format!("{err:#}"),
                        ],
                    ),
                }));
            }
        }
    }

    /// Import a path into the library, then open it. `arg` may carry a mode flag.
    fn import(&mut self, arg: &str) {
        let (path, mode) = parse_import_arg(arg);
        if path.as_os_str().is_empty() {
            self.system(t("cmd.usage_import"));
            return;
        }
        match self.library.import(&path, mode) {
            Ok((entry, book)) => {
                let index = self
                    .library
                    .entries
                    .iter()
                    .position(|e| e.id == entry.id)
                    .map_or(0, |i| i + 1);
                let note = tf(
                    "lib.import_note",
                    &[
                        &mode.describe(),
                        &entry.title,
                        &paths::display(&entry.path),
                        &index,
                        &human(entry.chars),
                    ],
                );
                self.system(&note);
                self.adopt(book);
            }
            Err(err) => {
                self.sb.push(Block::Event(Event::Failed {
                    message: tf("lib.import_failed", &[&format!("{err:#}")]),
                }));
            }
        }
    }

    fn forget(&mut self, index: usize) {
        let open_id = self.book.as_ref().map(|b| b.id.clone());
        match self.library.forget(index) {
            Ok(entry) => {
                if open_id.as_deref() == Some(entry.id.as_str()) {
                    self.book = None;
                    self.pos = Pos::default();
                }
                let tail = match entry.mode {
                    Mode::Copy => t("lib.forget_copy"),
                    Mode::Link => t("lib.forget_link"),
                    Mode::Move => t("lib.forget_move"),
                };
                self.system(&tf("lib.forgotten", &[&entry.title, &tail]));
                self.show_library();
            }
            Err(err) => self.system(&format!("{err}")),
        }
    }

    fn no_book(&mut self) {
        self.system(t("cmd.no_book"));
    }

    fn system(&mut self, text: &str) {
        self.sb.push(Block::System(text.to_string()));
    }

    fn notice(&mut self, text: &str) {
        self.notice = Some((text.to_string(), Instant::now()));
    }

    pub fn save_progress(&mut self) {
        let Some(book) = self.book.as_ref() else {
            return;
        };
        let id = book.id.clone();
        let previous = self.store.get(&id).cloned().unwrap_or_default();
        let progress = Progress {
            title: book.title.clone(),
            path: book.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            chapter: self.pos.chapter,
            para: self.pos.para,
            chars_read: book.chars_before(self.pos.chapter, self.pos.para) as u64,
            sessions: previous.sessions.max(1),
            updated: now_secs(),
        };
        self.store.record(&id, progress);
        let _ = self.store.save();
    }

    /// Count a new session for the open book, so the store shows real usage.
    fn begin_session(&mut self) {
        let Some(book) = self.book.as_ref() else {
            return;
        };
        let id = book.id.clone();
        let mut progress = self.store.get(&id).cloned().unwrap_or_default();
        progress.title = book.title.clone();
        progress.sessions += 1;
        progress.chapter = self.pos.chapter;
        progress.para = self.pos.para;
        progress.updated = now_secs();
        progress.path = book.path.as_ref().map(|p| p.to_string_lossy().into_owned());
        self.store.record(&id, progress);
        let _ = self.store.save();
    }
}

/// One column of breathing room on each side of the scrollback.
fn inset(area: Rect) -> Rect {
    Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(2),
        height: area.height,
    }
}

/// Quote a device name for a command hint, so a name with spaces can be pasted
/// straight back in.
fn quoted(name: &str) -> String {
    if name.contains(char::is_whitespace) {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

/// Split `<path> [-c|-l|-m]` — in either order — into a path and a mode.
pub fn parse_import_arg(arg: &str) -> (PathBuf, Mode) {
    let mut mode = Mode::default();
    let mut parts: Vec<&str> = Vec::new();
    for token in arg.split_whitespace() {
        match Mode::parse(token) {
            Some(found) => mode = found,
            None => parts.push(token),
        }
    }
    // Paths with spaces are common enough to be worth rejoining.
    (expand_tilde(&parts.join(" ")), mode)
}

/// Import a path and return the book plus a line describing what happened.
///
/// Called before the TUI starts, so a bad path fails on the command line with a
/// readable message instead of inside a full-screen app.
pub fn import_and_open(path: &str, mode: Mode, library: &mut Library) -> Result<(Book, String)> {
    let (entry, book) = library.import(&expand_tilde(path), mode)?;
    let note = tf(
        "lib.imported",
        &[
            &mode.describe(),
            &entry.title,
            &paths::display(&entry.path),
            &human(entry.chars),
            &entry.chapters,
        ],
    );
    Ok((book, note))
}
