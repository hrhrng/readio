use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use readio::app::App;
use readio::cli::CloudCommand;
use readio::library::Library;
use readio::replica_sync::{LocalReplicaEntity, ReplicaSyncState};
use readio::store::Store;
use readio::{cloud, paths};
use readio_sdk::replica::ReplicaEntityKind;
use serde_json::{Value, json};

#[tokio::test]
async fn crdt_sync_persists_before_push_then_acknowledges_and_uploads_a_snapshot() {
    let home = temporary_home();
    std::fs::create_dir_all(&home).expect("create test home");
    paths::set_home(&home);

    let mut remote = ReplicaSyncState::default();
    remote
        .stage_local(&[LocalReplicaEntity::new(
            ReplicaEntityKind::ReadingPosition,
            "book-remote",
            json!({
                "title": "Remote book",
                "chapter": 2,
                "para": 3,
                "chars_read": 144,
                "sessions": 4,
                "updated": 5,
                "speech": null,
                "marks": []
            }),
        )])
        .expect("stage remote fixture");
    let remote_changes = serde_json::to_value(remote.outbox()).expect("remote changes");

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local cloud");
    let server = format!("http://{}", listener.local_addr().expect("server address"));
    std::fs::write(
        home.join("cloud.json"),
        serde_json::to_vec(&json!({
            "server": server,
            "access_token": "test-token",
            "device_id": "tui-test-device",
            "cursor": 0,
            "replica": {}
        }))
        .expect("cloud state"),
    )
    .expect("write cloud state");

    let server_home = home.clone();
    let server_thread = thread::spawn(move || serve_sync(listener, &server_home, remote_changes));

    cloud::run(CloudCommand::Sync)
        .await
        .expect("CRDT cloud sync succeeds");
    server_thread
        .join()
        .expect("local cloud thread")
        .expect("replica HTTP contract");

    let saved: Value =
        serde_json::from_slice(&std::fs::read(home.join("cloud.json")).expect("saved cloud state"))
            .expect("decode saved cloud state");
    assert_eq!(saved["cursor"], 2);
    assert_eq!(saved["replica"]["outbox"], json!([]));

    let store: Value = serde_json::from_slice(
        &std::fs::read(home.join("state.json")).expect("saved local projection"),
    )
    .expect("decode local projection");
    assert_eq!(store["books"]["book-remote"]["chars_read"], 144);

    let mut app = App::new(Library::load(), Store::load(), None, None);
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.turn.busy() && Instant::now() < deadline {
        app.on_tick();
        thread::sleep(Duration::from_millis(4));
    }
    assert!(!app.turn.busy(), "the new TUI session must settle");
    for character in "/diagnostics".chars() {
        app.on_key(KeyEvent::from(KeyCode::Char(character)));
    }
    app.on_key(KeyEvent::from(KeyCode::Enter));

    let support = std::fs::read_dir(home.join("support"))
        .expect("sync diagnostics support directory")
        .next()
        .expect("one support bundle")
        .expect("support entry")
        .path();
    let encoded = std::fs::read_to_string(support).expect("support bundle contents");
    let bundle: Value = serde_json::from_str(&encoded).expect("support bundle JSON");
    let events = bundle["events"]
        .as_array()
        .expect("diagnostic events")
        .iter()
        .filter(|event| event["category"] == "sync")
        .collect::<Vec<_>>();
    assert_eq!(
        events.len(),
        5,
        "pull, push, post-push pull, snapshot, and completion must be inspectable: {encoded}"
    );
    let operation_id = events[0]["operation_id"]
        .as_str()
        .expect("sync operation id");
    assert!(
        events
            .iter()
            .all(|event| event["operation_id"] == operation_id),
        "one live sync must stay correlated across every stage: {encoded}"
    );
    assert_eq!(
        events
            .iter()
            .map(|event| event["fields"]["direction"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["pull", "push", "pull", "snapshot", "sync"]
    );
    assert_eq!(
        events
            .iter()
            .map(|event| event["fields"]["phase"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["initial", "outbox", "post_push", "snapshot", "complete"]
    );
    assert_eq!(events[4]["fields"]["cursor"], 2);
    assert!(!encoded.contains("test-token"));
    assert!(!encoded.contains(&server));

    std::fs::remove_dir_all(&home).expect("remove isolated test home");
}

fn serve_sync(listener: TcpListener, home: &Path, remote_changes: Value) -> Result<(), String> {
    let first_pull = read_request(&listener)?;
    require_path(&first_pull, "/v1/replica/pull?cursor=0&limit=100")?;
    respond_json(
        first_pull.stream,
        200,
        &json!({ "cursor": 1, "hasMore": false, "changes": remote_changes }),
    )?;

    let push = read_request(&listener)?;
    require_path(&push, "/v1/replica/push")?;
    if push.method != "POST" {
        return Err(format!("expected POST push, got {}", push.method));
    }
    let persisted: Value = serde_json::from_slice(
        &std::fs::read(home.join("cloud.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if persisted["replica"]["outbox"]
        .as_array()
        .is_none_or(Vec::is_empty)
    {
        return Err("local outbox was not persisted before push".to_string());
    }
    let push_body: Value = serde_json::from_slice(&push.body).map_err(|error| error.to_string())?;
    let pushed = push_body["changes"]
        .as_array()
        .cloned()
        .ok_or_else(|| "push has no changes".to_string())?;
    let accepted = pushed
        .iter()
        .filter_map(|change| change["changeId"].as_str())
        .collect::<Vec<_>>();
    respond_json(
        push.stream,
        200,
        &json!({ "cursor": 2, "acceptedChangeIds": accepted }),
    )?;

    let second_pull = read_request(&listener)?;
    require_path(&second_pull, "/v1/replica/pull?cursor=1&limit=100")?;
    let echoed = pushed
        .into_iter()
        .map(|mut change| {
            change["cursor"] = json!(2);
            change["deviceId"] = json!("tui-test-device");
            change
        })
        .collect::<Vec<_>>();
    respond_json(
        second_pull.stream,
        200,
        &json!({ "cursor": 2, "hasMore": false, "changes": echoed }),
    )?;

    let snapshot = read_request(&listener)?;
    require_path(&snapshot, "/v1/replica/snapshot")?;
    if snapshot.method != "PUT"
        || snapshot.headers.get("content-type").map(String::as_str)
            != Some("application/octet-stream")
        || snapshot
            .headers
            .get("x-readio-replica-cursor")
            .map(String::as_str)
            != Some("2")
        || snapshot.body.is_empty()
    {
        return Err("snapshot was not uploaded as cursor-bound binary data".to_string());
    }
    respond_json(
        snapshot.stream,
        201,
        &json!({ "cursor": 2, "etag": "test-etag", "byteLength": snapshot.body.len() }),
    )
}

struct TestRequest {
    stream: TcpStream,
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn read_request(listener: &TcpListener) -> Result<TestRequest, String> {
    let (mut stream, _) = listener.accept().map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("connection closed before request headers".to_string());
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers_text = std::str::from_utf8(&bytes[..header_end]).map_err(|e| e.to_string())?;
    let mut lines = headers_text.split("\r\n");
    let mut request_line = lines
        .next()
        .ok_or_else(|| "missing request line".to_string())?
        .split_whitespace();
    let method = request_line.next().unwrap_or_default().to_string();
    let path = request_line.next().unwrap_or_default().to_string();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
        .collect::<BTreeMap<_, _>>();
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while bytes.len() - header_end < content_length {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("connection closed before request body".to_string());
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    let body = bytes[header_end..header_end + content_length].to_vec();
    Ok(TestRequest {
        stream,
        method,
        path,
        headers,
        body,
    })
}

fn require_path(request: &TestRequest, expected: &str) -> Result<(), String> {
    if request.path == expected {
        Ok(())
    } else {
        Err(format!("expected {expected}, got {}", request.path))
    }
}

fn respond_json(mut stream: TcpStream, status: u16, body: &Value) -> Result<(), String> {
    let body = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    let reason = if status == 201 { "Created" } else { "OK" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    )
    .map_err(|error| error.to_string())?;
    stream.write_all(&body).map_err(|error| error.to_string())
}

fn temporary_home() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("readio-crdt-sync-{}-{nonce}", std::process::id()))
}
