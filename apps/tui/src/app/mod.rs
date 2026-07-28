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
use crate::effort::{self, Effort};
use crate::i18n::{t, tf};
use crate::library::{Library, Mode};
// The library's `Mode` is how a file is held; this one is how the reader moves.
use crate::mode::Mode as ReadMode;
use crate::paths;
use crate::store::{Progress, Store, now_secs};
use crate::theme::theme;
use crate::tts::device::{self, Gate, Verdict};
use crate::tts::sentence::Unit;
use crate::tts::{Speaker, SpeechEvent, command::CommandSynth, sentence};
use crate::ui::block::{Block, Event, Highlight, LibraryRow};
use crate::ui::chrome::{self, Chrome};
use crate::ui::menu;
use crate::ui::prompt::Prompt;
use crate::ui::scrollback::Scrollback;

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
    /// Modes whose full explanation has already been printed this session, so
    /// that cycling with shift+tab does not reprint the same paragraph.
    explained: [bool; 3],
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
    /// The last search, kept so its hits can be walked and lit up.
    find: Option<Find>,
    /// Row highlighted in the slash-command menu. The list itself is derived
    /// from what is typed, so only the cursor has to be remembered.
    menu_at: usize,
    /// Term to highlight in the next passage that contains it, set when a jump
    /// lands on a hit.
    marking: Option<String>,
}

/// A search and where the reader is inside its results.
///
/// Kept after the turn that produced it, because a list of locations the reader
/// cannot walk is a list they have to transcribe by hand.
struct Find {
    needle: String,
    hits: crate::book::Hits,
    /// Zero-based index into `hits.shown`.
    at: usize,
}

/// A short name for an unnamed mark: the first few words of what is there.
///
/// A list of bookmarks that all say "bookmark" is a list nobody can read.
fn opening_words(book: &Book, pos: Pos) -> String {
    let text = book
        .chapter(pos.chapter)
        .and_then(|chapter| chapter.paras.get(pos.para))
        .map(|para| para.text().trim().to_string())
        .unwrap_or_default();
    if text.is_empty() {
        return book
            .chapter(pos.chapter)
            .map(|chapter| chapter.title.clone())
            .unwrap_or_default();
    }
    let mut out: String = text.chars().take(28).collect();
    if text.chars().count() > 28 {
        out.push('…');
    }
    out
}

/// Where a stored position lands in this copy of the book.///
/// The stored chapter and paragraph are indices into however the book was cut
/// into chapters the last time it was opened, and that can change under the
/// reader's feet: a release that learns to honour a table of contents turns
/// thirteen chapters into seventy-one, and index 3 is suddenly a different page.
/// The character offset does not move, so it decides whenever the two disagree.
fn restore(book: &Book, saved: &Progress) -> Pos {
    let chapter = saved.chapter.min(book.chapters.len().saturating_sub(1));
    let by_index = Pos {
        chapter,
        para: saved.para,
    };
    if saved.chars_read == 0
        || book.chars_before(by_index.chapter, by_index.para) as u64 == saved.chars_read
    {
        return by_index;
    }
    let (chapter, para) = book.locate(saved.chars_read as usize);
    Pos { chapter, para }
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
        // The pacer starts at the pace the effort level asks for, not the base.
        let paced = (cps * cfg.effort.multipliers.get(cfg.effort.level)).clamp(4.0, 4000.0);
        let mut app = Self {
            sb: Scrollback::new(),
            prompt: Prompt::new(),
            turn: Turn::new(paced),
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
            explained: [false; 3],
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
            find: None,
            marking: None,
            menu_at: 0,
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
            .map(|p| restore(&book, p))
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
    /// Where the reader is: chapter and paragraph, both zero-based.
    pub fn position(&self) -> (usize, usize) {
        (self.pos.chapter, self.pos.para)
    }

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
            self.mark_match(id, &text);
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
            self.rushing = false;
            self.apply_pace();
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

    // ── pace ─────────────────────────────────────────────────────────────────

    /// What the current effort level is worth, as a multiplier.
    ///
    /// One number for both worlds: the reveal speed is scaled by it and the voice
    /// plays at it, so `xhigh` reads slowly whether readio is typing or speaking.
    pub fn multiplier(&self) -> f32 {
        self.cfg.effort.multipliers.get(self.cfg.effort.level)
    }

    /// Characters per second the pacer should run at, effort included.
    fn reveal_cps(&self) -> f32 {
        (self.base_cps * self.multiplier()).clamp(4.0, 4000.0)
    }

    /// Hand the pacer the current pace, unless a rush or a clip owns it.
    fn apply_pace(&mut self) {
        if !self.rushing && self.speech_cps.is_none() {
            let cps = self.reveal_cps();
            self.turn.set_cps(cps);
        }
    }

    /// `/effort <level>` and `^r`: change the pace of everything at once.
    ///
    /// Clips are rendered at a speed rather than resampled at playback, so
    /// everything already prefetched is wrong the moment this changes. Dropping
    /// the speaker throws those away, and re-queueing from the start of the
    /// sentence that was playing means the reader hears the new pace within a
    /// sentence instead of at the next passage.
    fn set_effort(&mut self, level: Effort) {
        self.cfg.effort.level = level;
        let _ = self.cfg.save();
        self.apply_pace();

        let resume = self
            .cursor
            .as_ref()
            .map(|cursor| cursor.sentence.0)
            .filter(|_| self.cfg.tts.enabled);
        let voiced = self.voiced.clone();
        self.silence();
        self.speaker = None;
        if let (Some(from), Some((id, text))) = (resume, voiced) {
            self.enqueue_speech_from(id, &text, from);
        }

        let times = effort::times(self.multiplier());
        self.notice(&tf("effort.set", &[&level.label(), &times]));
    }

    /// `/effort` with no argument: the whole ladder, with the active level marked
    /// and each level's multiplier spelled out, since the multipliers are the
    /// reader's to change.
    fn effort_report(&mut self) {
        let level = self.cfg.effort.level;
        let rows: Vec<String> = effort::LEVELS
            .iter()
            .map(|candidate| {
                let mark = if *candidate == level { "❯" } else { " " };
                format!(
                    "{mark} {:<8} {:>6}   {}",
                    candidate.label(),
                    effort::times(self.cfg.effort.multipliers.get(*candidate)),
                    candidate.about()
                )
            })
            .collect();
        let cps = format!("{:.0}", self.reveal_cps());
        self.system(&tf(
            "effort.now",
            &[&level.label(), &effort::times(self.multiplier()), &cps],
        ));
        for row in rows {
            self.system(&row);
        }
        self.system(t("effort.tune"));
    }

    /// What the status line says while audio is playing: the engine, plus the
    /// multiplier whenever it is not 1× — the number a listener wants to glance
    /// at without opening a menu.
    fn speaking_engine(&self) -> Option<String> {
        match (&self.speaker, self.lit) {
            (Some(speaker), Some(_)) => {
                let engine = speaker.engine();
                if (self.multiplier() - 1.0).abs() < 0.001 {
                    Some(engine.to_string())
                } else {
                    Some(format!("{engine} {}", effort::times(self.multiplier())))
                }
            }
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
            self.multiplier(),
        );
        if !synth.is_available() {
            let program = synth.program().unwrap_or_default();
            let config = paths::display(&paths::config_file());
            self.system(&tf("tts.missing_binary", &[&program, &config]));
            return false;
        }
        self.speaker = Some(Speaker::spawn(
            Box::new(synth),
            paths::speech_dir(),
            self.cfg.tts.prefetch,
        ));
        true
    }

    /// Hand a passage to the speech worker, one sentence at a time.
    fn enqueue_speech(&mut self, id: u64, text: &str) {
        self.enqueue_speech_from(id, text, 0);
    }

    /// The same, but skipping everything that ends before `from`.
    ///
    /// Used when the speed changes mid-passage: the sentences already spoken
    /// stay spoken, and the one in progress starts again at the new speed.
    fn enqueue_speech_from(&mut self, id: u64, text: &str, from: usize) {
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
            if utterance.range.1 <= from {
                continue;
            }
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
                    if self.sb.set_highlight(id, Some(Highlight::new(range))) {
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
            self.sb.set_highlight(id, Some(Highlight::new(sentence)));
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
        let state = Highlight {
            sentence: cursor.sentence,
            word: Some(unit.range),
        };
        let id = cursor.id;
        self.sb.set_highlight(id, Some(state));
    }

    /// Speech stopped: clear the highlight and go back to timed reading.
    fn speech_done(&mut self) {
        self.sb.clear_highlight();
        self.cursor = None;
        self.idle = false;
        self.lit = None;
        if self.speech_cps.take().is_some() {
            self.apply_pace();
        }
    }

    /// Ctrl+S and `/tts on|off`: step in and out of read-aloud.
    ///
    /// Leaving read-aloud lands in auto-scroll rather than manual: the text was
    /// moving a moment ago, and silence is the only thing the reader asked to
    /// change.
    fn toggle_speech(&mut self) {
        if self.cfg.tts.enabled {
            self.set_mode(ReadMode::Auto);
        } else {
            self.set_mode(ReadMode::Speak);
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
        let rows = self.menu_rows();
        let menu_selected = self.menu_at.min(rows.len().saturating_sub(1));
        // The menu may not eat the whole screen: the header, the prompt, the
        // status line and three lines of book always come first.
        let menu_h = menu::height(&rows, menu_selected, area.width)
            .min(area.height.saturating_sub(prompt_h + 5));
        let [header, body, menu_area, prompt, status] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(menu_h),
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
        let highlight = self.speaking_engine();
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
            // The chip shows the speed the reader chose, not the rushed one a
            // held-down enter produces.
            cps: self.base_cps,
            mode: self.mode(),
            effort: self.cfg.effort.level,
            session_chars: self.turn.session_chars,
            busy: self.turn.busy(),
            paused: self.turn.paused(),
            tick: self.tick,
            notice: self.notice.as_ref().map(|(m, _)| m.as_str()),
            scrolled: self.sb.scrolled_away(),
            scroll_percent: self.sb.scroll_percent(),
            has_book: self.book.is_some(),
            library_count: self.library.len(),
            elapsed: self.started.elapsed(),
            speaking: highlight.as_deref(),
            audio_muted: self.cfg.tts.enabled && !self.audio_permitted(),
        };

        let buf = frame.buffer_mut();
        chrome::render_header(header, buf, &chrome);
        self.sb.render(inset(body), buf, self.tick);
        if !rows.is_empty() {
            menu::render(menu_area, buf, &rows, menu_selected);
        }
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
            (KeyCode::Char('r'), true) => self.step_speed(),
            (KeyCode::Char('g'), true) => self.step_hit(true),
            (KeyCode::Char('b'), true) => self.step_hit(false),
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
            // While the command menu is open the arrows belong to it: moving a
            // cursor through six offered commands is what those keys mean here.
            (KeyCode::Up, _) if self.menu_open() => self.menu_step(-1),
            (KeyCode::Down, _) if self.menu_open() => self.menu_step(1),
            (KeyCode::Tab, _) if self.menu_open() => self.menu_complete(),
            // shift+tab cycles the reading mode. crossterm reports it as
            // BackTab, but some terminals send a plain shifted tab instead.
            (KeyCode::BackTab, _) => self.cycle_mode(),
            (KeyCode::Tab, _) if key.modifiers.contains(KeyModifiers::SHIFT) => self.cycle_mode(),
            (KeyCode::Up, _) => self.sb.scroll_lines(-2),
            (KeyCode::Down, _) => self.scroll_down(2),
            (KeyCode::PageUp, _) => self.sb.page(-1),
            (KeyCode::PageDown, _) => self.page_down(),
            _ => {}
        }
    }

    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.sb.scroll_lines(-3),
            MouseEventKind::ScrollDown => self.scroll_down(3),
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

    // ── the slash-command menu ───────────────────────────────────────────────

    /// What the menu needs to know about the reader: which level, which mode,
    /// which engines, and the base pace the multipliers scale.
    fn menu_ctx(&self) -> menu::Ctx<'_> {
        menu::Ctx {
            cfg: &self.cfg,
            mode: self.mode(),
            base_cps: self.base_cps,
        }
    }

    /// Whether the menu is on screen, which decides who owns the arrow keys.
    fn menu_open(&self) -> bool {
        !self.menu_rows().is_empty()
    }

    /// The rows on offer: commands while a name is being typed, and the answers
    /// to that command once it has one.
    fn menu_rows(&self) -> Vec<menu::Row> {
        menu::offer(self.prompt.line(), &self.menu_ctx())
    }

    fn menu_step(&mut self, delta: isize) {
        let count = self.menu_rows().len();
        if count == 0 {
            return;
        }
        let at = self.menu_at.min(count - 1) as isize + delta;
        // Wrapping, because a list of six with no wrap makes the reader hold a
        // key down and wonder whether it is stuck.
        self.menu_at = at.rem_euclid(count as isize) as usize;
    }

    /// `tab`: write the highlighted row into the prompt. A command that still
    /// needs an argument is left with a trailing space, which is also what opens
    /// the second level of the menu.
    fn menu_complete(&mut self) {
        let rows = self.menu_rows();
        let Some(row) = rows.get(self.menu_at.min(rows.len().saturating_sub(1))) else {
            return;
        };
        let line = row.insert.clone();
        self.prompt.set(&line);
        self.menu_at = 0;
    }

    /// `⏎` with the menu open runs the highlighted row rather than whatever
    /// half-typed name is in the prompt.
    fn menu_submit(&mut self) -> bool {
        let rows = self.menu_rows();
        let Some(row) = rows.get(self.menu_at.min(rows.len().saturating_sub(1))) else {
            return false;
        };
        // A row still waiting for an argument is completed, not run: `⏎` on
        // `/goto <n>` cannot mean anything yet.
        if !row.run {
            self.menu_complete();
            return true;
        }
        let name = row.insert.trim_start_matches('/').to_string();
        self.prompt.clear();
        self.menu_at = 0;
        self.sb.push(Block::User(format!("/{name}")));
        self.sb.to_bottom();
        self.command(&name);
        true
    }

    /// `esc`: pause what is playing, then stop it if pressed again.
    ///
    /// Pausing used to be interrupting, and interrupting threw the turn away
    /// without advancing the position — so the next enter re-read the passage
    /// from the top and the screen filled with the same paragraphs twice. Pause
    /// keeps the place; a second press is the way out.
    fn on_escape(&mut self) {
        if self.menu_open() {
            // Closing the menu is what esc means here; the typed text stays, so
            // one more press clears it.
            self.prompt.clear();
            self.menu_at = 0;
        } else if self.turn.paused() {
            self.interrupt();
        } else if self.turn.busy() {
            self.pause();
        } else if !self.prompt.is_empty() {
            self.prompt.clear();
        } else if self.sb.scrolled_away() {
            self.sb.to_bottom();
        }
    }

    fn pause(&mut self) {
        if !self.turn.pause() {
            return;
        }
        // The mode is a setting, not a consequence: pausing holds the place
        // without demoting auto-scroll to manual. Audio cannot be frozen
        // mid-clip, so it stops and picks up at the sentence the reader was on.
        self.silence();
    }

    fn resume_turn(&mut self) {
        if !self.turn.paused() {
            return;
        }
        self.turn.resume();
        let voiced = self.voiced.clone();
        let from = self.cursor.as_ref().map(|cursor| cursor.sentence.0);
        if self.cfg.tts.enabled
            && let (Some(from), Some((id, text))) = (from, voiced)
        {
            self.enqueue_speech_from(id, &text, from);
        }
    }

    // ── the three reading modes ──────────────────────────────────────────────

    /// Which of the three modes the two underlying switches add up to.
    pub fn mode(&self) -> ReadMode {
        ReadMode::of(self.auto, self.cfg.tts.enabled)
    }

    /// `shift+tab`: the next mode, the way a coding agent cycles its own.
    fn cycle_mode(&mut self) {
        let want = self.mode().next();
        if !self.set_mode(want) && want == ReadMode::Speak {
            // No usable voice on this machine: skip the mode that cannot work
            // rather than making the reader press the key twice for nothing.
            self.set_mode(ReadMode::Manual);
        }
    }

    /// Move to `want`, reconcile both switches, and say what it means.
    ///
    /// False when the mode could not be entered, which happens only for
    /// read-aloud with no working engine — and `start_speaker` has already said
    /// why by then.
    fn set_mode(&mut self, want: ReadMode) -> bool {
        if want.speaks() && !self.cfg.tts.enabled {
            self.cfg.tts.enabled = true;
            if !self.start_speaker() {
                self.cfg.tts.enabled = false;
                self.notice(t("mode.tts_unavailable"));
                return false;
            }
            // Speech routed to a device the reader excluded is not read-aloud.
            if !self.audio_permitted() {
                self.report_audio_block();
            }
        } else if !want.speaks() && self.cfg.tts.enabled {
            self.cfg.tts.enabled = false;
            self.silence();
        }
        self.auto = want.scrolls();
        self.cfg.reading.auto = self.auto;
        let _ = self.cfg.save();

        // The first visit to a mode explains it, including how to change its
        // speed: a chip that says `⏵⏵` is not documentation. After that a switch
        // is one line in the status row, because someone cycling with shift+tab
        // does not want the paragraph three times.
        if self.explained[want.index()] {
            // The chip already shows the glyph, so the flash names the mode and
            // the number that mode obeys.
            let flash = match want {
                ReadMode::Manual => want.name().to_string(),
                ReadMode::Auto => format!(
                    "{}  ·  {}",
                    want.name(),
                    crate::metrics::rate_label(self.reveal_cps())
                ),
                ReadMode::Speak => {
                    format!("{}  ·  {}", want.name(), effort::times(self.multiplier()))
                }
            };
            self.notice(&flash);
        } else {
            self.explained[want.index()] = true;
            let message = match want {
                ReadMode::Manual => t("mode.set_manual").to_string(),
                ReadMode::Auto => tf(
                    "mode.set_auto",
                    &[&crate::metrics::rate_label(self.reveal_cps())],
                ),
                ReadMode::Speak => tf("mode.set_tts", &[&effort::times(self.multiplier())]),
            };
            self.system(&message);
        }

        // A mode that moves by itself should start moving.
        if want.scrolls() && self.book.is_some() && !self.turn.busy() && !self.at_end() {
            self.sb.to_bottom();
            self.read_more();
        }
        true
    }

    /// `↓` and the wheel: scroll, unless the reader is already at the bottom in
    /// manual mode, where it means "more book".
    fn scroll_down(&mut self, lines: i32) {
        if !self.load_more_if_at_tail() {
            self.sb.scroll_lines(lines);
        }
    }

    /// `pgdn`, same rule by the page.
    fn page_down(&mut self) {
        if !self.load_more_if_at_tail() {
            self.sb.page(1);
        }
    }

    /// At the very bottom in manual mode, scrolling down is a request for more
    /// book — the way reaching the end of a list loads the next page. In the modes
    /// that scroll themselves it is only ever scrolling.
    fn load_more_if_at_tail(&mut self) -> bool {
        let ready = !self.sb.scrolled_away()
            && self.mode() == ReadMode::Manual
            && self.book.is_some()
            && !self.turn.busy()
            && !self.at_end();
        if ready {
            self.read_more();
        }
        ready
    }

    fn interrupt(&mut self) {
        self.silence();
        if self.turn.interrupt(&mut self.sb) {
            self.sb.push(Block::Event(Event::Interrupted));
        }
        if self.rushing {
            self.rushing = false;
            self.apply_pace();
        }
        self.save_progress();
    }

    fn on_enter(&mut self) {
        if self.menu_open() && self.menu_submit() {
            return;
        }
        if self.prompt.is_empty() {
            // Paused: pick up where the reader stopped, rather than starting
            // the passage again from its first word.
            if self.turn.paused() {
                self.resume_turn();
                return;
            }
            // Enter during a turn means "get on with it" — including the tool
            // call that is spinning, which used to ignore the key entirely.
            if self.turn.busy() {
                if !self.rushing {
                    self.rushing = true;
                    self.turn.set_cps(RUSH_CPS);
                }
                self.turn.rush();
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
        let typed = self.prompt.line().trim().to_string();

        // A bare number picks from the numbered list on screen: the search hits
        // when there are any, the library otherwise. It is not a question and
        // never was, which is why it comes first.
        if let Ok(index) = typed.parse::<usize>() {
            let hits_live = self.book.is_some()
                && self
                    .find
                    .as_ref()
                    .is_some_and(|find| !find.hits.shown.is_empty());
            if hits_live || self.library.get(index).is_some() {
                let line = self.prompt.take();
                self.sb.push(Block::User(line));
                self.sb.to_bottom();
                if hits_live {
                    self.goto_hit(index.saturating_sub(1));
                } else {
                    self.open_entry(index);
                }
                return;
            }
        }

        // Anything else that is not a command is left alone. readio used to read
        // a stray line as a question and search the book for it, which turned a
        // typo into a Grep for `2` across eighty-five paragraphs. The text stays
        // in the prompt, and the status line says what a command looks like.
        if !typed.starts_with('/') {
            self.notice(t("cmd.needs_slash"));
            return;
        }
        let line = self.prompt.take();
        self.sb.push(Block::User(line.clone()));
        self.sb.to_bottom();
        self.command(line.trim_start_matches('/').trim());
    }

    // ── actions ──────────────────────────────────────────────────────────────

    fn read_more(&mut self) {
        let Some(book) = self.book.as_ref() else {
            self.system(t("cmd.no_book"));
            return;
        };
        if self.at_end() {
            self.sb.push(Block::Event(Event::BookComplete));
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
                    let (steps, needle, hits) = flow::answer(book, arg);
                    self.remember_search(needle, hits);
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
            // The base pace the effort multiplier scales. `/effort` is the
            // everyday control; this is the one for someone who knows they read
            // at 70 characters a second.
            "speed" | "cps" => match arg.parse::<f32>() {
                Ok(v) if (4.0..=4000.0).contains(&v) => {
                    self.base_cps = v;
                    self.cfg.reading.speed = v;
                    let _ = self.cfg.save();
                    self.apply_pace();
                    let effective = format!("{:.0}", self.reveal_cps());
                    let level = self.cfg.effort.level.label();
                    self.system(&tf("cmd.speed_set", &[&effective, &level]));
                }
                _ => self.system(t("cmd.usage_speed")),
            },
            "effort" => self.effort_command(arg),
            // No `m` alias: that belongs to `/mark`, which readers reach for far
            // more often than they switch modes.
            "mode" => match arg {
                "" => {
                    let mode = self.mode();
                    let chip = crate::mode::chip(mode);
                    let name = mode.name();
                    self.system(&tf("mode.now", &[&name, &chip]));
                }
                other => match ReadMode::parse(other) {
                    Some(want) => {
                        self.set_mode(want);
                    }
                    None => self.system(t("mode.usage")),
                },
            },
            // `/auto` predates the three modes and still means what it did: on
            // and off, between auto-scroll and manual.
            "auto" => {
                let want = if self.mode() == ReadMode::Auto {
                    ReadMode::Manual
                } else {
                    ReadMode::Auto
                };
                self.set_mode(want);
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
                            &crate::metrics::amount(
                                read,
                                crate::metrics::words_from_char_count(read),
                            ),
                            &crate::metrics::amount(book.char_count(), book.word_count()),
                            &(self.pos.chapter + 1),
                            &(self.pos.para + 1),
                        ],
                    );
                    self.system(&text);
                }
            },
            // ── bookmarks ──
            "mark" | "m" => self.mark_command(arg),
            "marks" | "bookmarks" => self.marks_command(arg),
            "unmark" => self.unmark_command(arg),
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

    /// `/effort [minimal|low|medium|high|xhigh|max]`
    fn effort_command(&mut self, arg: &str) {
        if arg.is_empty() {
            self.effort_report();
            return;
        }
        match Effort::parse(arg) {
            Some(level) => self.set_effort(level),
            None => self.system(t("effort.usage")),
        }
    }

    /// `/rate [0.5-3.0]` — what the current level is worth.
    ///
    /// The levels are labels; the multipliers behind them belong to the reader,
    /// and this is the way to change one without opening the config file. It edits
    /// the level in force, so `/effort xhigh` then `/rate 0.9` is how someone
    /// makes their slow gear their own.
    fn rate_command(&mut self, arg: &str) {
        if arg.is_empty() {
            self.effort_report();
            return;
        }
        match arg.trim_end_matches(['x', 'X', '×']).parse::<f32>() {
            Ok(v) if (0.5..=3.0).contains(&v) => {
                let level = self.cfg.effort.level;
                self.cfg.effort.multipliers.set(level, v);
                // Re-entering the level applies it and re-queues the audio.
                self.set_effort(level);
                let times = effort::times(self.multiplier());
                self.system(&tf("effort.tuned", &[&level.label(), &times]));
            }
            _ => self.system(t("tts.usage_rate")),
        }
    }

    /// `^r`: the next effort level, and with it the next pace — one key that
    /// means "read faster" whether the words are being typed or spoken.
    fn step_speed(&mut self) {
        self.set_effort(self.cfg.effort.level.next());
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

    /// Keep a search's results so the reader can walk them.
    fn remember_search(&mut self, needle: String, hits: crate::book::Hits) {
        self.find = if hits.is_empty() {
            None
        } else {
            Some(Find {
                needle,
                hits,
                at: usize::MAX, // nothing visited yet; the first jump lands on 0
            })
        };
    }

    /// Go to hit `index` (zero-based) and light the term up there.
    fn goto_hit(&mut self, index: usize) {
        let Some(find) = self.find.as_mut() else {
            self.system(t("find.none_active"));
            return;
        };
        let Some(hit) = find.hits.shown.get(index).cloned() else {
            let total = find.hits.shown.len();
            self.system(&tf("find.no_such", &[&total, &(index + 1)]));
            return;
        };
        find.at = index;
        let (needle, count) = (find.needle.clone(), find.hits.shown.len());

        self.pos = Pos {
            chapter: hit.chapter,
            para: hit.para,
        };
        self.resumed = false;
        self.save_progress();
        // The passage has not been rendered yet: mark the term so the block that
        // carries it is highlighted as soon as it appears.
        self.marking = Some(needle);
        self.system(&tf(
            "find.jumped",
            &[&(index + 1), &count, &(hit.chapter + 1), &(hit.para + 1)],
        ));
        self.read_more();
    }

    // ── bookmarks ────────────────────────────────────────────────────────────

    /// `/mark [note]`: keep this place, with a note or with the words that are
    /// here.
    fn mark_command(&mut self, note: &str) {
        let Some(book) = self.book.as_ref() else {
            self.no_book();
            return;
        };
        let chars = book.chars_before(self.pos.chapter, self.pos.para) as u64;
        let label = if note.trim().is_empty() {
            opening_words(book, self.pos)
        } else {
            note.trim().to_string()
        };
        let id = book.id.clone();
        let mark = crate::store::Mark {
            chars,
            chapter: self.pos.chapter,
            para: self.pos.para,
            label,
            at: now_secs(),
        };

        // Save first: the mark is stored beside the position, and a fresh book
        // has no entry to attach it to yet.
        self.save_progress();
        let mut progress = self.store.get(&id).cloned().unwrap_or_default();
        // Marking the same place twice is one mark, renamed.
        match progress.marks.iter_mut().find(|m| m.chars == chars) {
            Some(existing) => *existing = mark.clone(),
            None => progress.marks.push(mark.clone()),
        }
        progress.marks.sort_by_key(|m| m.chars);
        let count = progress
            .marks
            .iter()
            .position(|m| m.chars == chars)
            .map(|index| index + 1)
            .unwrap_or(progress.marks.len());
        self.store.record(&id, progress);
        let _ = self.store.save();

        let book = self.book.as_ref().expect("checked above");
        let steps = flow::marked(book, &mark, count);
        self.turn.enqueue(steps);
    }

    /// `/marks` to list them, `/marks <n>` to go to one.
    fn marks_command(&mut self, arg: &str) {
        let Some(book) = self.book.as_ref() else {
            self.no_book();
            return;
        };
        let marks = self
            .store
            .get(&book.id)
            .map(|p| p.marks.clone())
            .unwrap_or_default();
        if marks.is_empty() {
            self.system(t("cmd.marks_none"));
            return;
        }
        if arg.trim().is_empty() {
            let steps = flow::marks(book, &marks);
            self.turn.enqueue(steps);
            return;
        }
        match arg.trim().parse::<usize>() {
            Ok(n) if n >= 1 && n <= marks.len() => self.goto_mark(&marks[n - 1].clone(), n),
            _ => self.system(&tf("cmd.mark_no_such", &[&marks.len()])),
        }
    }

    /// `/unmark <n>`: drop one.
    fn unmark_command(&mut self, arg: &str) {
        let Some(book) = self.book.as_ref() else {
            self.no_book();
            return;
        };
        let id = book.id.clone();
        let mut progress = self.store.get(&id).cloned().unwrap_or_default();
        let count = progress.marks.len();
        match arg.trim().parse::<usize>() {
            Ok(n) if n >= 1 && n <= count => {
                let gone = progress.marks.remove(n - 1);
                self.store.record(&id, progress);
                let _ = self.store.save();
                self.system(&tf("cmd.unmark_done", &[&n, &gone.label]));
            }
            _ if count == 0 => self.system(t("cmd.marks_none")),
            _ => self.system(&tf("cmd.mark_no_such", &[&count])),
        }
    }

    /// Go to a mark. The character offset decides, not the stored indices: the
    /// mark may predate a release that cuts this book into chapters differently.
    fn goto_mark(&mut self, mark: &crate::store::Mark, number: usize) {
        let Some(book) = self.book.as_ref() else {
            self.no_book();
            return;
        };
        let (chapter, para) = book.locate(mark.chars as usize);
        self.pos = Pos { chapter, para };
        self.resumed = false;
        self.save_progress();
        self.system(&tf(
            "cmd.mark_jumped",
            &[&number, &mark.label, &(chapter + 1)],
        ));
        self.read_more();
    }

    /// `^g` / `^b`: the next or previous hit, wrapping round with a word about it.
    fn step_hit(&mut self, forward: bool) {
        let Some(find) = self.find.as_ref() else {
            self.system(t("find.none_active"));
            return;
        };
        let count = find.hits.shown.len();
        if count == 0 {
            self.system(t("find.none_active"));
            return;
        }
        let next = match (find.at, forward) {
            (usize::MAX, true) => 0,
            (usize::MAX, false) => count - 1,
            (at, true) if at + 1 < count => at + 1,
            (_, true) => {
                self.system(t("find.wrapped"));
                0
            }
            (at, false) if at > 0 => at - 1,
            (_, false) => {
                self.system(t("find.wrapped_back"));
                count - 1
            }
        };
        self.goto_hit(next);
    }

    /// Light the search term where it appears in a passage just rendered.
    ///
    /// Reuses the read-aloud highlight: the sentence holding the match gets the
    /// light wash and the term itself the deep one, which is exactly the pair of
    /// questions a search result raises — where is it, and where exactly.
    fn mark_match(&mut self, id: u64, text: &str) {
        let Some(needle) = self.marking.clone() else {
            return;
        };
        let folded_text = text.to_lowercase();
        let Some(at) = folded_text.find(&needle.to_lowercase()) else {
            return;
        };
        // Snap to a character boundary in case folding shifted the offset.
        let start = (0..=at)
            .rev()
            .find(|i| text.is_char_boundary(*i))
            .unwrap_or(0);
        let end = (start + needle.len()).min(text.len());
        let end = (start..=end)
            .rev()
            .find(|i| text.is_char_boundary(*i))
            .unwrap_or(text.len());
        let sentence = crate::tts::sentence::split(text)
            .into_iter()
            .map(|utterance| utterance.range)
            .find(|(from, to)| *from <= start && end <= *to)
            .unwrap_or((0, text.len()));
        self.marking = None;
        self.sb
            .set_highlight(id, Some(Highlight::focused(sentence, (start, end))));
        self.lit = Some(id);
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
                        &entry.amount(),
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
            marks: previous.marks,
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
            &entry.amount(),
            &entry.chapters,
        ],
    );
    Ok((book, note))
}
