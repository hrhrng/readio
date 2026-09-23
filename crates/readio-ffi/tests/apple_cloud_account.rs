#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_cloud_account_is_optional_and_gates_sync_behind_device_login() {
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
    let fixture_dir = env::temp_dir().join(format!("readio-apple-cloud-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("AppleCloudSmoke.swift");
    let executable = fixture_dir.join("apple-cloud-smoke");
    fs::write(&fixture, SWIFT_FIXTURE).expect("the Swift fixture can be written");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg("-I")
        .arg(&include_dir)
        .arg(apple_core.join("ReadioSDK.swift"))
        .arg(apple_core.join("AppleReplicaSync.swift"))
        .arg(apple_core.join("AppleLocalReplicaStore.swift"))
        .arg(apple_core.join("AppleCloudContentSync.swift"))
        .arg(apple_core.join("AppleCloudService.swift"))
        .arg(&fixture)
        .arg(&static_library)
        .arg("-framework")
        .arg("Security")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify Apple cloud account behavior");
    assert!(
        compile.status.success(),
        "the Apple cloud account service must compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Apple cloud account fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "Apple cloud access must remain optional and authenticated:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r####"
import Foundation

actor MemoryCloudSessionPersistence: AppleCloudSessionPersistence {
    var value: AppleCloudSession?
    func load() async throws -> AppleCloudSession? { value }
    func save(_ session: AppleCloudSession) async throws { value = session }
    func delete() async throws { value = nil }
}

actor FixtureDeviceTransport: AppleDeviceAuthorizationTransport {
    var polls = 0

    func requestCode(serverURL: URL) async throws -> AppleDeviceAuthorization {
        AppleDeviceAuthorization(
            deviceCode: "device-code",
            userCode: "ABCD-EFGH",
            verificationURL: URL(string: "https://app.read-io.com/device")!,
            verificationURLComplete: URL(string: "https://app.read-io.com/device?user_code=ABCD-EFGH"),
            expiresInSeconds: 600,
            intervalSeconds: 1
        )
    }

    func exchange(serverURL: URL, deviceCode: String) async throws -> AppleDeviceTokenResult {
        polls += 1
        return polls == 1 ? .pending : .authorized(accessToken: "secret-token")
    }
}

actor FixtureCloudSync: AppleCloudSyncExecuting {
    var received: AppleCloudSession?
    var count = 0
    func sync(session: AppleCloudSession) async throws -> String {
        count += 1
        received = session
        try await Task.sleep(for: .milliseconds(20))
        return #"{"state":{}}"#
    }
}

actor FixtureReplicaRefresh: AppleReplicaRefreshNotifying {
    var count = 0

    func didSync() async {
        count += 1
    }
}

@main
enum AppleCloudSmoke {
    static func main() async throws {
        let sessions = MemoryCloudSessionPersistence()
        let auth = FixtureDeviceTransport()
        let sync = FixtureCloudSync()
        let refresh = FixtureReplicaRefresh()
        let service = AppleCloudService(
            authorizationTransport: auth,
            sessionPersistence: sessions,
            syncExecutor: sync,
            refreshNotifier: refresh,
            makeDeviceID: { "apple-device" }
        )

        guard try await service.session() == nil else {
            throw FixtureError.anonymousStateMissing
        }
        do {
            _ = try await service.sync()
            throw FixtureError.anonymousSyncWasAllowed
        } catch AppleCloudError.authenticationRequired {
            // Expected: local use remains available, cloud effects do not.
        }
        guard try await service.syncIfConnected() == false,
              await sync.received == nil,
              await refresh.count == 0 else {
            throw FixtureError.anonymousAutomaticSyncWasNotLocal
        }

        let server = URL(string: "https://cloud.read-io.com")!
        let code = try await service.beginLogin(serverURL: server)
        guard code.userCode == "ABCD-EFGH" else {
            throw FixtureError.deviceCodeFailed
        }
        guard try await service.pollLogin(serverURL: server, deviceCode: code.deviceCode) == .pending else {
            throw FixtureError.pendingStateFailed
        }
        let connected = try await service.pollLogin(
            serverURL: server,
            deviceCode: code.deviceCode
        )
        guard case .connected(let session) = connected,
              session.accessToken == "secret-token",
              session.deviceID == "apple-device",
              try await service.session() == session else {
            throw FixtureError.sessionWasNotPersisted
        }

        _ = try await service.sync()
        guard await sync.received == session,
              await refresh.count == 1,
              try await service.syncIfConnected(),
              await refresh.count == 2 else {
            throw FixtureError.syncDidNotUseSession
        }
        async let firstAutomaticSync = service.syncIfConnected()
        async let secondAutomaticSync = service.syncIfConnected()
        guard try await (firstAutomaticSync, secondAutomaticSync) == (true, true),
              await sync.count == 3,
              await refresh.count == 3 else {
            throw FixtureError.concurrentSyncWasNotCoalesced
        }
        try await service.logout()
        guard try await service.session() == nil else {
            throw FixtureError.logoutFailed
        }
    }
}

enum FixtureError: Error {
    case anonymousStateMissing
    case anonymousSyncWasAllowed
    case anonymousAutomaticSyncWasNotLocal
    case deviceCodeFailed
    case pendingStateFailed
    case sessionWasNotPersisted
    case syncDidNotUseSession
    case concurrentSyncWasNotCoalesced
    case logoutFailed
}
"####;
