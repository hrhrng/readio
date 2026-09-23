#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_bookmark_service_uses_rust_projection_and_transition_rules() {
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
        env::temp_dir().join(format!("readio-apple-bookmarks-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("AppleBookmarkSmoke.swift");
    let executable = fixture_dir.join("apple-bookmark-smoke");
    fs::write(&fixture, SWIFT_FIXTURE).expect("the Swift fixture can be written");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg("-I")
        .arg(&include_dir)
        .arg(apple_core.join("ReadioSDK.swift"))
        .arg(apple_core.join("AppleReplicaSync.swift"))
        .arg(apple_core.join("AppleLocalReplicaStore.swift"))
        .arg(apple_core.join("AppleBookmarkService.swift"))
        .arg(&fixture)
        .arg(&static_library)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify Apple bookmarks");
    assert!(
        compile.status.success(),
        "the Apple bookmark service must compile against the Rust SDK:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Apple bookmark fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "the Apple bookmark service must preserve Rust semantics:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r#"
import Foundation

@main
enum AppleBookmarkSmoke {
    static func main() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("readio-bookmarks-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = AppleLocalReplicaStore(
            statePersistence: FileAppleReplicaSyncPersistence(
                fileURL: root.appendingPathComponent("replica.json")
            ),
            contentPersistence: FileAppleLibraryContentPersistence(
                rootDirectory: root.appendingPathComponent("content")
            )
        )
        let service = AppleBookmarkService(localStore: store)
        let document = try ReadioSDK().parseHTML("<p>Alpha beta</p><p>Gamma</p>")
        guard try await service.list(itemID: "book-1").isEmpty else {
            throw FixtureError.initialListWasNotEmpty
        }

        let created = try await service.upsert(
            documentJSON: document,
            request: AppleBookmarkUpsertRequest(
                bookmarkId: "bookmark-a",
                itemId: "book-1",
                charOffset: 0,
                affinity: .forward,
                note: "",
                createdAtUnixMilliseconds: 10,
                updatedAtUnixMilliseconds: 10,
                baseVersion: 0
            )
        )
        guard created.created,
              created.bookmark.note == "Alpha beta",
              created.bookmark.version == 1 else {
            throw FixtureError.createFailed
        }
        let updated = try await service.upsert(
            documentJSON: document,
            request: AppleBookmarkUpsertRequest(
                bookmarkId: "ignored-at-same-offset",
                itemId: "book-1",
                charOffset: 0,
                affinity: .forward,
                note: "Pinned",
                createdAtUnixMilliseconds: 99,
                updatedAtUnixMilliseconds: 20,
                baseVersion: 1
            )
        )
        guard !updated.created,
              updated.bookmark.id == "bookmark-a",
              updated.bookmark.note == "Pinned",
              updated.bookmark.version == 2 else {
            throw FixtureError.updateFailed
        }
        guard try await service.list(itemID: "book-1") == [updated.bookmark] else {
            throw FixtureError.projectionFailed
        }
        try await service.delete(bookmarkID: "bookmark-a")
        guard try await service.list(itemID: "book-1").isEmpty else {
            throw FixtureError.deleteFailed
        }
    }
}

enum FixtureError: Error {
    case initialListWasNotEmpty
    case createFailed
    case updateFailed
    case projectionFailed
    case deleteFailed
}
"#;
