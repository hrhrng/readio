#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_reading_position_service_preserves_rust_version_and_conflict_semantics() {
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
    let fixture_dir = env::temp_dir().join(format!("readio-apple-position-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("AppleReadingPositionSmoke.swift");
    let executable = fixture_dir.join("apple-reading-position-smoke");
    fs::write(&fixture, SWIFT_FIXTURE).expect("the Swift fixture can be written");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg("-I")
        .arg(&include_dir)
        .arg(apple_core.join("ReadioSDK.swift"))
        .arg(apple_core.join("AppleReplicaSync.swift"))
        .arg(apple_core.join("AppleLocalReplicaStore.swift"))
        .arg(apple_core.join("AppleReadingPositionService.swift"))
        .arg(&fixture)
        .arg(&static_library)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify Apple reading positions");
    assert!(
        compile.status.success(),
        "the Apple reading-position service must compile against the Rust SDK:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Apple reading-position fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "the Apple reading-position service must retain version semantics:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r#"
import Foundation

@main
enum AppleReadingPositionSmoke {
    static func main() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("readio-position-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = AppleLocalReplicaStore(
            statePersistence: FileAppleReplicaSyncPersistence(
                fileURL: root.appendingPathComponent("replica.json")
            ),
            contentPersistence: FileAppleLibraryContentPersistence(
                rootDirectory: root.appendingPathComponent("content")
            )
        )
        let service = AppleReadingPositionService(localStore: store)
        guard try await service.fetch(itemID: "book-1") == nil else {
            throw FixtureError.missingPositionWasNotNil
        }
        let locator = AppleTextLocator(
            version: 1,
            documentRevision: "revision-1",
            charOffset: 21,
            blockId: "block-stable",
            offsetInBlock: 4,
            affinity: .forward,
            quote: AppleTextQuote(
                prefix: "before ",
                exact: "stable text",
                suffix: " after",
                offsetInExact: 0
            )
        )
        let mutation = AppleReadingPositionMutation(
            documentRevision: "revision-1",
            charOffset: 21,
            affinity: .forward,
            observedAtUnixMilliseconds: 1_786_000_000_000,
            sessionId: "apple-a",
            mutationId: "mutation-a",
            baseVersion: 0,
            locator: locator
        )
        let applied = try await service.save(itemID: "book-1", mutation: mutation)
        guard applied.status == .applied,
              applied.version == 1,
              applied.position?.charOffset == 21,
              applied.position?.locator == locator else {
            throw FixtureError.applyFailed
        }
        let replay = try await service.save(itemID: "book-1", mutation: mutation)
        guard replay.idempotentReplay, replay.version == 1 else {
            throw FixtureError.replayFailed
        }
        let conflict = try await service.save(
            itemID: "book-1",
            mutation: AppleReadingPositionMutation(
                documentRevision: "revision-1",
                charOffset: 34,
                affinity: .forward,
                observedAtUnixMilliseconds: 1_786_000_000_100,
                sessionId: "apple-b",
                mutationId: "mutation-b",
                baseVersion: 0
            )
        )
        guard conflict.status == .conflict,
              conflict.version == 1,
              conflict.candidates.map(\.origin) == [.current, .incoming] else {
            throw FixtureError.conflictFailed
        }
        let restarted = AppleReadingPositionService(localStore: store)
        guard let restored = try await restarted.fetch(itemID: "book-1"),
              restored.version == 1,
              restored.locator == locator else {
            throw FixtureError.restartFailed
        }
    }
}

enum FixtureError: Error {
    case missingPositionWasNotNil
    case applyFailed
    case replayFailed
    case conflictFailed
    case restartFailed
}
"#;
