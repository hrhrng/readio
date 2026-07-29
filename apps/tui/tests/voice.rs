//! Who reads which book.
//!
//! Read-aloud cannot actually be entered here — there is no speech engine on a
//! test machine, and readio ships none — so these tests cover the part that
//! works without one: the choice itself, where it is written down, and which
//! books it applies to.

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
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
    config.reading.mode = readio::mode::Mode::Manual;
    config.reading.speed = 400.0;
    config.voice.engine = "kokoro".to_string();
    config.voice.name.clear();
    config.voice.language = "auto".to_string();
    config.voice.books.clear();
    // Two engines backed by a program every Unix test machine has. The Voice
    // workspace must distinguish "downloaded and selectable" from the real
    // presets, whose Python packages intentionally are not part of the test
    // environment.
    for (name, voice) in [("local-a", "reader-a"), ("local-b", "reader-b")] {
        config.voice.engines.insert(
            name.to_string(),
            readio::voice::config::EngineSpec {
                synth: "sh -c true".to_string(),
                voice: voice.to_string(),
                about: format!("{name} test model"),
                languages: std::collections::BTreeMap::from([(
                    "zh".to_string(),
                    readio::voice::config::LanguageSpec {
                        voice: voice.to_string(),
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            },
        );
    }
    config.voice.engine = "local-a".to_string();
    config.voice.name = "reader-a".to_string();
    config.voice.language = "zh".to_string();
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

fn type_line(app: &mut App, line: &str) {
    for c in line.chars() {
        app.on_key(KeyEvent::from(KeyCode::Char(c)));
    }
    app.on_key(KeyEvent::from(KeyCode::Enter));
}

fn saved() -> readio::config::Config {
    readio::config::Config::load().0
}

fn book_id(app: &App) -> String {
    app.book.as_ref().expect("a book is open").id.clone()
}

/// A shelf is not read in one voice: a Chinese novel and an English manual want
/// different voices as often as they want different engines. A choice made while
/// a book is open belongs to that book.
#[test]
fn a_voice_chosen_while_reading_belongs_to_that_book() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/voice af_heart");
    draw(&mut app, &mut terminal, 3);

    let cfg = saved();
    let id = book_id(&app);
    assert_eq!(
        cfg.book_voice(&id).and_then(|entry| entry.name.clone()),
        Some("af_heart".to_string()),
        "the book should carry the choice: {:?}",
        cfg.voice.books
    );
    assert_eq!(
        cfg.voice.name, "reader-a",
        "and the default should be exactly as it was"
    );
    assert_eq!(cfg.voice_for(Some(&id)).name, "af_heart");
    assert_eq!(cfg.voice_for(Some("some-other-book")).name, "reader-a");
}

/// And it says so. A reader who sets a voice while reading and finds their next
/// book unchanged has been surprised by a rule nobody told them.
#[test]
fn the_scope_of_a_choice_is_said_out_loud() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/voice af_heart");
    let view = draw(&mut app, &mut terminal, 3);

    let title = app.book.as_ref().expect("a book").title.clone();
    assert!(
        view.contains(&title),
        "the line should name the book it changed:\n{view}"
    );
}

/// The two ways a choice moves between one book and all of them.
#[test]
fn a_book_can_hand_its_voice_to_every_book() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    let id = book_id(&app);

    type_line(&mut app, "/voice zf_xiaoyi");
    draw(&mut app, &mut terminal, 3);
    type_line(&mut app, "/voice everywhere");
    draw(&mut app, &mut terminal, 3);

    let cfg = saved();
    assert_eq!(cfg.voice.name, "zf_xiaoyi", "everyone reads with it now");
    assert!(
        cfg.book_voice(&id).is_none(),
        "and the book stops carrying a second copy of the same decision"
    );
}

#[test]
fn a_book_can_be_handed_back_to_the_default() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    let id = book_id(&app);

    type_line(&mut app, "/voice zf_xiaoyi");
    draw(&mut app, &mut terminal, 3);
    assert!(saved().book_voice(&id).is_some());

    type_line(&mut app, "/voice default");
    draw(&mut app, &mut terminal, 3);
    let cfg = saved();
    assert!(
        cfg.book_voice(&id).is_none(),
        "the book has no opinion any more: {:?}",
        cfg.voice.books
    );
    assert_eq!(
        cfg.voice_for(Some(&id)),
        cfg.voice_for(None),
        "so it gets whatever everyone gets"
    );
}

/// The legacy `auto` spelling clears both settings and returns to the model
/// preset. It no longer enables sentence-language detection.
#[test]
fn auto_releases_both_overrides_to_the_model_defaults() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    let id = book_id(&app);

    type_line(&mut app, "/voice zh");
    draw(&mut app, &mut terminal, 3);
    assert_eq!(saved().voice_for(Some(&id)).language, "zh");

    type_line(&mut app, "/voice auto");
    draw(&mut app, &mut terminal, 3);
    let chosen = saved().voice_for(Some(&id));
    assert_eq!(chosen.language, "");
    assert_eq!(chosen.name, "");
}

/// In the library there is no book to attach a choice to, so it is the default.
#[test]
fn a_choice_made_in_the_library_is_the_default() {
    let _guard = exclusive();
    common::isolated_home();
    let mut config = readio::config::Config::load().0;
    config.reading.mode = readio::mode::Mode::Manual;
    config.voice.books.clear();
    config.voice.name.clear();
    let _ = config.save();

    let mut app = App::new(Library::ephemeral(), Store::ephemeral(), None, None);
    let mut terminal = Terminal::new(TestBackend::new(96, 30)).expect("terminal");
    type_line(&mut app, "/voice af_heart");
    draw(&mut app, &mut terminal, 3);

    let cfg = saved();
    assert_eq!(cfg.voice.name, "af_heart");
    assert!(
        cfg.voice.books.is_empty(),
        "nothing to attach it to: {:?}",
        cfg.voice.books
    );
}

/// `/voice everywhere` in the library is a question with no subject, and saying
/// so beats silently doing nothing.
#[test]
fn promoting_a_voice_needs_a_book_to_promote_it_from() {
    let _guard = exclusive();
    common::isolated_home();
    let mut config = readio::config::Config::load().0;
    config.reading.mode = readio::mode::Mode::Manual;
    let _ = config.save();

    let mut app = App::new(Library::ephemeral(), Store::ephemeral(), None, None);
    let mut terminal = Terminal::new(TestBackend::new(96, 30)).expect("terminal");
    type_line(&mut app, "/voice everywhere");
    let view = draw(&mut app, &mut terminal, 3);
    assert!(
        view.contains("先打开一本书") || view.contains("open a book"),
        "expected an explanation:\n{view}"
    );
}

/// The old name still works. It is not a second command with a second menu any
/// more, which was the whole point — but a reader with `/tts` in their fingers
/// should not be told there is no such thing.
#[test]
fn the_old_name_reaches_the_same_command() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    let id = book_id(&app);

    type_line(&mut app, "/tts af_heart");
    draw(&mut app, &mut terminal, 3);
    assert_eq!(
        saved()
            .book_voice(&id)
            .and_then(|entry| entry.name.clone())
            .as_deref(),
        Some("af_heart")
    );
}

/// Downloaded models and scoped configuration are two independent dimensions,
/// but they belong in one workspace rather than in two commands.
#[test]
fn voice_opens_one_workspace_for_models_and_configuration() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/voice");
    let view = draw(&mut app, &mut terminal, 3);

    assert!(
        view.contains("模型库") || view.contains("Models"),
        "the left side manages local models:\n{view}"
    );
    assert!(
        view.contains("Voice 配置") || view.contains("Voice configuration"),
        "the right side configures how a downloaded model is used:\n{view}"
    );
    assert!(
        !view.contains("auto"),
        "the workspace must not offer per-sentence language routing:\n{view}"
    );
}

#[test]
fn voice_workspace_uses_tabs_when_the_terminal_is_narrow() {
    let _guard = exclusive();
    let (mut app, _) = fixture();
    settle(
        &mut app,
        &mut Terminal::new(TestBackend::new(80, 24)).expect("terminal"),
    );
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");

    type_line(&mut app, "/voice");
    let models = draw(&mut app, &mut terminal, 2);
    assert!(
        models.contains("[模型库]") || models.contains("[Models]"),
        "the active narrow pane should be presented as a tab:\n{models}"
    );

    app.on_key(KeyEvent::from(KeyCode::Tab));
    let config = draw(&mut app, &mut terminal, 2);
    assert!(
        config.contains("[Voice 配置]") || config.contains("[Voice configuration]"),
        "the configuration pane should replace the model pane:\n{config}"
    );
}

/// Installing or rediscovering a model changes only local availability. It
/// never silently turns that model into the global or per-book voice.
#[test]
fn model_management_never_changes_voice_configuration() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    let before = saved().voice_for(None);
    type_line(&mut app, "/voice install local-b");
    draw(&mut app, &mut terminal, 3);
    let after = saved().voice_for(None);

    assert_eq!(
        after, before,
        "a model becoming available is not a configuration choice"
    );
}

/// Scope is a field in the form, not a guess based on whether a book happens to
/// be open. Nothing is written until the Save row is activated.
#[test]
fn workspace_can_explicitly_save_a_different_model_for_one_book() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    let id = book_id(&app);

    type_line(&mut app, "/voice");
    app.on_key(KeyEvent::from(KeyCode::Tab)); // configuration pane
    app.on_key(KeyEvent::from(KeyCode::Enter)); // scope: global -> one book
    app.on_key(KeyEvent::from(KeyCode::Down)); // book
    app.on_key(KeyEvent::from(KeyCode::Down)); // model
    app.on_key(KeyEvent::from(KeyCode::Enter)); // local-a -> local-b

    assert!(
        saved().book_voice(&id).is_none(),
        "editing the draft must not persist before Save"
    );

    for _ in 0..4 {
        app.on_key(KeyEvent::from(KeyCode::Down));
    }
    app.on_key(KeyEvent::from(KeyCode::Enter)); // Save
    draw(&mut app, &mut terminal, 2);

    let config = saved();
    assert_eq!(
        config
            .book_voice(&id)
            .and_then(|entry| entry.engine.as_deref()),
        Some("local-b")
    );
    assert_ne!(
        config.voice.engine, "local-b",
        "saving a book override must leave the global model alone"
    );
}
