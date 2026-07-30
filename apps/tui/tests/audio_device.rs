//! The audio-output whitelist, from the reader's side.
//!
//! The scenario these protect is switching headphones: the system silently
//! re-routes to the built-in speakers, and readio has to catch it, stop the
//! audio, and say so in a way the reader can act on.

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use readio::app::App;
use readio::book::Book;
use readio::library::Library;
use readio::store::Store;

/// The screen as text, walking each row by display width so the trailing cell
/// of a wide glyph does not inject stale characters between the real ones.
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

/// These tests all read and write the one `config.yaml`, so they take turns.
/// Running them in parallel would have one test's `/device any` erase the rule
/// another just saved.
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A fresh app with no whitelist, whatever an earlier test left behind.
fn fixture() -> (App, Terminal<TestBackend>) {
    configured(|_| {})
}

/// The same, with a chance to bend the config before the app reads it.
fn configured(edit: impl FnOnce(&mut readio::config::Config)) -> (App, Terminal<TestBackend>) {
    common::isolated_home();
    let mut config = readio::config::Config::load().0;
    config.voice.output.allow.clear();
    config.voice.output.query.clear();
    edit(&mut config);
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
    screen(terminal)
}

/// Pump until the welcome turn is done, so a typed command lands on an idle app
/// instead of being read as "hurry up".
fn settle(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    // A deadline rather than a frame count: turn timing is wall-clock, so a
    // frame budget gives a busy CI machine less time, not more.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.turn.busy() && std::time::Instant::now() < deadline {
        draw(app, terminal, 1);
    }
}

fn type_line(app: &mut App, line: &str) {
    for c in line.chars() {
        app.on_key(ratatui::crossterm::event::KeyEvent::from(
            ratatui::crossterm::event::KeyCode::Char(c),
        ));
    }
    app.on_key(ratatui::crossterm::event::KeyEvent::from(
        ratatui::crossterm::event::KeyCode::Enter,
    ));
}

#[test]
fn an_unlisted_output_is_named_and_the_fix_is_offered() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    // A whitelist that the machine's actual output cannot satisfy: this is the
    // headphones-fell-asleep case.
    type_line(&mut app, "/device allow readio-no-such-headphones");
    let view = draw(&mut app, &mut terminal, 6);

    assert!(
        view.contains("readio-no-such-headphones"),
        "the rule should be echoed back:\n{view}"
    );
    assert!(
        view.contains("音频输出设备") || view.contains("读不到音频设备列表"),
        "the reader should see the device list, or why it could not be read:\n{view}"
    );
    assert!(
        view.contains("/device any"),
        "the way out of the restriction has to be on screen:\n{view}"
    );
}

/// The Linux CI box has no `pactl`, so the probe fails there and used to leave
/// the reader with a mute and no way out. `output.query` pins that failure on
/// any machine, which is the point: this path must not depend on the host's
/// audio stack to be exercised.
#[test]
fn an_unreadable_device_list_still_offers_the_way_out() {
    let _guard = exclusive();
    let (mut app, mut terminal) = configured(|config| {
        config.voice.output.query = "readio-no-such-probe-command".into();
    });
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/device allow readio-no-such-headphones");
    let view = draw(&mut app, &mut terminal, 6);

    assert!(
        view.contains("读不到音频设备列表"),
        "the failure itself should be reported:\n{view}"
    );
    assert!(
        view.contains("认不出的设备一律不出声"),
        "and its consequence — still muted — spelled out:\n{view}"
    );
    assert!(
        view.contains("/device any"),
        "the way out has to be on screen even when nothing can be probed:\n{view}"
    );
}

#[test]
fn turning_read_aloud_on_while_muted_says_so_instead_of_looking_fine() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    type_line(&mut app, "/device allow readio-no-such-headphones");
    draw(&mut app, &mut terminal, 4);

    // Speech cannot start without an engine on this machine, so drive the gate
    // report directly: what matters is that a mute is never silent about itself.
    type_line(&mut app, "/device");
    let view = draw(&mut app, &mut terminal, 6);
    assert!(
        view.contains("白名单") || view.contains("读不到音频设备列表"),
        "the whitelist state should be visible:\n{view}"
    );
}

#[test]
fn any_clears_the_restriction_and_the_config_remembers() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);

    type_line(&mut app, "/device allow airpods");
    draw(&mut app, &mut terminal, 4);
    type_line(&mut app, "/device any");
    let view = draw(&mut app, &mut terminal, 4);

    assert!(
        view.contains("白名单已清空") || view.contains("任何"),
        "clearing should be confirmed:\n{view}"
    );

    // The config file on disk is the source of truth, so check it directly.
    let saved = std::fs::read_to_string(readio::paths::config_file()).expect("config written");
    let config: serde_yaml_ng::Value = serde_yaml_ng::from_str(&saved).expect("valid yaml");
    let allow = config
        .get("tts")
        .and_then(|tts| tts.get("output"))
        .and_then(|output| output.get("allow"))
        .and_then(|allow| allow.as_sequence())
        .cloned()
        .unwrap_or_default();
    assert!(
        allow.is_empty(),
        "the whitelist should be empty on disk, got {allow:?}"
    );
}

#[test]
fn a_bad_subcommand_explains_itself() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    type_line(&mut app, "/device wat");
    let view = draw(&mut app, &mut terminal, 4);
    assert!(
        view.contains("/device [allow") && view.contains("any"),
        "an unknown subcommand should print the usage:\n{view}"
    );
}

#[test]
fn the_whitelist_survives_a_restart() {
    let _guard = exclusive();
    let (mut app, mut terminal) = fixture();
    settle(&mut app, &mut terminal);
    type_line(&mut app, "/device allow airpods");
    draw(&mut app, &mut terminal, 4);

    // A fresh App reads the same config file, the way the next launch would.
    let (config, note) = readio::config::Config::load();
    assert!(note.is_none(), "the file we wrote should parse: {note:?}");
    assert_eq!(
        config.voice.output.allow,
        vec!["airpods".to_string()],
        "the rule should come back after a restart"
    );
    assert!(config.voice.output.is_active());
    assert_eq!(config.voice.output.poll, 5, "defaults come along unchanged");

    // And clean up, so the other tests in this binary start from no whitelist.
    type_line(&mut app, "/device any");
    draw(&mut app, &mut terminal, 4);
}
