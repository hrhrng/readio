//! Manual, auto-reading and read-aloud are three parallel TUI modes.
//!
//! Read-aloud is never an alias for auto: if its engine is slow or unavailable,
//! tokens wait or the turn stops while the mode remains read-aloud.

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use readio::app::App;
use readio::book::Book;
use readio::library::Library;
use readio::mode::Mode;
use readio::store::Store;

/// These tests share one `config.yaml`, so they take turns.
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fixture() -> (App, Terminal<TestBackend>) {
    common::isolated_home();
    let mut config = readio::config::Config::load().0;
    config.reading.mode = Mode::Manual;
    config.reading.speed = 400.0;
    config.voice.enabled = false;
    config.voice.engine = "local".to_string();
    config.voice.engines.insert(
        "local".to_string(),
        readio::voice::config::EngineSpec {
            synth: "sh -c true".to_string(),
            about: "test voice".to_string(),
            ..Default::default()
        },
    );
    let _ = config.save();
    let book = Book::load(None).expect("sample book");
    let app = App::new(Library::ephemeral(), Store::ephemeral(), Some(book), None);
    let terminal = Terminal::new(TestBackend::new(96, 30)).expect("terminal");
    (app, terminal)
}

fn fixture_without_voice() -> (App, Terminal<TestBackend>) {
    common::isolated_home();
    let mut config = readio::config::Config::load().0;
    config.reading.mode = Mode::Manual;
    config.voice.enabled = false;
    config.voice.engine = "not-installed".to_string();
    let _ = config.save();
    let book = Book::load(None).expect("sample book");
    let app = App::new(Library::ephemeral(), Store::ephemeral(), Some(book), None);
    let terminal = Terminal::new(TestBackend::new(96, 30)).expect("terminal");
    (app, terminal)
}

fn draw(app: &mut App, terminal: &mut Terminal<TestBackend>, frames: usize) -> String {
    for _ in 0..frames {
        app.on_tick();
        terminal.draw(|frame| app.draw(frame)).expect("draw");
        std::thread::sleep(std::time::Duration::from_millis(4));
    }
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

fn settle(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.turn.busy() && std::time::Instant::now() < deadline {
        draw(app, terminal, 1);
    }
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::from(code));
}

fn type_line(app: &mut App, line: &str) {
    for c in line.chars() {
        press(app, KeyCode::Char(c));
    }
    press(app, KeyCode::Enter);
}

fn shift_tab(app: &mut App) {
    app.on_key(KeyEvent {
        code: KeyCode::BackTab,
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: ratatui::crossterm::event::KeyEventState::NONE,
    });
}

fn ctrl_s(app: &mut App) {
    app.on_key(KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: ratatui::crossterm::event::KeyEventState::NONE,
    });
}

#[test]
fn ctrl_s_enters_read_aloud_and_returns_to_manual() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    ctrl_s(&mut app);

    assert!(app.speech_enabled(), "Voice should be on");
    assert_eq!(app.mode(), Mode::Speak);
    let saved = readio::config::Config::load().0;
    assert_eq!(saved.reading.mode, Mode::Speak);
    assert!(saved.voice.enabled);

    ctrl_s(&mut app);
    assert_eq!(app.mode(), Mode::Manual);
    assert!(!app.speech_enabled());
}

#[test]
fn ctrl_s_returns_to_auto_when_that_was_the_previous_mode() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    type_line(&mut app, "/mode auto");

    ctrl_s(&mut app);
    assert!(app.speech_enabled());
    assert_eq!(app.mode(), Mode::Speak);

    ctrl_s(&mut app);
    assert!(!app.speech_enabled());
    assert_eq!(app.mode(), Mode::Auto);
}

#[test]
fn shift_tab_cycles_manual_auto_and_read_aloud() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    shift_tab(&mut app);
    assert_eq!(app.mode(), Mode::Auto);

    shift_tab(&mut app);
    assert_eq!(app.mode(), Mode::Speak);
    assert!(app.speech_enabled());

    shift_tab(&mut app);
    assert_eq!(app.mode(), Mode::Manual);
    assert!(!app.speech_enabled());
}

#[test]
fn unavailable_tts_stays_in_read_aloud_instead_of_falling_back() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture_without_voice();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/mode aloud");
    let view = draw(&mut app, &mut terminal, 2);

    assert_eq!(
        app.mode(),
        Mode::Speak,
        "an unavailable engine must stop read-aloud, never turn it into auto or manual"
    );
    assert!(
        !app.turn.busy(),
        "without a voice there is no clock allowed to release tokens"
    );
    assert!(
        view.contains("朗读"),
        "the mode chip should continue to say what the reader selected:\n{view}"
    );
    assert_eq!(
        readio::config::Config::load().0.reading.mode,
        Mode::Speak,
        "the no-downgrade promise must survive restart"
    );
}

#[test]
fn a_fresh_reader_is_in_manual_mode() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    let view = draw(&mut app, &mut terminal, 3);

    assert_eq!(app.mode(), Mode::Manual);
    assert!(
        view.contains("手动"),
        "the chip should name the mode:\n{view}"
    );
}

#[test]
fn shift_tab_moves_to_auto_scroll_and_starts_reading() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    shift_tab(&mut app);
    let view = draw(&mut app, &mut terminal, 3);

    assert_eq!(app.mode(), Mode::Auto);
    assert!(app.turn.busy(), "auto-scroll should start moving by itself");
    assert!(
        view.contains("自动滚动"),
        "the switch should explain itself:\n{view}"
    );
    assert!(
        view.contains("tok/s"),
        "and name the speed it obeys:\n{view}"
    );
}

#[test]
fn auto_scroll_keeps_queueing_passages_and_manual_does_not() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/mode auto");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let start = app.pos;
    // Two turns' worth of reading with nobody pressing anything.
    while app.pos == start && std::time::Instant::now() < deadline {
        draw(&mut app, &mut terminal, 1);
    }
    let after_first = app.pos;
    while app.pos == after_first && std::time::Instant::now() < deadline {
        draw(&mut app, &mut terminal, 1);
    }
    assert!(
        (app.pos.chapter, app.pos.para) > (after_first.chapter, after_first.para),
        "auto-scroll stopped at one passage"
    );

    type_line(&mut app, "/mode manual");
    settle(&mut app, &mut terminal);
    let parked = app.pos;
    draw(&mut app, &mut terminal, 20);
    assert_eq!(app.pos, parked, "manual mode moved without being asked");
}

#[test]
fn in_manual_mode_the_bottom_of_the_page_loads_more() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    press(&mut app, KeyCode::Enter);
    settle(&mut app, &mut terminal);
    let read_so_far = app.pos;

    // At the tail, down means "more book" — the way scrolling to the end of a
    // list loads the next page.
    press(&mut app, KeyCode::Down);
    settle(&mut app, &mut terminal);

    assert!(
        (app.pos.chapter, app.pos.para) > (read_so_far.chapter, read_so_far.para),
        "↓ at the bottom should have loaded the next passage"
    );
}

#[test]
fn pausing_does_not_quietly_drop_out_of_auto_scroll() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/mode auto");
    draw(&mut app, &mut terminal, 3);
    assert_eq!(app.mode(), Mode::Auto);

    press(&mut app, KeyCode::Esc);
    let view = draw(&mut app, &mut terminal, 3);
    assert!(app.turn.paused(), "esc should pause");
    assert_eq!(
        app.mode(),
        Mode::Auto,
        "a pause is not a mode change:\n{view}"
    );

    press(&mut app, KeyCode::Enter);
    draw(&mut app, &mut terminal, 3);
    assert!(!app.turn.paused(), "enter should resume");
    assert_eq!(app.mode(), Mode::Auto);
}

#[test]
fn the_mode_survives_into_the_config_file() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/mode auto");
    draw(&mut app, &mut terminal, 3);
    assert_eq!(
        readio::config::Config::load().0.reading.mode,
        Mode::Auto,
        "auto-scroll should still be on next time"
    );

    type_line(&mut app, "/mode manual");
    draw(&mut app, &mut terminal, 3);
    assert_eq!(readio::config::Config::load().0.reading.mode, Mode::Manual);
}

/// The mode is a setting, so readio starts the way it was left. It used to be
/// half a setting: read-aloud came back after a restart because `tts.enabled`
/// was read on every frame, and auto-scroll did not, because nothing ever read
/// `reading.auto` back — the same switch, remembered in one direction only.
#[test]
fn the_mode_comes_back_on_the_next_launch() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/mode auto");
    draw(&mut app, &mut terminal, 3);

    // A second launch against the same host directory, which is what restarting
    // readio amounts to.
    let book = Book::load(None).expect("sample book");
    let restarted = App::new(Library::ephemeral(), Store::ephemeral(), Some(book), None);
    assert_eq!(
        restarted.mode(),
        Mode::Auto,
        "auto-scroll was a choice, not a mood"
    );
}

/// Opening a book is not the reader changing their mind about how they read.
/// This used to reset auto-scroll to manual while leaving read-aloud alone,
/// because one lived on the app and the other in the config.
#[test]
fn opening_a_book_leaves_the_mode_alone() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/mode auto");
    draw(&mut app, &mut terminal, 3);
    type_line(&mut app, "/sample");
    settle(&mut app, &mut terminal);

    assert_eq!(
        app.mode(),
        Mode::Auto,
        "the mode is not a property of a book"
    );
}

#[test]
fn a_mode_nobody_can_spell_is_explained_not_ignored() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/mode sideways");
    let view = draw(&mut app, &mut terminal, 3);

    assert_eq!(app.mode(), Mode::Manual);
    assert!(
        view.contains("/mode manual | auto | aloud"),
        "expected the usage line:\n{view}"
    );
}

#[test]
fn bare_mode_reports_where_things_stand() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/mode");
    let view = draw(&mut app, &mut terminal, 3);

    assert!(view.contains("手动"), "expected the mode named:\n{view}");
    assert!(
        view.contains("朗读"),
        "the mode picker must keep read-aloud beside manual and auto:\n{view}"
    );
    assert!(
        view.contains("shift+tab"),
        "expected the way to change it:\n{view}"
    );
}
