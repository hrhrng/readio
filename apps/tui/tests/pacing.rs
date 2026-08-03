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
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use readio::app::App;
use readio::book::Book;
use readio::library::Library;
use readio::store::{Progress, SpeechCheckpoint, Store};
use readio::voice::config::EngineSpec;

/// How long the stand-in engine pretends to spend synthesizing a sentence.
///
/// Longer than the clip it produces, which is the situation that matters:
/// Kokoro renders Chinese at roughly the speed it speaks it, so the renderer is
/// the bottleneck and every sentence boundary has a real gap behind it. Those
/// gaps are where a reveal running on its own clock gets ahead.
const SYNTH_MS: u64 = 900;
/// How long every clip it produces plays for.
const CLIP_MS: u64 = 300;

/// The other engine worth modelling: one whose clips are worth waiting for.
///
/// A resident Kokoro renders a sentence in about a second and speaks it for
/// three or four. Nothing about a *sentence* boundary is hard at that rate —
/// the pipeline covers it. What is left is the boundary between paragraphs,
/// where the next one does not exist yet and so nothing was being rendered:
/// that second landed in silence instead of underneath a clip.
const SLOW_SYNTH_MS: u64 = 2_000;
/// A clip long enough to render the next paragraph's opening sentence inside.
const LONG_CLIP_MS: u64 = 2_000;

/// Paths are process-global, so these tests take turns while each fixture gets
/// its own home. Sharing one `config.yaml` and speech scratch directory made a
/// device-gate test that deliberately tears down the pipeline leak timing into
/// whichever read-aloud test happened to run next.
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fixture_home() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let home = common::isolated_home()
        .join("cases")
        .join(NEXT.fetch_add(1, Ordering::Relaxed).to_string());
    std::fs::create_dir_all(&home).expect("fixture home");
    readio::paths::set_home(&home);
    home
}

/// A silent mono wav of `ms` milliseconds, which is all the clock needs: the
/// player waits out the duration in the header.
fn silence(ms: u64) -> Vec<u8> {
    // Playback in these integration tests is clocked rather than sent to a
    // device. A low valid sample rate keeps the family of duration-scaled WAVs
    // tiny while preserving their exact media time.
    let rate = 100u32;
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
fn stand_in_engine(home: &Path, synth_ms: u64, clip_ms: u64) -> EngineSpec {
    let clips = home.join(format!("clips-{clip_ms}"));
    std::fs::create_dir_all(&clips).expect("clip dir");
    // The baseline fixture sentence is seven Chinese characters, or 21 UTF-8
    // bytes. Real TTS duration grows with its input; keeping one fixed WAV for
    // every contextual window makes a paragraph appear to be spoken in the
    // time previously assigned to one sentence.
    for bytes in 1..=512u64 {
        // 600ms for the baseline sentence keeps contextual playback at a
        // plausible upper bound even in fixtures whose original 300ms clip was
        // chosen only to make synthesis slower than playback.
        let duration = clip_ms.max(600).saturating_mul(bytes).div_ceil(21).max(20);
        std::fs::write(clips.join(format!("{bytes}.wav")), silence(duration))
            .expect("write scaled clip");
    }
    let script = home.join(format!("stand-in-voice-{synth_ms}-{clip_ms}"));
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nsleep {}\nbytes=$(printf %s \"$2\" | wc -c | tr -d ' ')\n\
             [ \"$bytes\" -lt 1 ] && bytes=1\n[ \"$bytes\" -gt 512 ] && bytes=512\n\
             cp \"{}/$bytes.wav\" \"$1\"\n",
            synth_ms as f64 / 1000.0,
            clips.display()
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
        synth: format!("{} {{out}} {{text}}", script.display()),
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
    fixture_with(book_path, SYNTH_MS, CLIP_MS)
}

/// The same, with the engine's timing chosen: slow to render, or slow to speak.
fn fixture_with(book_path: PathBuf, synth_ms: u64, clip_ms: u64) -> (App, Terminal<TestBackend>) {
    fixture_with_query(book_path, synth_ms, clip_ms, "")
}

fn fixture_with_query(
    book_path: PathBuf,
    synth_ms: u64,
    clip_ms: u64,
    output_query: &str,
) -> (App, Terminal<TestBackend>) {
    fixture_with_pace(book_path, synth_ms, clip_ms, output_query, 46.0)
}

fn fixture_with_pace(
    book_path: PathBuf,
    synth_ms: u64,
    clip_ms: u64,
    output_query: &str,
    cps: f32,
) -> (App, Terminal<TestBackend>) {
    let home = fixture_home();
    fixture_at_home(home, book_path, synth_ms, clip_ms, output_query, cps, None)
}

fn fixture_at_home(
    home: PathBuf,
    book_path: PathBuf,
    synth_ms: u64,
    clip_ms: u64,
    output_query: &str,
    cps: f32,
    input_log: Option<&Path>,
) -> (App, Terminal<TestBackend>) {
    let mut config = readio::config::Config::load().0;
    config.reading.mode = readio::mode::Mode::Manual;
    config.voice.enabled = false;
    config.voice.engine = "stand-in".to_string();
    config.voice.name.clear();
    config.voice.prefetch = 2;
    config.voice.output.allow.clear();
    config.voice.output.query = output_query.to_string();
    config.voice.engines.insert(
        "stand-in".to_string(),
        stand_in_engine(&home, synth_ms, clip_ms),
    );
    config.reading.speed = cps;
    config.input_log = input_log.map(|path| path.display().to_string());
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

/// A chapter per sentence: nothing but turn boundaries, one every few seconds.
fn tiny_chapters(home: &Path) -> PathBuf {
    book(home, "pacing-tiny", 12, 1)
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

/// Settle the opening turn, enter read-aloud, and wait for its text to start
/// streaming.
fn start_reading_aloud(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    until(app, terminal, "the opening turn", |app| !app.turn.busy());
    type_line(app, "/mode tts");
    assert_eq!(app.mode(), readio::mode::Mode::Speak);
    assert!(app.speech_enabled(), "the stand-in voice should start");
    until(app, terminal, "the passage to start", |app| {
        shown(app).is_some()
    });
}

/// The activity strip models an interrupted agent turn. Speech may be the
/// current projection of that turn, but exposing an audio-specific pause here
/// makes the generic interrupt affordance change meaning between modes.
#[test]
fn speech_uses_the_generic_interruption_copy() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture(long_chapters(common::isolated_home()));
    start_reading_aloud(&mut app, &mut terminal);

    app.on_key(KeyEvent::from(KeyCode::Esc));
    tick(&mut app, &mut terminal);
    let view = screen(&terminal);

    assert!(
        view.contains("已中断"),
        "speech should look like an interrupted agent turn:\n{view}"
    );
    assert!(
        !view.contains("朗读已暂停") && !view.contains("朗读已中断"),
        "the activity strip should not expose the speech implementation:\n{view}"
    );
}

/// Characters of the passage now on screen.
fn shown(app: &App) -> Option<usize> {
    app.turn.streaming_passage().map(|(_, chars)| chars)
}

/// Manual means the token clock is no longer running. The most surprising
/// path into it is from read-aloud: stopping the speaker used to lift the
/// voice's reveal ceiling, which handed the half-spoken passage straight to
/// the automatic text pacer even though the mode chip already said manual.
#[test]
fn switching_to_manual_freezes_the_live_passage() {
    let _guard = exclusive();
    let (mut app, mut terminal) =
        fixture_with(long_chapters(common::isolated_home()), 100, LONG_CLIP_MS);
    start_reading_aloud(&mut app, &mut terminal);
    until(&mut app, &mut terminal, "some spoken text", |app| {
        shown(app).is_some_and(|chars| chars >= 4)
    });

    type_line(&mut app, "/mode manual");
    assert_eq!(app.mode(), readio::mode::Mode::Manual);
    let parked = shown(&app).expect("the passage remains in flight");
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        tick(&mut app, &mut terminal);
    }

    assert_eq!(
        shown(&app),
        Some(parked),
        "manual inherited the automatic text clock after leaving read-aloud"
    );
}

/// Switching modes during the simulated thinking/tool prelude is the same
/// decision as switching during visible prose. The passage does not exist yet,
/// so a ceiling captured only from `streaming_passage()` misses it and manual
/// quietly starts auto-streaming as soon as the tool completes.
#[test]
fn switching_to_manual_before_the_passage_starts_holds_that_turn() {
    let _guard = exclusive();
    let (mut app, mut terminal) =
        fixture_with(long_chapters(common::isolated_home()), 100, LONG_CLIP_MS);
    until(&mut app, &mut terminal, "the opening turn", |app| {
        !app.turn.busy()
    });

    type_line(&mut app, "/mode auto");
    assert!(app.turn.busy(), "auto should have queued a reading turn");
    type_line(&mut app, "/mode manual");
    assert_eq!(app.mode(), readio::mode::Mode::Manual);
    until(&mut app, &mut terminal, "the held passage", |app| {
        shown(app).is_some()
    });
    let parked = shown(&app).expect("the passage started");
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        tick(&mut app, &mut terminal);
    }

    assert_eq!(
        shown(&app),
        Some(parked),
        "manual started the passage on the automatic clock after the tool prelude"
    );
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

/// Token reveal is a projection of the playback PTS, not a second pacer.
///
/// The opening utterance says `第1章`, inside the six-character source heading
/// `## 第1章`. The renderer interprets rather than displays the Markdown prefix,
/// but the reveal queue still addresses the raw source: audio start consumes
/// `## ` at once, then the clip's two seconds project only across the three
/// spoken characters. The old independent character clock clamps itself to
/// four characters per second and has already run ahead here.
#[test]
fn token_reveal_samples_the_global_playback_position() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture_with(
        long_chapters(common::isolated_home()),
        SYNTH_MS,
        LONG_CLIP_MS,
    );
    start_reading_aloud(&mut app, &mut terminal);
    until(&mut app, &mut terminal, "the playback clock", |app| {
        app.voice_sounding()
    });

    until(&mut app, &mut terminal, "half of the opening clip", |app| {
        app.voice_position().is_some_and(|position| {
            position.elapsed.as_nanos().saturating_mul(2) >= position.duration.as_nanos()
        })
    });
    let position = app.voice_position().expect("a playback position");
    let (_, source, _) = app.turn.passage_in_flight().expect("the opening passage");
    assert!(
        source[position.range.0..position.range.1].contains("第1章"),
        "the opening context lost its heading"
    );
    let sentence_start = source[..position.range.0].chars().count();
    let sentence_end = source[..position.range.1].chars().count();
    let expected = sentence_start
        + ((sentence_end - sentence_start) as u128 * position.elapsed.as_millis()
            / position.duration.as_millis().max(1)) as usize;
    assert_eq!(
        shown(&app),
        Some(expected),
        "the text clock drifted away from the global playback position"
    );
}

/// Entering read-aloud partway through a sentence must keep the text parked
/// until the audio cursor catches up with what was already visible.
///
/// The old cursor treated the visible remainder as if it occupied the whole
/// clip. With five of this seven-character sentence already on screen, that
/// leaked the sixth character around halfway through the audio even though the
/// voice had only reached the fourth character on its absolute timeline.
#[test]
fn entering_read_aloud_mid_sentence_waits_for_audio_to_catch_up() {
    let _guard = exclusive();
    let source = long_chapters(common::isolated_home());
    let (mut app, mut terminal) = fixture_with_pace(source, SYNTH_MS, LONG_CLIP_MS, "", 4.0);

    until(&mut app, &mut terminal, "the opening turn", |app| {
        !app.turn.busy()
    });
    app.on_key(KeyEvent::from(KeyCode::Enter));

    until(
        &mut app,
        &mut terminal,
        "five characters of a body sentence",
        |app| {
            app.turn
                .passage_in_flight()
                .is_some_and(|(_, text, released)| {
                    text.find("这一句是甲子。").is_some_and(|byte| {
                        let sentence_start = text[..byte].chars().count();
                        released == sentence_start + 5
                    })
                })
        },
    );
    let already_visible = shown(&app).expect("a body passage is streaming");

    type_line(&mut app, "/mode tts");
    until(
        &mut app,
        &mut terminal,
        "the resumed sentence's audio clock",
        |app| {
            app.voice_position().is_some_and(|position| {
                position.range.1 > position.range.0
                    && position.elapsed.as_nanos().saturating_mul(2) >= position.duration.as_nanos()
            })
        },
    );

    assert_eq!(
        shown(&app),
        Some(already_visible),
        "text advanced before the absolute audio cursor caught up"
    );
}

/// The opt-in diagnostic file must include the evidence needed to explain a
/// pacing bug after it happened: the sentence boundary, clip duration, and a
/// sampled mapping from media time to visible characters.
#[test]
fn diagnostic_log_records_voice_boundaries_and_clock_samples() {
    let _guard = exclusive();
    let source = long_chapters(common::isolated_home());
    let home = fixture_home();
    let log = home.join("readio.log");
    let (mut app, mut terminal) =
        fixture_at_home(home, source, SYNTH_MS, LONG_CLIP_MS, "", 46.0, Some(&log));
    start_reading_aloud(&mut app, &mut terminal);
    until(&mut app, &mut terminal, "a sampled audio clock", |app| {
        app.voice_position()
            .is_some_and(|position| position.elapsed >= Duration::from_millis(300))
    });

    let trace = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        trace.contains("voice enqueue") && trace.contains("range=") && trace.contains("text="),
        "the log cannot reconstruct sentence boundaries:\n{trace}"
    );
    assert!(
        trace.contains("voice started")
            && trace.contains("duration_ms=")
            && trace.contains("released="),
        "the log cannot reconstruct clip start state:\n{trace}"
    );
    assert!(
        trace.contains("voice clock") && trace.contains("media_ms=") && trace.contains("target="),
        "the log cannot reconstruct the reveal clock:\n{trace}"
    );
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
    assert!(
        spoken > 20,
        "only {spoken} characters: nothing was read\n\
         mode={:?} speech={} busy={} sounding={} position={:?}\n{}",
        app.mode(),
        app.speech_enabled(),
        app.turn.busy(),
        app.voice_sounding(),
        app.voice_position(),
        screen(&terminal)
    );
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

    // This assertion is about liveness, not throughput. Thought/Read/Write
    // timing and synthesis all share the machine with the test runner, so a
    // fixed eight-second sample can catch a healthy reader in the transition
    // between chapters. Wait for proof that some of chapter two was revealed;
    // `until` still gives a stopped reader a hard deadline.
    until(
        &mut app,
        &mut terminal,
        "read-aloud to enter chapter two",
        |app| app.turn.session_chars.saturating_sub(before) > 36,
    );
    let spoken = app.turn.session_chars - before;

    assert!(
        spoken > 36,
        "only {spoken} characters, which is the first chapter and no more: \
         reading stopped instead of carrying on into the next\n\
         mode={:?} speech={} busy={} sounding={} position={:?}\n{}",
        app.mode(),
        app.speech_enabled(),
        app.turn.busy(),
        app.voice_sounding(),
        app.voice_position(),
        screen(&terminal)
    );
}

#[test]
fn read_aloud_continues_without_becoming_auto() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture(short_chapters(common::isolated_home()));
    start_reading_aloud(&mut app, &mut terminal);
    let before = app.turn.session_chars;

    until(&mut app, &mut terminal, "read-aloud to keep going", |app| {
        app.turn.session_chars.saturating_sub(before) > 36
    });
    let spoken = app.turn.session_chars - before;

    assert_eq!(
        app.mode(),
        readio::mode::Mode::Speak,
        "continuous read-aloud is its own mode, not auto with a speaker attached"
    );
    assert!(app.speech_enabled());
    assert!(
        spoken > 36,
        "only {spoken} characters: read-aloud stopped after one passage while its voice was healthy\n\
         speech={} busy={} sounding={} position={:?}\n{}",
        app.speech_enabled(),
        app.turn.busy(),
        app.voice_sounding(),
        app.voice_position(),
        screen(&terminal)
    );
}

#[test]
fn losing_the_allowed_audio_device_stops_without_becoming_auto() {
    let _guard = exclusive();
    let home = common::isolated_home();
    let source = long_chapters(home);
    let query = home.join("current-output");
    std::fs::write(&query, "#!/bin/sh\necho desk-speakers\n").expect("write output query");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&query, std::fs::Permissions::from_mode(0o755))
            .expect("chmod output query");
    }
    let (mut app, mut terminal) =
        fixture_with_query(source, SYNTH_MS, CLIP_MS, &query.display().to_string());
    start_reading_aloud(&mut app, &mut terminal);

    type_line(&mut app, "/device allow headphones");
    for _ in 0..8 {
        tick(&mut app, &mut terminal);
    }
    let view = screen(&terminal);

    assert_eq!(
        app.mode(),
        readio::mode::Mode::Speak,
        "losing an allowed output must not rewrite read-aloud to auto"
    );
    assert!(
        !app.turn.busy(),
        "tokens must stop when the selected audio output is unavailable:\n{view}"
    );
    assert!(
        !app.voice_sounding(),
        "audio queued for the wrong output must be stopped"
    );
}

#[test]
fn resuming_read_aloud_restores_the_voice_clock_before_tokens_move() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture(long_chapters(common::isolated_home()));
    start_reading_aloud(&mut app, &mut terminal);

    app.on_key(KeyEvent::from(KeyCode::Char(' ')));
    assert!(app.turn.paused(), "space should pause the read-aloud turn");
    app.on_key(KeyEvent::from(KeyCode::Char(' ')));

    assert!(!app.turn.paused(), "space should resume the turn");
    assert_eq!(app.mode(), readio::mode::Mode::Speak);
    assert!(
        app.turn.held() || app.voice_runway() > 0,
        "resuming must restore a TTS clock before any more tokens can move"
    );
}

/// Pause belongs to the audio timeline, not to the sentence queue.
///
/// Re-queueing the sentence on resume sounds like a stutter and also pays the
/// synthesis cost a second time. A long clip with a deliberately slow synth
/// makes the distinction observable without listening: resuming an existing
/// audio pointer advances promptly, while synthesizing the sentence again
/// cannot possibly do so inside this window.
#[test]
fn pause_and_resume_keep_the_exact_audio_pointer() {
    let _guard = exclusive();
    let source = long_chapters(common::isolated_home());
    let (mut app, mut terminal) = fixture_with(source, SYNTH_MS, LONG_CLIP_MS);
    start_reading_aloud(&mut app, &mut terminal);
    until(&mut app, &mut terminal, "the live audio pointer", |app| {
        app.voice_position()
            .is_some_and(|position| position.elapsed >= Duration::from_millis(100))
    });

    app.on_key(KeyEvent::from(KeyCode::Char(' ')));
    assert!(app.turn.paused(), "space should pause the audio timeline");
    assert!(
        !app.voice_sounding(),
        "paused audio must not report itself as audible"
    );
    let held = shown(&app).expect("a passage is in flight");
    let paused_at = app.voice_position().expect("a paused audio pointer");
    let paused_until = Instant::now() + Duration::from_millis(300);
    while Instant::now() < paused_until {
        tick(&mut app, &mut terminal);
        assert_eq!(
            shown(&app),
            Some(held),
            "tokens moved while the audio pointer was paused"
        );
        assert_eq!(
            app.voice_position().map(|position| position.elapsed),
            Some(paused_at.elapsed),
            "the media cursor moved while paused"
        );
    }

    app.on_key(KeyEvent::from(KeyCode::Char(' ')));
    let resumed = Instant::now();
    until(
        &mut app,
        &mut terminal,
        "the held audio pointer to move",
        |app| {
            app.voice_position().is_some_and(|position| {
                position.elapsed >= paused_at.elapsed + Duration::from_millis(100)
            })
        },
    );
    assert!(
        resumed.elapsed() < Duration::from_millis(650),
        "resume waited for the sentence to synthesize again instead of continuing its audio"
    );
}

#[test]
fn read_aloud_exposes_and_obeys_bracket_speed_keys() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture_with(
        long_chapters(common::isolated_home()),
        SYNTH_MS,
        LONG_CLIP_MS,
    );
    start_reading_aloud(&mut app, &mut terminal);
    until(&mut app, &mut terminal, "a live audio cursor", |app| {
        app.voice_position().is_some()
    });

    let deadline = Instant::now() + Duration::from_secs(8);
    let view = loop {
        tick(&mut app, &mut terminal);
        let view = screen(&terminal);
        if view.contains("stand-in") && view.contains("[1×]") {
            break view;
        }
        assert!(
            Instant::now() < deadline,
            "the transient mode notice never yielded to the transport:\n{view}"
        );
    };
    assert!(
        view.contains("[1×]"),
        "the live speed should carry its two bracket controls:\n{view}"
    );
    assert_eq!(
        view.matches("[1×]").count(),
        1,
        "speed state should appear once, next to the engine:\n{view}"
    );
    assert!(
        !view.contains("[ / ]"),
        "a second generic speed hint repeats the same control:\n{view}"
    );
    assert!(
        !view.contains("readio-1"),
        "the selected audio model should replace the fictional agent model:\n{view}"
    );
    assert_eq!(
        view.matches("stand-in").count(),
        1,
        "the audio model belongs in the model slot, not in two status regions:\n{view}"
    );

    app.on_key(KeyEvent::from(KeyCode::Char(']')));
    assert_eq!(app.multiplier(), 1.25, "] should make read-aloud faster");
    app.on_key(KeyEvent::from(KeyCode::Char('[')));
    assert_eq!(app.multiplier(), 1.0, "[ should make read-aloud slower");
}

/// Closing readio halfway through a spoken passage must not turn a restart into
/// a replay of the whole passage. The durable checkpoint is the current fine
/// sentence boundary; the live audio millisecond remains process-local.
#[test]
fn restarting_read_aloud_resumes_at_the_live_sentence() {
    let _guard = exclusive();
    let home = fixture_home();
    let path = book(&home, "spoken-checkpoint", 2, 8);
    let (mut app, mut terminal) = fixture_at_home(home, path.clone(), 20, CLIP_MS, "", 46.0, None);
    start_reading_aloud(&mut app, &mut terminal);

    let source = app
        .turn
        .passage_in_flight()
        .map(|(_, text, _)| text.to_string())
        .expect("streaming passage");
    let third = source.find("这一句是丙寅。").expect("third sentence");
    let inside_third = source[..third].chars().count() + 1;
    until(
        &mut app,
        &mut terminal,
        "the live voice to enter the third sentence",
        |app| {
            app.turn
                .streaming_passage()
                .is_some_and(|(_, released)| released >= inside_third)
        },
    );

    let id = app.book.as_ref().expect("open book").id.clone();
    let saved = app
        .store
        .get(&id)
        .cloned()
        .expect("read-aloud should have durable progress");
    drop(app);
    drop(terminal);

    let mut store = Store::ephemeral();
    store.record(&id, saved);
    let reopened = Book::load(Some(&path)).expect("reopen book");
    let mut restarted = App::new(Library::ephemeral(), store, Some(reopened), None);
    let mut terminal = Terminal::new(TestBackend::new(96, 30)).expect("terminal");
    until(
        &mut restarted,
        &mut terminal,
        "the restored audio cursor",
        |app| app.voice_position().is_some(),
    );

    let position = restarted.voice_position().expect("playing after restart");
    assert!(
        position.range.0 >= third,
        "restart replayed the passage from byte {} instead of resuming at byte {third}:\n{}",
        position.range.0,
        screen(&terminal)
    );
}

#[test]
fn an_explicit_chapter_jump_discards_the_spoken_checkpoint() {
    let _guard = exclusive();
    let home = fixture_home();
    let path = book(&home, "checkpoint-jump", 2, 8);
    let mut config = readio::config::Config::load().0;
    config.reading.mode = readio::mode::Mode::Manual;
    let _ = config.save();

    let book = Book::load(Some(&path)).expect("book");
    let id = book.id.clone();
    let mut store = Store::ephemeral();
    store.record(
        &id,
        Progress {
            title: book.title.clone(),
            speech: Some(SpeechCheckpoint {
                chapter: 0,
                para: 0,
                from: 54,
                anchor: "这一句是丙寅。".to_string(),
            }),
            ..Progress::default()
        },
    );
    let mut app = App::new(Library::ephemeral(), store, Some(book), None);
    let mut terminal = Terminal::new(TestBackend::new(96, 30)).expect("terminal");
    until(&mut app, &mut terminal, "the restored welcome", |app| {
        !app.turn.busy()
    });

    type_line(&mut app, "/goto 1");
    assert!(
        app.store
            .get(&id)
            .is_some_and(|progress| progress.speech.is_none()),
        "an explicit jump to the chapter start left the old spoken sentence armed"
    );
}

/// Playback speed belongs to the live audio cursor, not to synthesis.
///
/// A bracket press must keep the same clip and presentation timestamp alive.
/// Replacing the speaker resets that timestamp to `None`, creates a model wait,
/// and briefly gives the passage back to the ordinary character pacer.
#[test]
fn speed_change_keeps_the_same_audio_cursor_moving() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture_with(
        long_chapters(common::isolated_home()),
        SYNTH_MS,
        LONG_CLIP_MS,
    );
    start_reading_aloud(&mut app, &mut terminal);
    until(&mut app, &mut terminal, "a live audio cursor", |app| {
        app.voice_position()
            .is_some_and(|position| position.elapsed >= Duration::from_millis(100))
    });
    let before = app.voice_position().expect("a playback position");

    app.on_key(KeyEvent::from(KeyCode::Char(']')));
    let after_key = app
        .voice_position()
        .expect("changing playback speed must not discard the audio cursor");
    assert_eq!(after_key.id, before.id, "the playing clip was replaced");
    assert_eq!(
        after_key.range, before.range,
        "playback restarted from another sentence"
    );
    assert!(
        after_key.elapsed >= before.elapsed,
        "the audio pointer moved backwards"
    );

    until(
        &mut app,
        &mut terminal,
        "the same faster cursor to advance",
        |app| {
            app.voice_position().is_some_and(|position| {
                position.id == before.id
                    && position.range == before.range
                    && position.elapsed >= before.elapsed + Duration::from_millis(200)
            })
        },
    );
    assert_eq!(
        app.mode(),
        readio::mode::Mode::Speak,
        "changing speed must leave read-aloud as the only active pacing mode"
    );
}

/// The multiplier changes media time, not merely the label in the chrome.
#[test]
fn playback_rate_controls_the_live_media_clock() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture_with(
        long_chapters(common::isolated_home()),
        SYNTH_MS,
        LONG_CLIP_MS,
    );
    start_reading_aloud(&mut app, &mut terminal);
    until(&mut app, &mut terminal, "a live audio cursor", |app| {
        app.voice_position()
            .is_some_and(|position| position.elapsed >= Duration::from_millis(100))
    });

    // 1× → 1.25× → 1.5× → 2×.
    for _ in 0..3 {
        app.on_key(KeyEvent::from(KeyCode::Char(']')));
    }
    let before = app.voice_position().expect("a playback position");
    let wall_start = Instant::now();
    while wall_start.elapsed() < Duration::from_millis(300) {
        tick(&mut app, &mut terminal);
    }
    let after = app.voice_position().expect("the same playback position");
    let media_advance = after.elapsed.saturating_sub(before.elapsed);
    assert!(
        media_advance >= Duration::from_millis(450),
        "2× was only worth {media_advance:?} of media time in {:?} of wall time",
        wall_start.elapsed()
    );
}

/// Left and right belong to the speech transport when the composer is empty.
/// A synthesis window may contain a whole paragraph, so this deliberately
/// starts inside one and requires navigation to use textual sentence boundaries
/// rather than merely skipping the entire rendered clip.
#[test]
fn arrow_keys_move_the_audio_and_text_to_sentence_boundaries() {
    let _guard = exclusive();
    let home = common::isolated_home();
    let path = long_chapters(home);
    let (mut app, mut terminal) = fixture_with(path, SYNTH_MS, LONG_CLIP_MS);
    start_reading_aloud(&mut app, &mut terminal);
    let source = app
        .turn
        .passage_in_flight()
        .map(|(_, text, _)| text.to_string())
        .expect("streaming passage");
    let first = source.find("这一句是甲子。").expect("first sentence");
    let second = source.find("这一句是乙丑。").expect("second sentence");

    until(&mut app, &mut terminal, "the opening audio cursor", |app| {
        app.voice_position().is_some()
    });
    app.on_key(KeyEvent::from(KeyCode::Right));
    until(&mut app, &mut terminal, "the first body sentence", |app| {
        app.voice_position()
            .is_some_and(|position| position.range.0 == first)
    });
    app.on_key(KeyEvent::from(KeyCode::Right));
    until(&mut app, &mut terminal, "the second sentence", |app| {
        app.voice_position()
            .is_some_and(|position| position.range.0 == second)
    });
    let (_, released) = app
        .turn
        .streaming_passage()
        .expect("the passage stays on screen");
    assert!(
        released >= source[..second].chars().count(),
        "the audio jumped but the text stayed behind"
    );

    app.on_key(KeyEvent::from(KeyCode::Left));
    until(&mut app, &mut terminal, "the previous sentence", |app| {
        app.voice_position()
            .is_some_and(|position| position.range.0 == first)
    });

    let deadline = Instant::now() + Duration::from_secs(8);
    let view = loop {
        tick(&mut app, &mut terminal);
        let view = screen(&terminal);
        if view.contains("←") && view.contains("→") {
            break view;
        }
        assert!(
            Instant::now() < deadline,
            "the transient mode notice never yielded to sentence transport:\n{view}"
        );
    };
    assert!(
        view.contains("←") && view.contains("→"),
        "sentence transport keys should be discoverable:\n{view}"
    );
}

/// A seek has a textual destination even while the replacement clip is still
/// rendering. Keys pressed in that interval used to be discarded because the
/// old audio clock had already been cleared. On a heavy model this interval is
/// long enough to make repeated left presses look as if they broke playback.
#[test]
fn repeated_left_during_seek_clamps_at_the_first_sentence_and_resumes() {
    let _guard = exclusive();
    let home = common::isolated_home();
    let path = long_chapters(home);
    // The renderer is deliberately slow. Returning to a fine sentence creates
    // one new contextual window, so the deadline allows that 1.5s render but
    // not another 1.5s spent waiting for obsolete work first.
    let (mut app, mut terminal) = fixture_with(path, 1_500, LONG_CLIP_MS);
    start_reading_aloud(&mut app, &mut terminal);
    let source = app
        .turn
        .passage_in_flight()
        .map(|(_, text, _)| text.to_string())
        .expect("streaming passage");
    let first_body = source.find("这一句是甲子。").expect("first body sentence");
    let second_body = source.find("这一句是乙丑。").expect("second body sentence");
    let first = readio::voice::sentence::sentences(&source)
        .first()
        .expect("first textual sentence")
        .range
        .0;

    until(&mut app, &mut terminal, "the opening audio cursor", |app| {
        app.voice_position().is_some()
    });
    app.on_key(KeyEvent::from(KeyCode::Right));
    until(&mut app, &mut terminal, "the first body sentence", |app| {
        app.voice_position()
            .is_some_and(|position| position.range.0 == first_body)
    });
    app.on_key(KeyEvent::from(KeyCode::Right));
    until(&mut app, &mut terminal, "the second body sentence", |app| {
        app.voice_position()
            .is_some_and(|position| position.range.0 == second_body)
    });

    // The first press clears the live clock in the old implementation. Every
    // press after it must still move the pending textual cursor, not disappear.
    for _ in 0..8 {
        app.on_key(KeyEvent::from(KeyCode::Left));
    }

    let deadline = Instant::now() + Duration::from_millis(2_200);
    let mut resumed_at_first = false;
    while Instant::now() < deadline {
        tick(&mut app, &mut terminal);
        if app
            .voice_position()
            .is_some_and(|position| position.range.0 == first)
        {
            resumed_at_first = true;
            break;
        }
    }
    assert!(
        resumed_at_first,
        "rapid left presses were lost while the replacement clip rendered; \
         playback never resumed at the clamped first sentence\n{}",
        screen(&terminal)
    );
}

/// Enter must not take the pace away from the voice.
///
/// `⏎` during a turn means "get on with it", and it did that by setting the
/// reveal to two and a half thousand characters a second — and by claiming the
/// pace for the rest of the turn, so every clip that started afterwards was
/// ignored. In read-aloud that is a paragraph landing whole while the voice is
/// still on its first line: the mode stops being itself because of one
/// keypress, silently, which is the one thing read-aloud is not allowed to do.
///
/// Stated as the invariant rather than as a character count, because the
/// character count depends on how long the clips happen to be: in read-aloud
/// the pace belongs to the clip, whatever the reader presses.
#[test]
fn enter_does_not_take_the_pace_away_from_the_clip() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture(long_chapters(common::isolated_home()));
    start_reading_aloud(&mut app, &mut terminal);
    until(&mut app, &mut terminal, "a clip to set the pace", |app| {
        app.turn.cps() < 100.0
    });

    // The reader hurries readio along, the way they would in any other mode.
    app.on_key(KeyEvent::from(KeyCode::Enter));
    for _ in 0..30 {
        tick(&mut app, &mut terminal);
    }

    assert!(
        app.turn.cps() < 100.0,
        "the reveal is running at {:.0} characters a second, which is nobody's \
         reading voice: the keypress took the pace away from the clip\n{}",
        app.turn.cps(),
        screen(&terminal)
    );
}

/// Rendering ahead across a paragraph boundary: the seam that was dead air.
///
/// Within a paragraph the pipeline covers the sentence boundaries — every
/// sentence is handed over at once and rendered `tts.prefetch` ahead of the one
/// playing. Between one paragraph and the next there was nothing to render: the
/// following paragraph does not exist until the turn that carries it starts, so
/// the reader waited out a thinking line, a tool call and then a whole
/// synthesis in silence, every few hundred characters. That is what a listener
/// hears as "it keeps stopping".
///
/// Now the turn is asked where it is going while it is still running, and the
/// opening sentence of the paragraph after this one is rendered underneath the
/// last clip of this one. Measured as silence, because silence is what the
/// reader experiences: not "is anything queued" but "is anything playing".
///
/// A chapter per sentence, so the window is almost nothing but boundaries, and
/// an engine that takes two seconds a sentence, so a boundary it fails to cover
/// is unmistakable. On the build that fixed this: 18% of frames silent, longest
/// gap 1.3s — the thinking line and the tool call, which are meant to be there.
/// On the behaviour it replaced: 43%, longest gap 3.3s.
#[test]
fn the_voice_does_not_fall_silent_between_turns() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture_with(
        tiny_chapters(common::isolated_home()),
        SLOW_SYNTH_MS,
        LONG_CLIP_MS,
    );
    start_reading_aloud(&mut app, &mut terminal);
    // The first clip of the session is the one nothing can cover; the question
    // here is about the boundaries after it.
    until(&mut app, &mut terminal, "the voice", |app| {
        app.voice_sounding()
    });

    let mut quiet = 0usize;
    let mut frames = 0usize;
    let mut run = Duration::ZERO;
    let mut longest = Duration::ZERO;
    let mut fell_silent = Instant::now();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(14) {
        tick(&mut app, &mut terminal);
        frames += 1;
        if app.voice_sounding() {
            run = Duration::ZERO;
        } else {
            if run.is_zero() {
                fell_silent = Instant::now();
            }
            quiet += 1;
            run = fell_silent.elapsed();
            longest = longest.max(run);
        }
    }

    assert!(frames > 100, "only {frames} frames: the clock is wrong");
    assert!(
        quiet * 10 < frames * 3,
        "the voice had nothing to say in {quiet} of {frames} frames: \
         the next paragraph is not being rendered until the silence has \
         already started\n{}",
        screen(&terminal)
    );
    // The share can be spread thin by a slow machine; a single gap this long is
    // a boundary that nothing was rendering into.
    assert!(
        longest < Duration::from_millis(2_200),
        "{:.1}s of unbroken silence: a paragraph boundary went uncovered\n{}",
        longest.as_secs_f64(),
        screen(&terminal)
    );
}
