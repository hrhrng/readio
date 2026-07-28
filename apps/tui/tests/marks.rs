//! Bookmarks: keeping a place, going back to it, and dropping it.
//!
//! The interesting case is the last test. A mark is stored as a character
//! offset, so it still points at the same sentence after readio changes its mind
//! about where chapters begin — which it did, the release this file arrived in.

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use readio::app::App;
use readio::book::Book;
use readio::library::Library;
use readio::store::{Mark, Progress, Store};

fn fixture() -> (App, Terminal<TestBackend>) {
    common::isolated_home();
    let book = Book::load(None).expect("sample book");
    let mut app = App::new(Library::ephemeral(), Store::ephemeral(), Some(book), None);
    // Reading at reading speed would make this test a minute long.
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

fn draw(app: &mut App, terminal: &mut Terminal<TestBackend>) -> String {
    app.on_tick();
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    screen(terminal)
}

fn settle(app: &mut App, terminal: &mut Terminal<TestBackend>) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.turn.busy() && std::time::Instant::now() < deadline {
        draw(app, terminal);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    draw(app, terminal)
}

fn command(app: &mut App, terminal: &mut Terminal<TestBackend>, line: &str) -> String {
    for c in line.chars() {
        app.on_key(KeyEvent::from(KeyCode::Char(c)));
    }
    app.on_key(KeyEvent::from(KeyCode::Enter));
    settle(app, terminal)
}

#[test]
fn a_mark_with_no_note_is_named_after_what_is_there() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    command(&mut app, &mut terminal, "/mark");
    let view = command(&mut app, &mut terminal, "/marks");
    assert!(
        view.contains("1 marks") || view.contains("1  ch1"),
        "the listing should show the one mark: {view}"
    );
    assert!(view.contains('%'), "a mark says how far in it is: {view}");
}

#[test]
fn a_note_becomes_the_marks_name() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    command(&mut app, &mut terminal, "/mark 回来读这段");
    let view = command(&mut app, &mut terminal, "/marks");
    assert!(view.contains("回来读这段"), "the note is the name: {view}");
}

#[test]
fn marking_the_same_place_twice_renames_one_mark() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    command(&mut app, &mut terminal, "/mark first");
    command(&mut app, &mut terminal, "/mark second");
    let view = command(&mut app, &mut terminal, "/marks");
    assert!(view.contains("second"), "the newer name wins: {view}");
    assert!(
        view.contains("1 marks"),
        "and it is one mark, not two — the command echoes above are not the listing: {view}"
    );
}

#[test]
fn asking_for_a_mark_that_is_not_there_says_how_many_are() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    let view = command(&mut app, &mut terminal, "/marks");
    assert!(
        view.contains("/mark"),
        "with nothing marked, point at the command that marks: {view}"
    );

    command(&mut app, &mut terminal, "/mark somewhere");
    let view = command(&mut app, &mut terminal, "/marks 7");
    assert!(view.contains('1'), "say how many there really are: {view}");
}

#[test]
fn a_mark_can_be_dropped() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    command(&mut app, &mut terminal, "/mark 待删");
    let view = command(&mut app, &mut terminal, "/unmark 1");
    assert!(view.contains("待删"), "say what was dropped: {view}");
    let view = command(&mut app, &mut terminal, "/marks");
    assert!(
        !view.contains("marks  ·"),
        "with nothing left there is no listing to show: {view}"
    );
}

/// A mark made before chapters were cut differently still lands in the right
/// place, because the offset is what was stored.
#[test]
fn a_stale_chapter_index_does_not_send_a_mark_to_the_wrong_page() {
    common::isolated_home();
    let book = Book::load(None).expect("sample book");
    let chars = book.chars_before(2, 0) as u64;

    let mut store = Store::ephemeral();
    store.record(
        &book.id,
        Progress {
            title: book.title.clone(),
            path: None,
            chapter: 0,
            para: 0,
            chars_read: 0,
            sessions: 1,
            updated: 0,
            marks: vec![Mark {
                chars,
                // What an old state file holds: indices from a coarser cut.
                chapter: 0,
                para: 99,
                label: "old".into(),
                at: 0,
            }],
        },
    );

    let mut app = App::new(Library::ephemeral(), store, Some(book), None);
    app.turn.set_cps(4_000.0);
    let mut terminal = Terminal::new(TestBackend::new(96, 30)).expect("terminal");
    settle(&mut app, &mut terminal);
    for c in "/marks 1".chars() {
        app.on_key(KeyEvent::from(KeyCode::Char(c)));
    }
    app.on_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(
        app.position(),
        (2, 0),
        "the offset decides where the mark is, not the stale index beside it"
    );
}
