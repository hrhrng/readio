//! Read-aloud pacing: the text may not run ahead of the voice.
//!
//! The failure this file exists for looked like a bug in the speech engine and
//! was not. Chinese comes out of Kokoro at roughly four characters a second;
//! the reveal ran at forty-six, and the passage was only *nudged* towards the
//! audio — the pacer was handed the clip's characters per second once the clip
//! started, which was already too late. With auto-advance on, the next passage
//! was fetched the moment the last character appeared rather than the moment
//! the last word was spoken, so the gap compounded: a reader who asked for a
//! chapter to be read aloud got the whole chapter on screen inside a minute
//! with a voice somewhere back in the first paragraph.
//!
//! Neither half can be tested without an engine, and a test machine has none.
//! So these tests bring their own: a shell script that waits, the way real
//! synthesis waits, and then produces a clip of a known length. Everything
//! downstream — the queue, the prefetch window, the events, the hold — is the
//! real thing.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use readio::app::App;
use readio::book::Book;
use readio::library::Library;
use readio::store::Store;
use readio::tts::config::EngineSpec;

/// How long the stand-in engine pretends to spend synthesizing a sentence.
///
/// Longer than the clip it produces, which is the situation that matters:
/// Kokoro renders Chinese at roughly the speed it speaks it, so the renderer is
/// the bottleneck and every sentence boundary has a real gap behind it. Those
/// gaps are where a reveal running on its own clock gets ahead.
const SYNTH_MS: u64 = 900;
/// How long every clip it produces plays for.
const CLIP_MS: u64 = 300;

/// These tests share one `config.yaml`, so they take turns.
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A silent mono wav of `ms` milliseconds, which is all the clock needs: the
/// player waits out the duration in the header.
fn silence(ms: u64) -> Vec<u8> {
    let rate = 8_000u32;
    let frames = (rate as u64 * ms / 1000) as u32;
    let data = frames * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    out.resize(44 + data as usize, 0);
    out
}

/// Write the stand-in engine and return the command that runs it.
///
/// `play` is left empty in the spec, which means "the synth command played it
/// itself" — so playback is exactly the clip's own duration, with none of a
/// real player's start-up in the way.
fn stand_in_engine(home: &Path) -> EngineSpec {
    let clip = home.join("clip.wav");
    std::fs::write(&clip, silence(CLIP_MS)).expect("write clip");
    let script = home.join("stand-in-voice");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nsleep {}\ncp '{}' \"$1\"\n",
            SYNTH_MS as f64 / 1000.0,
            clip.display()
        ),
    )
    .expect("write engine");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod engine");
    }
    EngineSpec {
        synth: format!("{} {{out}}", script.display()),
        about: "stand-in".to_string(),
        ..EngineSpec::default()
    }
}

/// A book of short, individually recognisable sentences.
///
/// The shape matters, so it is a parameter. A turn covers a chapter, so long
/// chapters keep one passage on screen long enough for a reveal running on its
/// own clock to show itself, and short ones make the turn boundary — the moment
/// the *next* passage is fetched — come round quickly.
fn book(home: &Path, name: &str, chapters: usize, per_chapter: usize) -> PathBuf {
    let dir = home.join("sources").join(name);
    std::fs::create_dir_all(&dir).expect("source dir");
    let path = dir.join(format!("{name}.md"));
    let stems = [
        "甲子", "乙丑", "丙寅", "丁卯", "戊辰", "己巳", "庚午", "辛未", "壬申", "癸酉", "甲戌",
        "乙亥",
    ];
    let mut text = String::new();
    for chapter in 0..chapters {
        text.push_str(&format!("# 第{}章\n\n", chapter + 1));
        for line in 0..per_chapter {
            text.push_str(&format!(
                "这一句是{}。",
                stems[(chapter * per_chapter + line) % 12]
            ));
        }
        text.push_str("\n\n");
    }
    std::fs::write(&path, text).expect("write book");
    std::fs::canonicalize(&path).unwrap_or(path)
}

fn fixture(book_path: PathBuf) -> (App, Terminal<TestBackend>) {
    let home = common::isolated_home();
    let mut config = readio::config::Config::load().0;
    // Read-aloud is entered below the way a reader enters it, with `/mode tts`,
    // so the switch itself is part of what these tests cover.
    config.tts.enabled = false;
    config.tts.engine = "stand-in".to_string();
    config.tts.voice.clear();
    config.tts.prefetch = 2;
    config.tts.output.allow.clear();
    config
        .tts
        .engines
        .insert("stand-in".to_string(), stand_in_engine(home));
    config.reading.speed = 46.0;
    config.reading.auto = false;
    config.effort = readio::config::EffortConfig::default();
    let _ = config.save();

    let book = Book::load(Some(&book_path)).expect("book");
    let app = App::new(Library::ephemeral(), Store::ephemeral(), Some(book), None);
    let terminal = Terminal::new(TestBackend::new(96, 30)).expect("terminal");
    (app, terminal)
}

/// One long chapter: a passage that outlasts the window, so how far into it the
/// reveal has got is a measurement rather than a race to the end of the book.
fn long_chapters(home: &Path) -> PathBuf {
    book(home, "pacing-long", 3, 20)
}

/// Short chapters, so the turn boundary comes round inside the window.
fn short_chapters(home: &Path) -> PathBuf {
    book(home, "pacing-short", 10, 4)
}

fn tick(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    app.on_tick();
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    std::thread::sleep(Duration::from_millis(6));
}

/// The visible screen, for a failure that needs explaining.
fn screen(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            let mut row = String::new();
            let mut x = 0u16;
            while x < buffer.area.width {
                let symbol = buffer[(x, y)].symbol();
                row.push_str(symbol);
                x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            row.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Drive frames until `done`, or give up. A deadline rather than a frame count:
/// the timing here is wall-clock, so a frame budget gives a loaded machine less
/// time rather than more.
fn until(
    app: &mut App,
    terminal: &mut Terminal<TestBackend>,
    what: &str,
    done: impl Fn(&App) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(app) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}\n{}",
            screen(terminal)
        );
        tick(app, terminal);
    }
}

fn type_line(app: &mut App, line: &str) {
    for c in line.chars() {
        app.on_key(KeyEvent::from(KeyCode::Char(c)));
    }
    app.on_key(KeyEvent::from(KeyCode::Enter));
}

/// Settle the opening turn, switch to read-aloud, and wait for book text to
/// start streaming.
fn start_reading_aloud(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    until(app, terminal, "the opening turn", |app| !app.turn.busy());
    type_line(app, "/mode tts");
    until(app, terminal, "the passage to start", |app| {
        shown(app).is_some()
    });
    assert_eq!(
        app.mode(),
        readio::mode::Mode::Speak,
        "the stand-in engine should have been good enough to switch"
    );
}

/// Characters of the passage now on screen.
fn shown(app: &App) -> Option<usize> {
    app.turn.streaming_passage().map(|(_, chars)| chars)
}

/// The first half of the fix: a passage that is going to be read aloud shows
/// nothing until there is audio to show it against.
///
/// The stand-in engine spends the best part of a second on its first sentence,
/// which is the window the old build filled with thirty-odd characters of text
/// nobody was reading yet.
#[test]
fn a_passage_shows_nothing_until_its_first_clip_exists() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture(long_chapters(common::isolated_home()));
    start_reading_aloud(&mut app, &mut terminal);

    // Two thirds of the synthesis wait, and not one character may appear in it.
    // At the reading speed this window is worth about twenty-five characters.
    let deadline = Instant::now() + Duration::from_millis(SYNTH_MS * 2 / 3);
    while Instant::now() < deadline {
        tick(&mut app, &mut terminal);
        assert_eq!(
            shown(&app),
            Some(0),
            "the text is running ahead of the voice again"
        );
    }

    // And then it does arrive — a hold that never lifts is a frozen screen.
    until(&mut app, &mut terminal, "the first clip", |app| {
        shown(app).is_some_and(|chars| chars > 0)
    });
}

/// The pacing itself, measured rather than reasoned about.
///
/// One chapter of twenty sentences is one passage, long enough that eight
/// seconds lands in the middle of it rather than at the end of the book. The
/// stand-in engine is slower at making clips than at playing them — as Kokoro
/// is — so sentences arrive about one every nine tenths of a second.
///
/// Measured on the build that fixed this: 51 characters. Measured on the
/// behaviour it replaced: 133. The reading speed alone would be 368. The bound
/// sits between the first two with room for a machine having a bad day.
#[test]
fn the_text_never_gets_ahead_of_the_voice() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture(long_chapters(common::isolated_home()));
    start_reading_aloud(&mut app, &mut terminal);
    let before = app.turn.session_chars;

    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(8) {
        tick(&mut app, &mut terminal);
    }
    let spoken = app.turn.session_chars - before;

    assert!(
        spoken <= 110,
        "{spoken} characters in eight seconds is faster than anything said them"
    );
    assert!(spoken > 20, "only {spoken} characters: nothing was read");
}

/// And the hold has to let go. A reveal that waits for a voice is one missed
/// event away from a reader staring at a page that never fills, so this is the
/// other side of the same coin: short chapters, and reading has to cross from
/// one into the next on its own.
#[test]
fn reading_carries_on_by_itself_when_the_voice_finishes_a_chapter() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture(short_chapters(common::isolated_home()));
    start_reading_aloud(&mut app, &mut terminal);
    let before = app.turn.session_chars;

    // A chapter is a heading and four sentences: five clips, thirty-four
    // characters, four and a half seconds of audio.
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(8) {
        tick(&mut app, &mut terminal);
    }
    let spoken = app.turn.session_chars - before;

    assert!(
        spoken > 36,
        "only {spoken} characters, which is the first chapter and no more: \
         reading stopped instead of carrying on into the next"
    );
}
