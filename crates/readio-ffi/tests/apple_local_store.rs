#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_local_store_persists_rust_replica_and_library_content() {
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
        env::temp_dir().join(format!("readio-apple-local-store-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("AppleLocalStoreSmoke.swift");
    let executable = fixture_dir.join("apple-local-store-smoke");
    fs::write(&fixture, SWIFT_FIXTURE).expect("the Swift fixture can be written");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg("-I")
        .arg(&include_dir)
        .arg(apple_core.join("ReadioSDK.swift"))
        .arg(apple_core.join("AppleReplicaSync.swift"))
        .arg(apple_core.join("AppleLocalReplicaStore.swift"))
        .arg(&fixture)
        .arg(&static_library)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify the Apple local store");
    assert!(
        compile.status.success(),
        "the Apple local store must compile against the Rust SDK:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Apple local store fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "the Apple local store must survive restart and preserve Rust semantics:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r####"
import Foundation

@main
enum AppleLocalStoreSmoke {
    static func main() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("readio-local-store-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let state = FileAppleReplicaSyncPersistence(fileURL: root.appendingPathComponent("replica.json"))
        let content = FileAppleLibraryContentPersistence(rootDirectory: root.appendingPathComponent("content"))
        let store = AppleLocalReplicaStore(statePersistence: state, contentPersistence: content)
        let itemJSON = #"{"id":"book/1","title":"Offline Native","type":"txt","progress":0,"date":"2026-08-06T12:00:00Z","category":"imported","folder_id":"f1","source":"notes.txt","file_path":"local:book/1","cover_image":null,"voice":null,"speed":null,"current_chapter":null,"content_format":"plaintext"}"#
        let contentRecord = AppleLibraryContentRecord(
            itemId: "book/1",
            content: "Durable local text",
            fileData: Data([0, 1, 2, 255]),
            mediaType: "text/plain",
            fileName: "notes.txt"
        )
        try await store.saveLibraryItem(metadataJSON: itemJSON, content: contentRecord)
        try await store.stageLocal(
            entitiesJSON: #"[{"entityKind":"setting","entityId":"theme","payload":{"value":"dark"}}]"#
        )

        let page = try await store.queryLibrary(queryJSON: #"{"page":1,"page_size":20}"#)
        let pageObject = try JSONSerialization.jsonObject(with: Data(page.utf8)) as! [String: Any]
        let pageItems = pageObject["items"] as! [[String: Any]]
        guard pageItems.first?["title"] as? String == "Offline Native" else {
            throw FixtureError.libraryProjectionMissing
        }
        let settings = try await store.settingsJSON()
        guard settings == #"{"theme":"dark"}"# else {
            throw FixtureError.settingsProjectionMissing
        }

        let mutation = #"{"document_revision":"revision-1","char_offset":17,"affinity":"forward","observed_at_unix_ms":1786000000000,"session_id":"apple-local","mutation_id":"position-1","base_version":0}"#
        let transition = try await store.saveReadingPosition(itemID: "book/1", mutationJSON: mutation)
        let transitionObject = try JSONSerialization.jsonObject(
            with: Data(transition.utf8)
        ) as! [String: Any]
        guard transitionObject["version"] as? Int == 1 else {
            throw FixtureError.positionTransitionMissing
        }

        let restarted = AppleLocalReplicaStore(statePersistence: state, contentPersistence: content)
        let restoredItem = try await restarted.libraryItemJSON(itemID: "book/1")
        guard restoredItem.contains("Offline Native") else {
            throw FixtureError.metadataNotRestored
        }
        guard try await restarted.libraryContent(itemID: "book/1") == contentRecord else {
            throw FixtureError.contentNotRestored
        }
        let restoredPosition = try await restarted.readingPositionJSON(itemID: "book/1")
        guard restoredPosition.contains(#""char_offset":17"#),
              restoredPosition.contains(#""version":1"#) else {
            throw FixtureError.positionNotRestored
        }

        try await restarted.deleteLibraryItem(itemID: "book/1")
        guard try await restarted.libraryItemJSON(itemID: "book/1") == "null",
              try await restarted.libraryContent(itemID: "book/1") == nil else {
            throw FixtureError.deleteDidNotRemoveBothStores
        }
    }
}

enum FixtureError: Error {
    case libraryProjectionMissing
    case settingsProjectionMissing
    case positionTransitionMissing
    case metadataNotRestored
    case contentNotRestored
    case positionNotRestored
    case deleteDidNotRemoveBothStores
}
"####;
