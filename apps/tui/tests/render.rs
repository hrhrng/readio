//! Frame-level tests: drive the real `App` against ratatui's test backend and
//! assert on what a reader would actually see.

mod common;

use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use readio::app::App;
use readio::book::Book;
use readio::library::Library;
use readio::store::Store;

/// Render the app after `frames` ticks, returning the screen as plain text.
///
/// A double-width glyph occupies two cells; ratatui leaves the second one
/// empty and skips it when diffing, so the test backend keeps whatever was
/// there before. Walk the row by display width to reconstruct what a real
/// terminal would show.
fn screen(app: &mut App, terminal: &mut Terminal<TestBackend>, frames: usize) -> String {
    for _ in 0..frames {
        app.on_tick();
        terminal.draw(|frame| app.draw(frame)).expect("draw");
        // The turn machine is driven by wall-clock time, so give it some.
        std::thread::sleep(Duration::from_millis(6));
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

/// Pump frames until the turn queue drains, then render. Turn timing is
/// wall-clock based (tool calls have real durations), so tests wait on the
/// state machine instead of guessing frame counts.
fn run_until_idle(app: &mut App, terminal: &mut Terminal<TestBackend>) -> String {
    for _ in 0..600 {
        if !app.turn.busy() {
            break;
        }
        screen(app, terminal, 1);
    }
    screen(app, terminal, 1)
}

fn fixture() -> (App, Terminal<TestBackend>) {
    fixture_sized(90, 30)
}

fn fixture_sized(width: u16, height: u16) -> (App, Terminal<TestBackend>) {
    common::isolated_home();
    let book = Book::load(None).expect("sample book");
    let app = App::new(
        Library::ephemeral(),
        Store::ephemeral(),
        Some(book),
        Some("已复制进书库：《注意力的形状》".to_string()),
    );
    let terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    (app, terminal)
}

/// An app that starts with no book open, the way a bare `readio` launch does.
fn library_fixture(entries: usize) -> (App, Terminal<TestBackend>) {
    common::isolated_home();
    let mut library = Library::ephemeral();
    for i in 0..entries {
        library.entries.push(readio::library::Entry {
            id: format!("id-{i}"),
            title: format!("第 {} 本书", i + 1),
            author: Some("某人".to_string()),
            path: std::path::PathBuf::from(format!("/tmp/readio-missing-{i}.md")),
            origin: None,
            mode: readio::library::Mode::Copy,
            bytes: 1234,
            chars: 5000 * (i + 1),
            chapters: 3,
            imported: 1,
            last_opened: 0,
        });
    }
    let app = App::new(library, Store::ephemeral(), None, None);
    let terminal = Terminal::new(TestBackend::new(96, 24)).expect("terminal");
    (app, terminal)
}

#[test]
fn opening_frame_shows_chrome_and_welcome() {
    let (mut app, mut terminal) = fixture();
    let view = screen(&mut app, &mut terminal, 12);

    assert!(view.contains("readio"), "header brand missing:\n{view}");
    assert!(view.contains("注意力的形状"), "book title missing:\n{view}");
    assert!(
        view.contains("第 1/4 章"),
        "chapter counter missing:\n{view}"
    );
    assert!(
        view.contains("ctx 0.0%"),
        "progress should read as a context window:\n{view}"
    );
    assert!(
        view.contains("tok") && view.contains("0:00"),
        "the status line should show tokens and the session clock:\n{view}"
    );
    assert!(
        view.contains("已载入") || view.contains("阅读清单"),
        "welcome content missing:\n{view}"
    );
}

#[test]
fn enter_starts_a_reading_turn_with_a_tool_call() {
    // Tall enough that a whole turn — thought, tool call, passage — fits.
    let (mut app, mut terminal) = fixture_sized(90, 60);
    run_until_idle(&mut app, &mut terminal);
    app.turn.set_cps(3_000.0);

    press_enter(&mut app);
    let view = run_until_idle(&mut app, &mut terminal);

    assert!(
        view.contains("Read") && view.contains("readio://sample"),
        "expected a Read tool call against the book URI:\n{view}"
    );
    assert!(
        view.contains("终端") || view.contains("注意力"),
        "expected streamed book text:\n{view}"
    );
    // The sample's first chapter fits in one turn, so the position rolls into
    // the next chapter rather than merely moving a few paragraphs.
    assert!(
        (app.pos.chapter, app.pos.para) > (0, 0),
        "reading position should have advanced, still at {:?}",
        app.pos
    );
}

#[test]
fn a_question_runs_a_real_search() {
    let (mut app, mut terminal) = fixture();
    run_until_idle(&mut app, &mut terminal);
    app.turn.set_cps(3_000.0);

    type_line(&mut app, "进度条");
    let view = run_until_idle(&mut app, &mut terminal);

    assert!(view.contains("Grep"), "expected a Grep tool call:\n{view}");
    assert!(
        view.contains("hits"),
        "expected a hit count in the tool header:\n{view}"
    );
    assert!(
        view.contains("进度条"),
        "expected the matched text to be quoted back:\n{view}"
    );
}

#[test]
fn escape_interrupts_a_running_turn() {
    let (mut app, mut terminal) = fixture();
    screen(&mut app, &mut terminal, 20);
    app.turn.set_cps(20.0);

    press_enter(&mut app);
    screen(&mut app, &mut terminal, 8);
    assert!(app.turn.busy(), "turn should be running");

    press(&mut app, crossterm::event::KeyCode::Esc);
    let view = screen(&mut app, &mut terminal, 4);

    assert!(!app.turn.busy(), "turn should be stopped");
    assert!(view.contains("已中断"), "expected interrupt event:\n{view}");
}

#[test]
fn help_overlay_opens_and_closes() {
    let (mut app, mut terminal) = fixture();
    screen(&mut app, &mut terminal, 12);

    type_line(&mut app, "/help");
    let view = screen(&mut app, &mut terminal, 3);
    assert!(view.contains("快捷键与命令"), "help missing:\n{view}");

    press(&mut app, crossterm::event::KeyCode::Esc);
    let view = screen(&mut app, &mut terminal, 3);
    assert!(!view.contains("快捷键与命令"), "help should close:\n{view}");
}

#[test]
fn goto_moves_the_chapter_pointer() {
    let (mut app, mut terminal) = fixture();
    run_until_idle(&mut app, &mut terminal);
    // Slow enough that the jumped-to chapter is still being read.
    app.turn.set_cps(24.0);

    type_line(&mut app, "/goto 3");
    let view = screen(&mut app, &mut terminal, 25);

    assert_eq!(app.pos.chapter, 2, "should sit in chapter 3");
    assert!(view.contains("第 3/4 章"), "header should follow:\n{view}");
    assert!(
        view.contains("跳到第 3 章"),
        "the jump should be announced:\n{view}"
    );
}

#[test]
fn unknown_command_is_reported_not_swallowed() {
    let (mut app, mut terminal) = fixture();
    screen(&mut app, &mut terminal, 12);

    type_line(&mut app, "/nope");
    let view = screen(&mut app, &mut terminal, 6);
    assert!(view.contains("未知命令"), "expected an error note:\n{view}");
}

#[test]
fn scrolling_away_from_the_tail_shows_a_hint() {
    // A short viewport guarantees the content overflows.
    let (mut app, mut terminal) = fixture_sized(90, 16);
    app.turn.set_cps(4_000.0);
    run_until_idle(&mut app, &mut terminal);
    press_enter(&mut app);
    run_until_idle(&mut app, &mut terminal);

    for _ in 0..12 {
        app.on_mouse(mouse_scroll_up());
    }
    let view = screen(&mut app, &mut terminal, 2);
    assert!(
        view.contains("已上滚") || view.contains("回到底部"),
        "expected a scroll indicator:\n{view}"
    );

    // End returns to the tail.
    press(&mut app, crossterm::event::KeyCode::End);
    let view = screen(&mut app, &mut terminal, 2);
    assert!(
        !view.contains("已上滚"),
        "End should return to the tail:\n{view}"
    );
}

#[test]
fn import_switches_books_and_reports_failure() {
    let (mut app, mut terminal) = fixture();
    run_until_idle(&mut app, &mut terminal);

    let path = common::source_book("render-import", "新书");
    type_line(&mut app, &format!("/import {}", path.display()));
    let view = run_until_idle(&mut app, &mut terminal);

    let book = app.book.as_ref().expect("a book should be open");
    assert_eq!(book.title, "新书", "book should switch:\n{view}");
    assert_eq!(book.chapters.len(), 2);
    assert_eq!(
        app.pos,
        readio::app::flow::Pos::default(),
        "a fresh book starts at the top"
    );
    assert!(
        view.contains("已复制进书库"),
        "the import should say where the file went:\n{view}"
    );
    assert_eq!(app.library.len(), 1, "the book should be in the library");

    // A bad path must surface an error instead of failing silently.
    type_line(&mut app, "/import /nope/missing.epub");
    let view = run_until_idle(&mut app, &mut terminal);
    assert!(
        view.contains("导入失败"),
        "expected a failure block:\n{view}"
    );
    assert_eq!(
        app.book.as_ref().map(|b| b.title.clone()),
        Some("新书".to_string()),
        "the old book should stay open"
    );
}

#[test]
fn import_modes_are_accepted_from_the_prompt() {
    let (mut app, mut terminal) = fixture();
    run_until_idle(&mut app, &mut terminal);

    let linked = common::source_book("render-link", "引用的书");
    type_line(&mut app, &format!("/import {} -l", linked.display()));
    let view = run_until_idle(&mut app, &mut terminal);

    assert!(
        view.contains("已登记引用"),
        "expected link wording:\n{view}"
    );
    let entry = app.library.get(1).expect("an entry");
    assert_eq!(entry.mode, readio::library::Mode::Link);
    assert_eq!(entry.path, linked, "link keeps the original path");
    assert!(linked.exists(), "link mode must not move the file");
}

// ── the library listing ──────────────────────────────────────────────────────

#[test]
fn bare_launch_lists_imported_books() {
    let (mut app, mut terminal) = library_fixture(3);
    let view = screen(&mut app, &mut terminal, 4);

    assert!(
        view.contains("书库"),
        "expected the library header:\n{view}"
    );
    assert!(
        view.contains("书库 3 本"),
        "header should count books:\n{view}"
    );
    for n in 1..=3 {
        assert!(
            view.contains(&format!("第 {n} 本书")),
            "entry {n} missing:\n{view}"
        );
    }
    assert!(view.contains(" 1. "), "entries should be numbered:\n{view}");
    assert!(view.contains("copy"), "hold mode should be shown:\n{view}");
    assert!(app.book.is_none(), "no book is open yet");
}

#[test]
fn an_empty_library_explains_how_to_import() {
    let (mut app, mut terminal) = library_fixture(0);
    let view = screen(&mut app, &mut terminal, 4);

    assert!(
        view.contains("书库  0 本") || view.contains("还没有导入"),
        "{view}"
    );
    assert!(
        view.contains("-c") && view.contains("-l") && view.contains("-m"),
        "the three modes should be spelled out:\n{view}"
    );
    assert!(
        view.contains("/sample"),
        "offer the built-in sample:\n{view}"
    );
}

#[test]
fn a_bare_number_opens_that_book() {
    let (mut app, mut terminal) = library_fixture(0);
    let path = common::source_book("render-pick", "被选中的书");
    app.library
        .import(&path, readio::library::Mode::Copy)
        .expect("import");
    app.on_key(key(crossterm::event::KeyCode::Char('1')));
    press_enter(&mut app);
    let view = run_until_idle(&mut app, &mut terminal);

    assert_eq!(
        app.book.as_ref().map(|b| b.title.clone()),
        Some("被选中的书".to_string()),
        "typing 1 should open the first entry:\n{view}"
    );
}

#[test]
fn opening_a_missing_file_reports_it() {
    // library_fixture's entries point at paths that do not exist.
    let (mut app, mut terminal) = library_fixture(2);
    type_line(&mut app, "/open 2");
    let view = run_until_idle(&mut app, &mut terminal);

    assert!(view.contains("读不了"), "expected a failure block:\n{view}");
    assert!(app.book.is_none(), "nothing should have opened");
}

#[test]
fn out_of_range_selection_is_explained() {
    let (mut app, mut terminal) = library_fixture(2);
    type_line(&mut app, "/open 9");
    let view = run_until_idle(&mut app, &mut terminal);
    assert!(
        view.contains("没有第 9 本"),
        "expected a bounds note:\n{view}"
    );
}

#[test]
fn reading_commands_need_a_book_first() {
    let (mut app, mut terminal) = library_fixture(1);
    for command in ["/toc", "/find 注意力", "/goto 2", "/auto", "/progress"] {
        type_line(&mut app, command);
        let view = run_until_idle(&mut app, &mut terminal);
        assert!(
            view.contains("还没有打开的书"),
            "{command} should ask for a book first:\n{view}"
        );
    }
}

#[test]
fn sample_opens_without_any_import() {
    let (mut app, mut terminal) = library_fixture(0);
    type_line(&mut app, "/sample");
    let view = run_until_idle(&mut app, &mut terminal);

    assert_eq!(
        app.book.as_ref().map(|b| b.title.clone()),
        Some("注意力的形状".to_string()),
        "{view}"
    );
    assert!(view.contains("注意力的形状"), "{view}");
}

#[test]
fn quitting_works_even_with_the_help_panel_open() {
    let (mut app, mut terminal) = fixture();
    run_until_idle(&mut app, &mut terminal);

    type_line(&mut app, "/help");
    screen(&mut app, &mut terminal, 2);

    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('d'),
        crossterm::event::KeyModifiers::CONTROL,
    ));
    assert!(app.quit, "ctrl+d must quit even from an overlay");
}

// ── input helpers ────────────────────────────────────────────────────────────

fn key(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
}

fn press(app: &mut App, code: crossterm::event::KeyCode) {
    app.on_key(key(code));
}

fn press_enter(app: &mut App) {
    press(app, crossterm::event::KeyCode::Enter);
}

fn type_line(app: &mut App, line: &str) {
    for c in line.chars() {
        press(app, crossterm::event::KeyCode::Char(c));
    }
    press_enter(app);
}

fn mouse_scroll_up() -> crossterm::event::MouseEvent {
    crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::ScrollUp,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }
}

#[test]
fn read_aloud_highlights_the_sentence_lightly_and_the_word_deeply() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use readio::theme::theme;
    use readio::ui::block::{Block, Speaking};
    use readio::ui::scrollback::Scrollback;

    common::isolated_home();
    let passage = "界面不是中立的。它替你决定了什么值得注意。";
    let sentence = (0, "界面不是中立的。".len());
    let word_start = "界面".len();
    let word = (word_start, word_start + "不".len());

    let mut sb = Scrollback::new();
    let id = sb.push(Block::passage_text(passage));
    assert!(
        sb.set_speaking(
            id,
            Some(Speaking {
                sentence,
                word: Some(word)
            })
        ),
        "pointing the highlight at a passage should change it"
    );

    let area = Rect::new(0, 0, 60, 12);
    let mut buf = Buffer::empty(area);
    sb.render(area, &mut buf, 1);

    let th = theme();
    let mut counts = std::collections::HashMap::new();
    for cell in buf.content() {
        *counts.entry(cell.bg).or_insert(0usize) += 1;
    }
    let deep = counts.get(&th.bg_speaking_word).copied().unwrap_or(0);
    let light = counts.get(&th.bg_speaking).copied().unwrap_or(0);
    // A wide glyph owns two columns but ratatui styles only the leading cell —
    // the trailing one is reset and the terminal paints it from the glyph.
    assert_eq!(deep, 1, "「不」 and nothing else should be deeply washed");
    assert_eq!(
        light,
        "界面是中立的。".chars().count(),
        "the rest of the sentence should be lightly washed, got {light}"
    );

    // The second sentence stays plain: it has not been spoken yet. Walking by
    // display width skips the reset trailing cell of each wide glyph.
    let plain_text: String = (0..area.height)
        .flat_map(|y| {
            let mut row = String::new();
            let mut x = 0u16;
            while x < area.width {
                let symbol = buf[(x, y)].symbol();
                row.push_str(symbol);
                x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            row.chars().collect::<Vec<_>>()
        })
        .collect();
    assert!(
        plain_text.contains("值得注意"),
        "the unspoken remainder should still be on screen:\n{plain_text}"
    );

    // Dropping the word keeps the sentence lit — what happens between clips.
    assert!(sb.set_speaking(id, Some(Speaking::new(sentence))));
    let mut buf = Buffer::empty(area);
    sb.render(area, &mut buf, 2);
    let deep = buf
        .content()
        .iter()
        .filter(|cell| cell.bg == th.bg_speaking_word)
        .count();
    assert_eq!(deep, 0, "with no word, nothing should be deeply washed");
    let light = buf
        .content()
        .iter()
        .filter(|cell| cell.bg == th.bg_speaking)
        .count();
    assert_eq!(
        light,
        "界面不是中立的。".chars().count(),
        "the whole sentence should be lightly washed"
    );

    // And clearing it puts the page back to normal.
    sb.clear_speaking();
    let mut buf = Buffer::empty(area);
    sb.render(area, &mut buf, 3);
    assert!(
        buf.content()
            .iter()
            .all(|cell| cell.bg != th.bg_speaking && cell.bg != th.bg_speaking_word),
        "clearing the highlight should leave no wash behind"
    );
}

#[test]
fn the_word_highlight_survives_a_line_wrap() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use readio::theme::theme;
    use readio::ui::block::{Block, Speaking};
    use readio::ui::scrollback::Scrollback;

    common::isolated_home();
    // Long enough to wrap several times in a narrow column.
    let passage = "一个终端单元格大约是一比二的比例，所以用上半块字符可以把纵向分辨率翻一倍：前景色画上面那个像素，背景色画下面那个。";
    let mut sb = Scrollback::new();
    let id = sb.push(Block::passage_text(passage));

    let area = Rect::new(0, 0, 34, 16);
    let th = theme();

    // Walk a word cursor across the whole passage; at every step exactly one
    // unit is deeply washed, no matter where the wrap falls.
    let units = readio::tts::sentence::units(passage);
    assert!(
        units.len() > 20,
        "the fixture should be long: {}",
        units.len()
    );
    for unit in units {
        sb.set_speaking(
            id,
            Some(Speaking {
                sentence: (0, passage.len()),
                word: Some(unit.range),
            }),
        );
        let mut buf = Buffer::empty(area);
        sb.render(area, &mut buf, 4);
        let deep = buf
            .content()
            .iter()
            .filter(|cell| cell.bg == th.bg_speaking_word)
            .count();
        let text = &passage[unit.range.0..unit.range.1];
        assert_eq!(
            deep,
            text.chars().count(),
            "expected exactly 「{text}」 to be deeply washed"
        );
    }
}
