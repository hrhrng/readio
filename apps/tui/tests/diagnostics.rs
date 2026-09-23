mod common;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use readio::app::App;
use readio::diagnostics::DiagnosticLog;
use readio::library::Library;
use readio::store::Store;
use readio_sdk::diagnostics::{DiagnosticCategory, DiagnosticEventInput};
use serde_json::json;

fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn tui_support_bundle_keeps_recent_correlated_facts_without_private_content() {
    let mut log = DiagnosticLog::with_capacity(3);
    for index in 0..4_u64 {
        log.record(DiagnosticEventInput {
            timestamp_unix_ms: 1_000 + index,
            operation_id: format!("op-{index}"),
            component: "tui-player".to_string(),
            category: DiagnosticCategory::Playback,
            name: "clock_sample".to_string(),
            fields: BTreeMap::from([
                ("sentence_index".to_string(), json!(index)),
                ("media_position_ms".to_string(), json!(index * 250)),
                ("book_text".to_string(), json!("A private sentence")),
                ("authorization".to_string(), json!("Bearer private-token")),
            ]),
        });
    }

    let bundle = log.build_support_bundle(2_000);
    let encoded = serde_json::to_string(&bundle).expect("support bundle JSON");
    assert_eq!(bundle.platform, "tui");
    assert_eq!(bundle.events.len(), 3);
    assert_eq!(
        bundle
            .events
            .iter()
            .map(|event| event.operation_id.as_str())
            .collect::<Vec<_>>(),
        ["op-1", "op-2", "op-3"]
    );
    assert_eq!(bundle.events[2].fields["media_position_ms"], json!(750));
    assert!(!bundle.redaction.content_included);
    assert!(!bundle.redaction.secrets_included);
    assert_eq!(bundle.redaction.dropped_field_count, 6);
    assert!(!encoded.contains("private sentence"));
    assert!(!encoded.contains("private-token"));
    assert!(!encoded.contains("authorization"));
    assert!(!encoded.contains("book_text"));
}

#[test]
fn tui_support_bundle_file_is_json_and_never_overwrites_an_existing_export() {
    let directory =
        std::env::temp_dir().join(format!("readio-tui-diagnostics-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("diagnostics fixture directory");
    let path = directory.join("readio-support.json");
    let log = DiagnosticLog::with_capacity(3);

    let bundle = log
        .write_support_bundle(&path, 2_000)
        .expect("first support export");
    let encoded = std::fs::read_to_string(&path).expect("written support bundle");
    assert_eq!(bundle.generated_at_unix_ms, 2_000);
    assert!(encoded.ends_with('\n'));
    assert!(encoded.contains("\"content_included\": false"));

    let second = log.write_support_bundle(&path, 3_000);
    assert!(
        second.is_err(),
        "an earlier support export must not be overwritten"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("original support bundle remains"),
        encoded
    );

    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn diagnostics_command_exports_the_live_session_through_the_safe_bundle_path() {
    let _guard = exclusive();
    let home = common::isolated_home();
    let support = home.join("support");
    let _ = std::fs::remove_dir_all(&support);
    let mut app = App::new(Library::ephemeral(), Store::ephemeral(), None, None);
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.turn.busy() && Instant::now() < deadline {
        app.on_tick();
        std::thread::sleep(Duration::from_millis(4));
    }
    assert!(!app.turn.busy(), "the library welcome turn must settle");

    for character in "/diagnostics".chars() {
        app.on_key(KeyEvent::from(KeyCode::Char(character)));
    }
    app.on_key(KeyEvent::from(KeyCode::Enter));

    let files = std::fs::read_dir(&support)
        .expect("the diagnostics command creates a support directory")
        .map(|entry| entry.expect("support entry").path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    assert!(
        files[0]
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("readio-support-") && name.ends_with(".json"))
    );
    let bundle = std::fs::read_to_string(&files[0]).expect("support bundle contents");
    assert!(bundle.contains("\"platform\": \"tui\""));
    assert!(bundle.contains("\"content_included\": false"));
    assert!(!bundle.contains("authorization"));
    assert!(!bundle.contains("book_text"));
}

#[test]
fn tui_import_and_parser_events_share_an_operation_without_leaking_the_book() {
    let _guard = exclusive();
    let home = common::isolated_home();
    let support = home.join("support");
    let _ = std::fs::remove_dir_all(&support);
    let source = common::source_book("private-diagnostics-book", "A Private Diagnostics Title");
    let mut app = App::new(Library::ephemeral(), Store::ephemeral(), None, None);
    settle(&mut app);

    type_line(&mut app, &format!("/import {} --copy", source.display()));
    type_line(&mut app, "/diagnostics");

    let path = std::fs::read_dir(&support)
        .expect("support directory")
        .next()
        .expect("one support file")
        .expect("support entry")
        .path();
    let encoded = std::fs::read_to_string(path).expect("support bundle");
    let bundle: serde_json::Value = serde_json::from_str(&encoded).expect("support JSON");
    let events = bundle["events"].as_array().expect("diagnostic events");
    assert_eq!(
        events.len(),
        2,
        "expected import and parsing facts: {encoded}"
    );
    assert_eq!(events[0]["category"], "import");
    assert_eq!(events[1]["category"], "parsing");
    assert_eq!(events[0]["operation_id"], events[1]["operation_id"]);
    assert_eq!(events[0]["fields"]["source_kind"], "file");
    assert_eq!(events[1]["fields"]["document_format"], "markdown");
    assert!(events[1]["fields"]["block_count"].as_u64().unwrap_or(0) > 0);
    assert!(!encoded.contains("A Private Diagnostics Title"));
    assert!(!encoded.contains("第一段内容"));
    assert!(!encoded.contains("private-diagnostics-book"));
    assert!(!encoded.contains(source.to_string_lossy().as_ref()));
}

fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.turn.busy() && Instant::now() < deadline {
        app.on_tick();
        std::thread::sleep(Duration::from_millis(4));
    }
    assert!(!app.turn.busy(), "the current turn must settle");
}

fn type_line(app: &mut App, line: &str) {
    for character in line.chars() {
        app.on_key(KeyEvent::from(KeyCode::Char(character)));
    }
    app.on_key(KeyEvent::from(KeyCode::Enter));
}
