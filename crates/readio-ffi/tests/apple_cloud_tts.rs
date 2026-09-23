#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_cloud_tts_requires_a_session_and_resolves_private_job_audio() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("the FFI crate lives under the repository crates directory");
    let apple_core = repo_root.join("apps/ios/Readio/Readio/Core");
    let include_dir = manifest_dir.join("include");
    let deps_dir = env::current_exe()
        .expect("the test executable has a path")
        .parent()
        .expect("the test executable lives in Cargo's deps directory")
        .to_owned();
    let static_library = deps_dir.join("libreadio_ffi.a");
    let fixture_dir =
        env::temp_dir().join(format!("readio-apple-cloud-tts-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("AppleCloudTTSSmoke.swift");
    let executable = fixture_dir.join("apple-cloud-tts-smoke");
    fs::write(&fixture, SWIFT_FIXTURE).expect("the Swift fixture can be written");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg("-I")
        .arg(&include_dir)
        .arg(apple_core.join("ReadioSDK.swift"))
        .arg(apple_core.join("AppleDiagnostics.swift"))
        .arg(apple_core.join("AppleReplicaSync.swift"))
        .arg(apple_core.join("AppleLocalReplicaStore.swift"))
        .arg(apple_core.join("AppleCloudContentSync.swift"))
        .arg(apple_core.join("AppleCloudService.swift"))
        .arg(apple_core.join("AppleNativeTTS.swift"))
        .arg(apple_core.join("AppleCloudTTS.swift"))
        .arg(&fixture)
        .arg(&static_library)
        .arg("-framework")
        .arg("AVFoundation")
        .arg("-framework")
        .arg("Security")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify Apple cloud TTS");
    assert!(
        compile.status.success(),
        "the Apple cloud TTS adapter must compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Apple cloud TTS fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "Apple cloud TTS must enforce authentication and private audio fetches:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r#"
import Foundation

actor FixtureCloudAccount: AppleCloudSessionProviding {
    var value: AppleCloudSession?
    func session() async throws -> AppleCloudSession? { value }
    func connect(_ session: AppleCloudSession) { value = session }
}

actor FixtureTTSJobs: AppleCloudTTSJobTransport {
    var statusCalls = 0
    var receivedSession: AppleCloudSession?

    func create(
        session: AppleCloudSession,
        text: String,
        voiceID: String,
        speed: Double,
        operationID: String
    ) async throws -> AppleCloudTTSJob {
        receivedSession = session
        return AppleCloudTTSJob(
            id: "job-1",
            status: .queued,
            createdAtUnixMilliseconds: 10,
            audioPath: nil,
            errorCode: nil
        )
    }

    func get(
        session: AppleCloudSession,
        jobID: String,
        operationID: String
    ) async throws -> AppleCloudTTSJob {
        statusCalls += 1
        return AppleCloudTTSJob(
            id: jobID,
            status: statusCalls == 1 ? .running : .completed,
            createdAtUnixMilliseconds: 10,
            audioPath: statusCalls == 1 ? nil : "/v1/tts/jobs/job-1/audio",
            errorCode: nil
        )
    }

    func audio(
        session: AppleCloudSession,
        jobID: String,
        path: String,
        operationID: String
    ) async throws -> Data {
        Data("ID3-private-audio".utf8)
    }
}

final class FixtureURLProtocol: URLProtocol, @unchecked Sendable {
    private static let lock = NSLock()
    private static var captured: [URLRequest] = []

    static func reset() {
        lock.withLock { captured.removeAll() }
    }

    static func requests() -> [URLRequest] {
        lock.withLock { captured }
    }

    override class func canInit(with request: URLRequest) -> Bool { true }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest {
        request
    }

    override func startLoading() {
        Self.lock.withLock { Self.captured.append(request) }
        let path = request.url?.path ?? ""
        let body: Data
        let contentType: String
        if path.hasSuffix("/audio") {
            body = Data("ID3-http-audio".utf8)
            contentType = "audio/mpeg"
        } else if request.httpMethod == "POST" {
            body = Data(
                "{\"id\":\"job-http\",\"status\":\"queued\",\"createdAtUnixMs\":10}".utf8
            )
            contentType = "application/json"
        } else {
            body = Data(
                "{\"id\":\"job-http\",\"status\":\"completed\",\"createdAtUnixMs\":10,\"audioURL\":\"/v1/tts/jobs/job-http/audio\"}".utf8
            )
            contentType = "application/json"
        }
        let response = HTTPURLResponse(
            url: request.url!,
            statusCode: 200,
            httpVersion: "HTTP/1.1",
            headerFields: ["Content-Type": contentType]
        )!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: body)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}

@main
enum AppleCloudTTSSmoke {
    static func main() async throws {
        let account = FixtureCloudAccount()
        let jobs = FixtureTTSJobs()
        let client = AppleCloudTTSClient(
            account: account,
            transport: jobs,
            pollInterval: .zero,
            maximumPollCount: 3
        )
        do {
            _ = try await client.synthesize(
                text: "Private sentence",
                voiceID: "English_expressive_narrator",
                speed: 1
            )
            throw FixtureError.anonymousTTSWasAllowed
        } catch AppleCloudError.authenticationRequired {
            // Expected.
        }
        let anonymousEvents = AppleDiagnosticLog.shared.snapshot()
        guard anonymousEvents.map(\.name) == ["job_requested", "job_failed"],
              anonymousEvents.count == 2,
              anonymousEvents[0].operationID == anonymousEvents[1].operationID,
              anonymousEvents[1].fields["provider"] == .string("cloud"),
              anonymousEvents[1].fields["reason_code"]
                  == .string("authentication_required") else {
            throw FixtureError.missingFailureDiagnostics
        }
        AppleDiagnosticLog.shared.clear()

        let session = AppleCloudSession(
            serverURL: URL(string: "https://cloud.read-io.com")!,
            accessToken: "secret-token",
            deviceID: "apple-device"
        )
        await account.connect(session)
        let audio = try await client.synthesize(
            text: "Private sentence",
            voiceID: "English_expressive_narrator",
            speed: 1
        )
        guard String(decoding: audio, as: UTF8.self) == "ID3-private-audio",
              await jobs.receivedSession == session,
              await jobs.statusCalls == 2 else {
            throw FixtureError.privateAudioFailed
        }

        let events = AppleDiagnosticLog.shared.snapshot()
        guard events.map(\.name) == [
                  "job_requested",
                  "job_queued",
                  "job_running",
                  "job_completed",
                  "audio_downloaded",
              ],
              let operationID = events.first?.operationID,
              events.allSatisfy({ event in
                  event.operationID == operationID
                      && event.category == .tts
                      && event.fields["provider"] == .string("cloud")
              }),
              events.last?.fields["job_id"] == .string("job-1") else {
            throw FixtureError.missingDiagnostics
        }
        let encoded = String(
            decoding: try JSONEncoder().encode(events),
            as: UTF8.self
        )
        guard !encoded.contains("Private sentence"),
              !encoded.contains("secret-token"),
              !encoded.contains("cloud.read-io.com") else {
            throw FixtureError.privateDiagnostics
        }

        FixtureURLProtocol.reset()
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [FixtureURLProtocol.self]
        let httpDiagnostics = AppleDiagnosticLog(
            capacity: 16,
            now: { 1_000 },
            makeOperationID: { "op-apple-cloud-http" }
        )
        let httpClient = AppleCloudTTSClient(
            account: account,
            transport: URLSessionAppleCloudTTSJobTransport(
                urlSession: URLSession(configuration: configuration)
            ),
            pollInterval: .zero,
            maximumPollCount: 2,
            diagnostics: httpDiagnostics
        )
        _ = try await httpClient.synthesize(
            text: "A second private sentence",
            voiceID: "English_expressive_narrator",
            speed: 1
        )
        let requests = FixtureURLProtocol.requests()
        guard requests.count == 3,
              requests.allSatisfy({ request in
                  request.value(forHTTPHeaderField: "x-readio-operation-id")
                      == "op-apple-cloud-http"
              }),
              httpDiagnostics.snapshot().allSatisfy({ event in
                  event.operationID == "op-apple-cloud-http"
              }) else {
            throw FixtureError.operationIDDidNotCrossHTTP
        }
    }
}

enum FixtureError: Error {
    case anonymousTTSWasAllowed
    case privateAudioFailed
    case missingFailureDiagnostics
    case missingDiagnostics
    case privateDiagnostics
    case operationIDDidNotCrossHTTP
}
"#;
