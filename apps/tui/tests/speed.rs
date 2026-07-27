//! Audiobook-style playback speed: the `^r` cycle, `/rate`, and what the reader
//! is told when read-aloud is not running.

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use readio::app::App;
use readio::book::Book;
use readio::library::Library;
use readio::store::Store;

/// These tests share one `config.yaml`, so they take turns.
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fixture() -> (App, Terminal<TestBackend>) {
    common::isolated_home();
    let mut config = readio::config::Config::load().0;
    config.tts.rate = 1.0;
    config.tts.enabled = false;
    config.tts.output.allow.clear();
    let _ = config.save();
    let book = Book::load(None).expect("sample book");
    let app = App::new(Library::ephemeral(), Store::ephemeral(), Some(book), None);
    let terminal = Terminal::new(TestBackend::new(96, 30)).expect("terminal");
    (app, terminal)
}

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

fn draw(app: &mut App, terminal: &mut Terminal<TestBackend>, frames: usize) -> String {
    for _ in 0..frames {
        app.on_tick();
        terminal.draw(|frame| app.draw(frame)).expect("draw");
        std::thread::sleep(std::time::Duration::from_millis(4));
    }
    screen(terminal)
}

fn settle(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    for _ in 0..600 {
        draw(app, terminal, 1);
        if !app.turn.busy() {
            return;
        }
    }
}

fn type_line(app: &mut App, line: &str) {
    for c in line.chars() {
        app.on_key(KeyEvent::from(KeyCode::Char(c)));
    }
    app.on_key(KeyEvent::from(KeyCode::Enter));
}

fn ctrl(app: &mut App, c: char) {
    app.on_key(KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: ratatui::crossterm::event::KeyEventState::NONE,
    });
}

fn saved_rate() -> f32 {
    readio::config::Config::load().0.tts.rate
}

#[test]
fn ctrl_r_cycles_through_the_usual_speeds() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    ctrl(&mut app, 'r');
    let view = draw(&mut app, &mut terminal, 3);
    assert!(
        view.contains("1.25×"),
        "the new speed should be named on screen:\n{view}"
    );
    assert_eq!(saved_rate(), 1.25, "and written to the config");

    ctrl(&mut app, 'r');
    draw(&mut app, &mut terminal, 3);
    assert_eq!(saved_rate(), 1.5);

    ctrl(&mut app, 'r');
    draw(&mut app, &mut terminal, 3);
    assert_eq!(saved_rate(), 2.0);

    // Past the top it wraps, the way a phone's speed button does.
    ctrl(&mut app, 'r');
    draw(&mut app, &mut terminal, 3);
    assert_eq!(saved_rate(), 0.75);
}

#[test]
fn a_speed_change_with_speech_off_says_when_it_will_apply() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    ctrl(&mut app, 'r');
    let view = draw(&mut app, &mut terminal, 3);
    assert!(
        view.contains("下次开启朗读时生效"),
        "silence must not look like nothing happened:\n{view}"
    );
}

#[test]
fn rate_takes_an_exact_value_and_rejects_the_impossible() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/rate 1.75");
    draw(&mut app, &mut terminal, 3);
    assert_eq!(saved_rate(), 1.75, "off-ladder values are still allowed");

    // A multiplier typed the way it is displayed should work too.
    type_line(&mut app, "/rate 2×");
    draw(&mut app, &mut terminal, 3);
    assert_eq!(saved_rate(), 2.0);

    type_line(&mut app, "/rate 9");
    let view = draw(&mut app, &mut terminal, 3);
    assert_eq!(saved_rate(), 2.0, "an impossible speed changes nothing");
    assert!(
        view.contains("/rate") && view.contains("^r"),
        "the usage line should mention both ways to set it:\n{view}"
    );
}

#[test]
fn bare_rate_shows_where_you_are_and_what_else_there_is() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/rate 1.5");
    draw(&mut app, &mut terminal, 3);
    type_line(&mut app, "/rate");
    let view = draw(&mut app, &mut terminal, 3);

    assert!(
        view.contains("1.5×") && view.contains("0.75×") && view.contains("2×"),
        "the current speed and the ladder should both be visible:\n{view}"
    );
}
