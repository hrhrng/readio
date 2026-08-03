//! The select above the composer: the one place readio asks a question.
//!
//! Books, chapters, bookmarks, effort levels, modes and paths are all chosen the
//! same way — a list above the prompt, arrow keys, ⏎ — rather than printed into
//! the transcript as a numbered listing the reader has to transcribe back. These
//! tests hold that shape in place, and hold the two rules that make it usable: a
//! select never touches what the reader was typing, and a slash always begins a
//! command no matter what is open.

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use readio::app::App;
use readio::book::Book;
use readio::library::Library;
use readio::store::Store;

fn fixture() -> (App, Terminal<TestBackend>) {
    common::isolated_home();
    let book = Book::load(None).expect("sample book");
    let mut app = App::new(Library::ephemeral(), Store::ephemeral(), Some(book), None);
    app.turn.set_cps(4_000.0);
    let terminal = Terminal::new(TestBackend::new(96, 34)).expect("terminal");
    (app, terminal)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::from(code)
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
}

fn draw(app: &mut App, terminal: &mut Terminal<TestBackend>) -> String {
    app.on_tick();
    terminal.draw(|frame| app.draw(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            let mut row = String::new();
            let mut x = 0u16;
            while x < buffer.area.width {
                let cell = &buffer[(x, y)];
                row.push_str(cell.symbol());
                x += unicode_width::UnicodeWidthStr::width(cell.symbol()).max(1) as u16;
            }
            row.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    type_text(app, line);
    app.on_key(key(KeyCode::Enter));
    settle(app, terminal)
}

/// The bug this file was written for: opening a select used to write `/effort `
/// into the composer and read the rows back out of it, so the reader was left
/// staring at a command they never typed, on a line they could not use.
#[test]
fn a_select_keeps_its_state_out_of_the_composer() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    // A bare `/effort` asks which level, rather than printing six of them.
    let view = command(&mut app, &mut terminal, "/effort");
    assert!(
        view.contains("Minimal") || view.contains("Xhigh"),
        "the ladder is on offer:\n{view}"
    );
    assert!(
        !view.contains("│❯ /effort"),
        "and not by hijacking the prompt line:\n{view}"
    );

    // Letters narrow the select instead of reaching the composer behind it.
    type_text(&mut app, "max");
    let view = draw(&mut app, &mut terminal);
    assert!(
        !view.contains("│❯ max"),
        "a select owns the letters while it is open:\n{view}"
    );

    // esc declines the question and leaves nothing behind.
    app.on_key(key(KeyCode::Esc));
    let view = draw(&mut app, &mut terminal);
    assert!(
        !view.contains("Minimal") && !view.contains("/effort "),
        "esc closes the select and leaves no residue:\n{view}"
    );

    // And the composer works again, from empty.
    type_text(&mut app, "after");
    let view = draw(&mut app, &mut terminal);
    assert!(
        view.contains("after"),
        "the prompt is the reader's again:\n{view}"
    );
}

/// A select is open the moment readio starts, so this is not a corner case: it
/// is what happens to the first thing a reader ever types.
#[test]
fn a_slash_always_begins_a_command_even_with_a_select_open() {
    common::isolated_home();
    let mut app = App::new(Library::ephemeral(), Store::ephemeral(), None, None);
    let mut terminal = Terminal::new(TestBackend::new(96, 34)).expect("terminal");
    settle(&mut app, &mut terminal);

    type_text(&mut app, "/sample");
    let view = draw(&mut app, &mut terminal);
    assert!(
        view.contains("/sample"),
        "the slash escapes the select and reaches the composer:\n{view}"
    );

    app.on_key(key(KeyCode::Enter));
    let view = settle(&mut app, &mut terminal);
    assert!(app.book.is_some(), "and the command runs as typed:\n{view}");
}

/// Typing in a select narrows it. The text has to be visible somewhere, and the
/// composer is not that somewhere.
#[test]
fn typing_narrows_a_select_and_shows_what_was_typed() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    command(&mut app, &mut terminal, "/toc");

    // A select row carries a length in characters; the reading plan in the
    // transcript above counts tokens. That is how these assertions tell the two
    // listings apart.
    let all = draw(&mut app, &mut terminal);
    assert!(all.contains("392字"), "the chapters are on offer:\n{all}");

    type_text(&mut app, "2");
    let view = draw(&mut app, &mut terminal);
    assert!(
        view.contains("筛选：2") || view.contains("filter: 2"),
        "the narrowing text is echoed with the rows:\n{view}"
    );
    assert!(
        !view.contains("392字"),
        "and the rows it excludes are gone:\n{view}"
    );

    // Backspace walks it back out again.
    app.on_key(key(KeyCode::Backspace));
    let view = draw(&mut app, &mut terminal);
    assert!(
        !view.contains("筛选：") && !view.contains("filter: "),
        "an empty filter is not a filter:\n{view}"
    );
    assert!(view.contains("392字"), "and every chapter is back:\n{view}");
}

/// `/plan` is the chapter picker, not a request to print or execute a plan in
/// the transcript. Reintroducing the old `flow::plan` branch makes this fail:
/// there are no chapter rows above the prompt and the turn becomes busy.
#[test]
fn plan_opens_the_chapter_select() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    let view = command(&mut app, &mut terminal, "/plan");

    assert!(
        view.contains("392字"),
        "/plan should offer chapters in the select above the prompt:\n{view}"
    );
    assert!(
        !app.turn.busy(),
        "/plan must not enqueue a plan behind the current turn"
    );
}

/// A filter that matches nothing is a dead end: the reader sees an empty box and
/// cannot tell whether the key registered.
#[test]
fn a_filter_that_would_empty_the_list_is_refused() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    command(&mut app, &mut terminal, "/toc");

    type_text(&mut app, "zzzz");
    let view = draw(&mut app, &mut terminal);
    assert!(
        view.contains("1.") || view.contains("2."),
        "the rows stay rather than emptying:\n{view}"
    );
}

/// Choosing from a select is not typing a command, so the transcript records the
/// result and not the command the reader never wrote.
#[test]
fn choosing_a_row_records_the_result_not_the_command() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    command(&mut app, &mut terminal, "/toc");
    // The select lands on the chapter in force, so a step down is one chapter on
    // from wherever the reader happens to be.
    let before = app.position().0;

    app.on_key(key(KeyCode::Down));
    app.on_key(key(KeyCode::Enter));
    // Read the position before settling: a reading turn would carry on past the
    // chapter this test is about.
    let after = app.position().0;
    let view = settle(&mut app, &mut terminal);

    assert_eq!(after, before + 1, "the next chapter opened:\n{view}");
    assert!(
        !view.contains(&format!("/goto {}", before + 2)),
        "and no invented command is echoed:\n{view}"
    );
}

/// A bare `/import` is a question — which file? — and it is answered in the
/// composer rather than in a select, because a path is text: the reader may want
/// to edit it, and it takes a `--copy` or `--link` after it.
#[test]
fn a_bare_import_opens_the_path_menu_at_home() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    let view = command(&mut app, &mut terminal, "/import");
    assert!(
        view.contains("/import ~/"),
        "the line is seeded with a real directory:\n{view}"
    );
    assert!(
        view.contains("目录") || view.contains("directory"),
        "and the menu lists what is in it:\n{view}"
    );
}

/// `/import` is the one argument that cannot come from a fixed list, so the rows
/// come from the filesystem: directories to walk into, and only what readio can
/// open.
#[test]
fn import_offers_paths_and_walks_into_directories() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    let dir = std::env::temp_dir().join(format!("readio-select-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("shelf")).expect("a directory");
    std::fs::write(dir.join("shelf/calvino.md"), "# 看不见的城市\n\n一段。\n").expect("a book");
    std::fs::write(dir.join("shelf/notes.rtf"), "x").expect("a file readio cannot open");

    type_text(&mut app, &format!("/import {}/", dir.display()));
    let view = draw(&mut app, &mut terminal);
    assert!(view.contains("shelf/"), "the directory is offered:\n{view}");

    // ⏎ on a directory is a step, not an answer: it walks in.
    app.on_key(key(KeyCode::Enter));
    let view = draw(&mut app, &mut terminal);
    assert!(
        view.contains("calvino.md"),
        "walking in lists what is inside:\n{view}"
    );
    assert!(
        !view.contains("notes.rtf"),
        "and only what readio can open:\n{view}"
    );

    // ⏎ on the file imports it. A markdown book is named after its file; the
    // heading inside becomes the first chapter.
    app.on_key(key(KeyCode::Enter));
    let view = settle(&mut app, &mut terminal);
    assert_eq!(
        app.book.as_ref().map(|b| b.title.as_str()),
        Some("calvino"),
        "and the file opens:\n{view}"
    );
    // Opening a book says what was loaded and stops there — the chapter list is
    // a select, not a transcript block — so the words themselves arrive with the
    // first passage.
    app.on_key(key(KeyCode::Enter));
    let view = settle(&mut app, &mut terminal);
    assert!(
        view.contains("看不见的城市"),
        "with its own contents:\n{view}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// Voice is a workspace rather than another select: model acquisition and
/// scoped configuration are visible together, but remain independent.
#[test]
fn the_voice_workspace_separates_models_from_configuration() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    let view = command(&mut app, &mut terminal, "/voice");
    assert!(
        view.contains("模型库") && view.contains("Voice 配置"),
        "both independent dimensions share one workspace:\n{view}"
    );
    assert!(
        view.contains("kokoro") && view.contains("openai"),
        "the model library shows every preset:\n{view}"
    );
    assert!(
        !view.contains("auto"),
        "sentence-language routing is not offered:\n{view}"
    );
}

/// Nothing in the list installs a server: the one engine whose answer is fixed
/// on every machine is the one readio never installs, and it says so.
#[test]
fn a_server_is_never_offered_for_installing() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    command(&mut app, &mut terminal, "/voice");
    for _ in 0..3 {
        app.on_key(key(KeyCode::Down));
    }
    let view = draw(&mut app, &mut terminal);
    assert!(
        view.contains("外部服务"),
        "a server is not a package:\n{view}"
    );

    // ⏎ on it neither downloads nor configures anything.
    app.on_key(key(KeyCode::Enter));
    let view = settle(&mut app, &mut terminal);
    assert!(
        !view.contains("确认下载")
            && !view.contains("uv tool install")
            && !view.contains("pipx install"),
        "nothing was installed:\n{view}"
    );
}

/// `/voice install openai` reaches the same answer the select gives, and says
/// where to read about the thing readio cannot do for you.
///
/// It is spelled out rather than picked because the menu would otherwise answer
/// first: with the select open on openai, ⏎ runs the row, and the row switches.
#[test]
fn installing_a_server_explains_itself_instead_of_running_a_command() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    let view = command(&mut app, &mut terminal, "/voice install openai");
    assert!(
        view.contains("自己起的服务"),
        "it says what openai is:\n{view}"
    );
    assert!(
        view.contains("github.com"),
        "and where to read about it:\n{view}"
    );
}

/// An engine nobody has heard of is a typo, not a job to start.
#[test]
fn installing_something_that_is_not_an_engine_is_refused() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    let view = command(&mut app, &mut terminal, "/voice install nosuchengine");
    assert!(
        view.contains("没有这个引擎"),
        "and it lists the ones there are:\n{view}"
    );
}

/// `/tts on` was a switch once, and muscle memory outlives a rewrite. So is
/// `/tts` itself: it is the old name for `/voice`, and both have to land.
///
/// The reply has to teach the gesture that replaced the switch rather than
/// report a missing voice called "on", which is true and useless.
#[test]
fn asking_to_switch_speech_on_points_at_the_key_that_does_it() {
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    for word in ["/voice on", "/voice off", "/tts on", "/tts off"] {
        let view = command(&mut app, &mut terminal, word);
        assert!(view.contains("^s"), "{word} should name the key:\n{view}");
        assert!(
            !view.contains("没有这个引擎"),
            "{word} is not a misspelled engine:\n{view}"
        );
    }
}
