#![cfg(target_vendor = "apple")]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::{env, fs, path::PathBuf, process::Command, thread};

#[test]
fn apple_adapter_executes_the_shared_rust_sync_state_in_durable_order() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("the FFI crate lives under the repository crates directory");
    let sdk = repo_root.join("apps/ios/Readio/Readio/Core/ReadioSDK.swift");
    let adapter = repo_root.join("apps/ios/Readio/Readio/Core/AppleReplicaSync.swift");
    let include_dir = manifest_dir.join("include");
    let deps_dir = env::current_exe()
        .expect("the test executable has a path")
        .parent()
        .expect("the test executable lives in Cargo's deps directory")
        .to_owned();
    let static_library = deps_dir.join("libreadio_ffi.a");
    let fixture_dir = env::temp_dir().join(format!("readio-apple-sync-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("AppleSyncSmoke.swift");
    let executable = fixture_dir.join("apple-sync-smoke");
    fs::write(&fixture, SWIFT_FIXTURE).expect("the Swift fixture can be written");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg("-I")
        .arg(&include_dir)
        .arg(&sdk)
        .arg(repo_root.join("apps/ios/Readio/Readio/Core/AppleDiagnostics.swift"))
        .arg(&adapter)
        .arg(repo_root.join("apps/ios/Readio/Readio/Core/AppleLocalReplicaStore.swift"))
        .arg(&fixture)
        .arg(&static_library)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify the Apple sync adapter");
    assert!(
        compile.status.success(),
        "the Apple sync adapter must compile against the Rust SDK:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind Apple transport fixture");
    let server_url = format!("http://{}", listener.local_addr().expect("fixture address"));
    let server = thread::spawn(move || serve_transport(listener));
    let run = Command::new(&executable)
        .env("READIO_TEST_SERVER", server_url)
        .output()
        .expect("the Apple sync fixture can run");
    let server_result = server.join().expect("Apple transport server thread");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "the Apple adapter must preserve sync ordering:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    server_result.expect("the URLSession adapter must honor the replica HTTP contract");
}

fn serve_transport(listener: TcpListener) -> Result<(), String> {
    let pull = read_request(&listener)?;
    require_request(&pull, "GET", "/v1/replica/pull?cursor=3&limit=100")?;
    require_bearer(&pull)?;
    respond_json(
        pull.stream,
        200,
        r#"{"cursor":3,"hasMore":false,"changes":[]}"#,
    )?;

    let push = read_request(&listener)?;
    require_request(&push, "POST", "/v1/replica/push")?;
    require_bearer(&push)?;
    let push_body = String::from_utf8(push.body).map_err(|error| error.to_string())?;
    if !push_body.contains(r#""deviceId":"apple-http""#)
        || !push_body.contains(r#""ackCursor":3"#)
        || !push_body.contains(
            r#""changeId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa""#,
        )
    {
        return Err(format!("invalid push body: {push_body}"));
    }
    respond_json(
        push.stream,
        200,
        r#"{"cursor":4,"acceptedChangeIds":["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}"#,
    )?;

    let snapshot = read_request(&listener)?;
    require_request(&snapshot, "PUT", "/v1/replica/snapshot")?;
    require_bearer(&snapshot)?;
    if snapshot.headers.get("content-type").map(String::as_str) != Some("application/octet-stream")
        || snapshot
            .headers
            .get("x-readio-device-id")
            .map(String::as_str)
            != Some("apple-http")
        || snapshot
            .headers
            .get("x-readio-replica-cursor")
            .map(String::as_str)
            != Some("4")
        || snapshot.body != [1, 2, 3]
    {
        return Err("invalid binary snapshot request".to_string());
    }
    respond_json(
        snapshot.stream,
        201,
        r#"{"cursor":4,"etag":"fixture","byteLength":3}"#,
    )
}

struct HttpRequest {
    stream: TcpStream,
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn read_request(listener: &TcpListener) -> Result<HttpRequest, String> {
    let (mut stream, _) = listener.accept().map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let count = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("connection closed before headers".to_string());
        }
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let header_text = std::str::from_utf8(&bytes[..header_end]).map_err(|e| e.to_string())?;
    let mut lines = header_text.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split_whitespace();
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
        let count = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("connection closed before body".to_string());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    Ok(HttpRequest {
        stream,
        method,
        path,
        headers,
        body: bytes[header_end..header_end + content_length].to_vec(),
    })
}

fn require_request(request: &HttpRequest, method: &str, path: &str) -> Result<(), String> {
    if request.method == method && request.path == path {
        Ok(())
    } else {
        Err(format!(
            "expected {method} {path}, got {} {}",
            request.method, request.path
        ))
    }
}

fn require_bearer(request: &HttpRequest) -> Result<(), String> {
    if request.headers.get("authorization").map(String::as_str) == Some("Bearer test-token") {
        Ok(())
    } else {
        Err("missing bearer token".to_string())
    }
}

fn respond_json(mut stream: TcpStream, status: u16, body: &str) -> Result<(), String> {
    let reason = if status == 201 { "Created" } else { "OK" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|error| error.to_string())
}

const SWIFT_FIXTURE: &str = r####"
import Foundation

actor MemoryReplicaPersistence: AppleReplicaSyncPersistence {
    var record: AppleReplicaSyncRecord?

    func load() async throws -> AppleReplicaSyncRecord? { record }
    func save(_ record: AppleReplicaSyncRecord) async throws { self.record = record }
}

actor FixtureTransport: AppleReplicaTransport {
    let persistence: MemoryReplicaPersistence
    let remote: AppleReplicaStoredChange
    let concurrentStore: AppleLocalReplicaStore
    var pushed: [AppleReplicaChange] = []
    var pushObservedDurableOutbox = false
    var uploadedCursor: UInt64?
    var uploadedBytes = 0
    var concurrentWrite: Task<Void, Error>?

    init(
        persistence: MemoryReplicaPersistence,
        remote: AppleReplicaStoredChange,
        concurrentStore: AppleLocalReplicaStore
    ) {
        self.persistence = persistence
        self.remote = remote
        self.concurrentStore = concurrentStore
    }

    func pull(after cursor: UInt64, limit: Int) async throws -> AppleReplicaPullPage {
        if cursor == 0 {
            let store = concurrentStore
            concurrentWrite = Task {
                try await store.stageLocal(
                    entitiesJSON: #"[{"entityKind":"setting","entityId":"during_sync","payload":{"value":"kept"}}]"#
                )
            }
            await Task.yield()
            return AppleReplicaPullPage(cursor: 1, hasMore: false, changes: [remote])
        }
        if cursor == 1, let change = pushed.first {
            return AppleReplicaPullPage(
                cursor: 2,
                hasMore: false,
                changes: [AppleReplicaStoredChange(
                    cursor: 2,
                    changeId: change.changeId,
                    deviceId: "apple-test",
                    changeBase64: change.changeBase64
                )]
            )
        }
        return AppleReplicaPullPage(cursor: cursor, hasMore: false, changes: [])
    }

    func push(_ request: AppleReplicaPushRequest) async throws -> AppleReplicaPushResult {
        let saved = try await persistence.load()
        pushObservedDurableOutbox = saved?.stateJSON.contains("changeBase64") == true
        pushed = request.changes
        return AppleReplicaPushResult(
            cursor: 2,
            acceptedChangeIds: request.changes.map(\.changeId)
        )
    }

    func uploadSnapshot(_ data: Data, deviceId: String, cursor: UInt64) async throws {
        uploadedCursor = cursor
        uploadedBytes = data.count
    }

    func verdict() -> (Bool, UInt64?, Int) {
        (pushObservedDurableOutbox, uploadedCursor, uploadedBytes)
    }

    func waitForConcurrentWrite() async throws {
        try await concurrentWrite?.value
    }
}

@main
enum AppleSyncSmoke {
    static func main() async throws {
        let server = ProcessInfo.processInfo.environment["READIO_TEST_SERVER"]!
        let httpTransport = URLSessionAppleReplicaTransport(
            baseURL: URL(string: server)!,
            bearerToken: "test-token"
        )
        let httpPull = try await httpTransport.pull(after: 3, limit: 100)
        guard httpPull.cursor == 3, httpPull.changes.isEmpty else {
            throw FixtureError.httpContractFailed
        }
        let changeID = String(repeating: "a", count: 64)
        let httpPush = try await httpTransport.push(AppleReplicaPushRequest(
            deviceId: "apple-http",
            ackCursor: 3,
            headsBase64: "BAUG",
            changes: [AppleReplicaChange(changeId: changeID, changeBase64: "AQID")]
        ))
        guard httpPush.cursor == 4, httpPush.acceptedChangeIds == [changeID] else {
            throw FixtureError.httpContractFailed
        }
        try await httpTransport.uploadSnapshot(
            Data([1, 2, 3]),
            deviceId: "apple-http",
            cursor: 4
        )

        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("readio-apple-sync-\(UUID().uuidString).json")
        let filePersistence = FileAppleReplicaSyncPersistence(fileURL: fileURL)
        let fileRecord = AppleReplicaSyncRecord(cursor: 7, stateJSON: #"{"outbox":[]}"#)
        try await filePersistence.save(fileRecord)
        guard try await filePersistence.load() == fileRecord else {
            throw FixtureError.fileRoundTripFailed
        }
        try FileManager.default.removeItem(at: fileURL)

        var remoteState = ReadioSDK().makeSyncState()
        try remoteState.stageLocal(
            entitiesJSON: #"[{"entityKind":"reading_position","entityId":"remote-book","payload":{"char_offset":144}}]"#
        )
        let remoteView = try JSONSerialization.jsonObject(
            with: Data(try remoteState.viewJSON().utf8)
        ) as! [String: Any]
        let remoteOutbox = remoteView["outbox"] as! [[String: Any]]
        let remoteChange = AppleReplicaStoredChange(
            cursor: 1,
            changeId: remoteOutbox[0]["changeId"] as! String,
            deviceId: "other-device",
            changeBase64: remoteOutbox[0]["changeBase64"] as! String
        )

        let persistence = MemoryReplicaPersistence()
        let accessGate = AppleReplicaAccessGate()
        let concurrentStore = AppleLocalReplicaStore(
            statePersistence: persistence,
            contentPersistence: FileAppleLibraryContentPersistence(
                rootDirectory: FileManager.default.temporaryDirectory.appendingPathComponent(
                    "readio-sync-content-\(UUID().uuidString)"
                )
            ),
            accessGate: accessGate
        )
        let transport = FixtureTransport(
            persistence: persistence,
            remote: remoteChange,
            concurrentStore: concurrentStore
        )
        let diagnostics = AppleDiagnosticLog(
            now: { 250 },
            makeOperationID: { "op-apple-sync" }
        )
        let coordinator = AppleReplicaSyncCoordinator(
            deviceId: "apple-test",
            persistence: persistence,
            transport: transport,
            accessGate: accessGate,
            diagnostics: diagnostics
        )
        let viewJSON = try await coordinator.sync(
            localEntitiesJSON: #"[{"entityKind":"setting","entityId":"theme","payload":{"value":"dark"}}]"#
        )
        let view = try JSONSerialization.jsonObject(with: Data(viewJSON.utf8)) as! [String: Any]
        let state = view["state"] as! [String: Any]
        let positions = state["reading_positions"] as! [String: Any]
        guard (positions["remote-book"] as? [String: Any])?["char_offset"] as? Int == 144 else {
            throw FixtureError.remoteStateMissing
        }

        let saved = try await persistence.load()
        guard saved?.cursor == 2, saved?.stateJSON.contains(#""outbox":[]"#) == true else {
            throw FixtureError.outboxNotAcknowledged
        }
        let verdict = await transport.verdict()
        guard verdict.0, verdict.1 == 2, verdict.2 > 0 else {
            throw FixtureError.invalidEffectOrder
        }
        try await transport.waitForConcurrentWrite()
        guard try await concurrentStore.settingsJSON().contains(#""during_sync":"kept""#) else {
            throw FixtureError.concurrentLocalWriteWasLost
        }
        let diagnosticEvents = await diagnostics.snapshot()
        guard diagnosticEvents.allSatisfy({ $0.operationID == "op-apple-sync" }),
              diagnosticEvents.contains(where: { event in
                  event.name == "pull_completed"
                      && event.fields["direction"] == .string("pull")
                      && event.fields["change_count"] == .number(1)
              }),
              diagnosticEvents.contains(where: { event in
                  event.name == "push_completed"
                      && event.fields["direction"] == .string("push")
                      && event.fields["change_count"] == .number(1)
              }),
              diagnosticEvents.contains(where: { event in
                  event.name == "snapshot_uploaded"
                      && event.fields["cursor"] == .number(2)
              }) else {
            throw FixtureError.missingDiagnostics
        }
    }
}

enum FixtureError: Error {
    case fileRoundTripFailed
    case httpContractFailed
    case remoteStateMissing
    case outboxNotAcknowledged
    case invalidEffectOrder
    case concurrentLocalWriteWasLost
    case missingDiagnostics
}
"####;
