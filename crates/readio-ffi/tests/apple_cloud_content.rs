#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_cloud_content_reconciles_private_bytes_without_reuploading_matches() {
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
        env::temp_dir().join(format!("readio-apple-cloud-content-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("AppleCloudContentSmoke.swift");
    let executable = fixture_dir.join("apple-cloud-content-smoke");
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
        .expect("swiftc is required to verify Apple cloud content sync");
    assert!(
        compile.status.success(),
        "the Apple cloud content reconciler must compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Apple cloud content fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "Apple cloud content must reconcile without duplicate uploads:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r####"
import CryptoKit
import Foundation

actor MemoryContentPersistence: AppleLibraryContentPersistence {
    var records: [String: AppleLibraryContentRecord]

    init(records: [String: AppleLibraryContentRecord]) {
        self.records = records
    }

    func load(itemID: String) async throws -> AppleLibraryContentRecord? { records[itemID] }
    func save(_ record: AppleLibraryContentRecord) async throws { records[record.itemId] = record }
    func delete(itemID: String) async throws { records.removeValue(forKey: itemID) }
}

actor FixtureContentTransport: AppleCloudContentTransport {
    struct Key: Hashable { let itemID: String; let variant: AppleCloudContentVariant }
    var stored: [Key: Data]
    var uploadCount = 0

    init(stored: [Key: Data]) {
        self.stored = stored
    }

    func head(itemID: String, variant: AppleCloudContentVariant) async throws -> AppleCloudContentMetadata? {
        guard let data = stored[Key(itemID: itemID, variant: variant)] else { return nil }
        return metadata(itemID: itemID, variant: variant, data: data)
    }

    func upload(
        itemID: String,
        variant: AppleCloudContentVariant,
        data: Data,
        contentType: String,
        sha256Base64: String
    ) async throws -> AppleCloudContentMetadata {
        uploadCount += 1
        stored[Key(itemID: itemID, variant: variant)] = data
        return AppleCloudContentMetadata(
            itemID: itemID,
            variant: variant,
            byteLength: data.count,
            contentType: contentType,
            sha256Base64: sha256Base64,
            etag: "fixture"
        )
    }

    func download(
        itemID: String,
        variant: AppleCloudContentVariant
    ) async throws -> AppleCloudContentDownload? {
        guard let data = stored[Key(itemID: itemID, variant: variant)] else { return nil }
        return AppleCloudContentDownload(
            metadata: metadata(itemID: itemID, variant: variant, data: data),
            data: data
        )
    }

    func uploads() -> Int { uploadCount }

    private func metadata(
        itemID: String,
        variant: AppleCloudContentVariant,
        data: Data
    ) -> AppleCloudContentMetadata {
        AppleCloudContentMetadata(
            itemID: itemID,
            variant: variant,
            byteLength: data.count,
            contentType: variant == .reader ? "text/plain; charset=utf-8" : "application/epub+zip",
            sha256Base64: Data(SHA256.hash(data: data)).base64EncodedString(),
            etag: "fixture"
        )
    }
}

actor MemoryReplicaPersistence: AppleReplicaSyncPersistence {
    var record: AppleReplicaSyncRecord?
    func load() async throws -> AppleReplicaSyncRecord? { record }
    func save(_ record: AppleReplicaSyncRecord) async throws { self.record = record }
}

actor FixtureReplicaTransport: AppleReplicaTransport {
    func pull(after cursor: UInt64, limit: Int) async throws -> AppleReplicaPullPage {
        AppleReplicaPullPage(cursor: cursor, hasMore: false, changes: [])
    }

    func push(_ request: AppleReplicaPushRequest) async throws -> AppleReplicaPushResult {
        AppleReplicaPushResult(
            cursor: request.ackCursor,
            acceptedChangeIds: request.changes.map(\.changeId)
        )
    }

    func uploadSnapshot(_ data: Data, deviceId: String, cursor: UInt64) async throws {}
}

actor FixtureContentSyncExecutor: AppleCloudContentSyncExecuting {
    var calls: [(String, AppleCloudSession)] = []

    func sync(viewJSON: String, session: AppleCloudSession) async throws {
        calls.append((viewJSON, session))
    }

    func verdict() -> (Int, AppleCloudSession?) {
        (calls.count, calls.first?.1)
    }
}

@main
enum AppleCloudContentSmoke {
    static func main() async throws {
        let local = MemoryContentPersistence(records: [
            "local-book": AppleLibraryContentRecord(
                itemId: "local-book",
                content: "Local reader content",
                fileData: Data([1, 2, 3]),
                mediaType: "application/epub+zip",
                fileName: "local.epub"
            ),
        ])
        let remoteReader = Data("Remote reader content".utf8)
        let remoteSource = Data([4, 5, 6])
        let transport = FixtureContentTransport(stored: [
            .init(itemID: "remote-book", variant: .reader): remoteReader,
            .init(itemID: "remote-book", variant: .source): remoteSource,
        ])
        let reconciler = AppleLibraryContentCloudReconciler(
            contentPersistence: local,
            transport: transport
        )
        let view = #"{"state":{"library_items":{"local-book":{"type":"epub","source":"local.epub"},"remote-book":{"type":"epub","source":"remote.epub"}}}}"#

        try await reconciler.reconcile(viewJSON: view)
        guard let restored = try await local.load(itemID: "remote-book"),
              restored.content == "Remote reader content",
              restored.fileData == remoteSource,
              restored.mediaType == "application/epub+zip",
              restored.fileName == "remote.epub",
              await transport.uploads() == 2 else {
            throw FixtureError.firstReconciliationFailed
        }

        try await reconciler.reconcile(viewJSON: view)
        guard await transport.uploads() == 2 else {
            throw FixtureError.matchWasReuploaded
        }

        let session = AppleCloudSession(
            serverURL: URL(string: "https://cloud.read-io.com")!,
            accessToken: "private-token",
            deviceID: "apple-content"
        )
        let contentSync = FixtureContentSyncExecutor()
        let replicaTransport = FixtureReplicaTransport()
        let executor = AppleCloudReplicaSyncExecutor(
            persistence: MemoryReplicaPersistence(),
            contentSyncExecutor: contentSync,
            makeTransport: { _ in replicaTransport }
        )
        _ = try await executor.sync(session: session)
        let verdict = await contentSync.verdict()
        guard verdict.0 == 1, verdict.1 == session else {
            throw FixtureError.productionSyncSkippedContent
        }
    }
}

enum FixtureError: Error {
    case firstReconciliationFailed
    case matchWasReuploaded
    case productionSyncSkippedContent
}
"####;
