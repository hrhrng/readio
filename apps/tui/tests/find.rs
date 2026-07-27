//! Searching as a reader expects it: true counts, and hits you can walk to.

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use readio::app::App;
use readio::book::Book;
use readio::library::Library;
use readio::store::Store;
use readio::theme::theme;

fn fixture() -> (App, Terminal<TestBackend>) {
    common::isolated_home();
    let book = Book::load(None).expect("sample book");
    let mut app = App::new(Library::ephemeral(), Store::ephemeral(), Some(book), None);
    // Each jump reads a passage; at reading speed this test would spend half a
    // minute watching text arrive.
    app.turn.set_cps(4_000.0);
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
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    screen(terminal)
}

fn settle(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.turn.busy() && std::time::Instant::now() < deadline {
        draw(app, terminal, 1);
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

/// Cells painted with the deep wash — the same one the read-aloud cursor uses.
fn focused_cells(terminal: &Terminal<TestBackend>) -> usize {
    let deep = theme().bg_speaking_word;
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .filter(|cell| cell.bg == deep)
        .count()
}

/// A term the sample book uses more than once, taken from the book itself so the
/// test does not depend on which language the sample is in.
fn needle(book: &Book) -> String {
    let text = book.chapters[0].paras[1].text();
    text.chars().skip(1).take(2).collect()
}

#[test]
fn a_search_reports_the_whole_truth_about_how_often_a_term_appears() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    let book = Book::load(None).expect("sample book");
    let term = needle(&book);
    let hits = book.search(&term, 12);

    type_line(&mut app, &format!("/find {term}"));
    settle(&mut app, &mut terminal);
    let view = draw(&mut app, &mut terminal, 2);

    assert!(
        view.contains(&hits.total.to_string()),
        "the true occurrence count ({}) should be on screen for {term:?}:\n{view}",
        hits.total
    );
    assert!(
        view.contains(&hits.paragraphs.to_string()),
        "so should the number of paragraphs ({}):\n{view}",
        hits.paragraphs
    );
}

#[test]
fn typing_a_number_goes_to_that_hit_and_lights_the_term_up() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    let book = Book::load(None).expect("sample book");
    let term = needle(&book);

    type_line(&mut app, &format!("/find {term}"));
    settle(&mut app, &mut terminal);
    assert_eq!(focused_cells(&terminal), 0, "nothing is focused yet");

    type_line(&mut app, "1");
    let view = draw(&mut app, &mut terminal, 4);
    assert!(
        view.contains("1/") || view.contains("Hit 1"),
        "the jump should say which hit it went to:\n{view}"
    );

    settle(&mut app, &mut terminal);
    draw(&mut app, &mut terminal, 2);
    assert!(
        focused_cells(&terminal) > 0,
        "the term should be washed where it was found:\n{}",
        screen(&terminal)
    );
}

#[test]
fn ctrl_g_walks_the_hits_and_says_when_it_wraps() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    let book = Book::load(None).expect("sample book");
    let term = needle(&book);
    let count = book.search(&term, 12).shown.len();

    type_line(&mut app, &format!("/find {term}"));
    settle(&mut app, &mut terminal);

    // Walk past the end: the last step has to wrap and admit it.
    let mut wrapped = false;
    for _ in 0..count + 1 {
        ctrl(&mut app, 'g');
        let view = draw(&mut app, &mut terminal, 2);
        if view.contains("回到第 1 处") || view.contains("back to the first") {
            wrapped = true;
        }
        settle(&mut app, &mut terminal);
    }
    assert!(wrapped, "walking past the last hit should wrap and say so");
}

#[test]
fn walking_hits_without_a_search_explains_itself() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    ctrl(&mut app, 'g');
    let view = draw(&mut app, &mut terminal, 3);
    assert!(
        view.contains("/find"),
        "the key should point at the command that makes it useful:\n{view}"
    );
}

#[test]
fn a_term_that_is_not_there_leaves_no_search_to_walk() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/find readio-no-such-term-xyz");
    settle(&mut app, &mut terminal);
    ctrl(&mut app, 'g');
    let view = draw(&mut app, &mut terminal, 3);
    assert!(
        view.contains("/find"),
        "a failed search should not leave a phantom hit list:\n{view}"
    );
}
