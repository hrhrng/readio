#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_reader_restores_the_saved_scalar_sentence_position() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("the FFI crate lives under the repository crates directory");
    let apple = repo_root.join("apps/ios/Readio/Readio");
    let include_dir = manifest_dir.join("include");
    let deps_dir = env::current_exe()
        .expect("the test executable has a path")
        .parent()
        .expect("the test executable lives in Cargo's deps directory")
        .to_owned();
    let static_library = deps_dir.join("libreadio_ffi.a");
    let fixture_dir =
        env::temp_dir().join(format!("readio-apple-reader-resume-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("AppleReaderResumeSmoke.swift");
    let executable = fixture_dir.join("apple-reader-resume-smoke");
    fs::write(&fixture, SWIFT_FIXTURE).expect("the Swift fixture can be written");

    let sources = [
        "Models/AppSettings.swift",
        "Models/LibraryItem.swift",
        "Models/LibraryItemSummary.swift",
        "Models/LibraryItemsResponse.swift",
        "Models/Sentence.swift",
        "Models/ContentBlock.swift",
        "Models/EpubChapter.swift",
        "Models/ReaderProjection.swift",
        "Models/RustEPUBPublication.swift",
        "Models/RustPDFPublication.swift",
        "Networking/APIEndpoints.swift",
        "Networking/APIClient.swift",
        "Core/ReadioSDK.swift",
        "Core/AppleReplicaSync.swift",
        "Core/AppleLocalReplicaStore.swift",
        "Core/AppleReadingPositionService.swift",
        "Core/AppleBookmarkService.swift",
        "Networking/LibraryService.swift",
        "ViewModels/ReaderViewModel.swift",
    ];
    let mut command = Command::new("swiftc");
    command
        .arg("-parse-as-library")
        .arg("-module-name")
        .arg("Readio")
        .arg("-I")
        .arg(&include_dir);
    for source in sources {
        command.arg(apple.join(source));
    }
    let compile = command
        .arg(&fixture)
        .arg(&static_library)
        .arg("-framework")
        .arg("SwiftUI")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify Apple reader resume");
    assert!(
        compile.status.success(),
        "the Apple reader must compile against the local position service:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Apple reader-resume fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "the Apple reader must restore the exact sentence cursor:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r#"
import Foundation

@main
enum AppleReaderResumeSmoke {
    @MainActor
    static func main() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("readio-reader-resume-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = AppleLocalReplicaStore(
            statePersistence: FileAppleReplicaSyncPersistence(
                fileURL: root.appendingPathComponent("replica.json")
            ),
            contentPersistence: FileAppleLibraryContentPersistence(
                rootDirectory: root.appendingPathComponent("content")
            )
        )
        let library = LibraryService(localStore: store)
        let positions = AppleReadingPositionService(localStore: store)
        let bookmarks = AppleBookmarkService(localStore: store)
        let item = try await library.importFile(
            data: Data("First sentence. Second sentence. Third sentence.".utf8),
            filename: "resume.txt"
        )

        let first = ReaderViewModel(
            libraryService: library,
            positionService: positions,
            bookmarkService: bookmarks,
            sessionID: "reader-a"
        )
        await first.loadItem(id: item.id)
        guard first.sentences.map(\.charOffset) == [0, 16, 33] else {
            throw FixtureError.scalarOffsetsWrong
        }
        first.search("ＳＥＮＴＥＮＣＥ")
        guard first.searchResult?.totalOccurrences == 3,
              first.searchResult?.totalPassages == 1,
              first.searchHits.count == 3,
              first.searchHits[0].snippet.contains("First sentence"),
              first.searchHits[0].snippetMatchRange == [6, 14] else {
            throw FixtureError.searchDidNotUseRustFoldingAndSourceRanges
        }
        guard first.activateSearchHit(at: 1) == 1,
              first.activeSearchHitIndex == 1,
              first.searchQuery == "ＳＥＮＴＥＮＣＥ",
              first.walkSearchHits(.next) == 2,
              first.walkSearchHits(.next) == 0,
              first.walkSearchHits(.previous) == 2 else {
            throw FixtureError.searchHitWalkingDidNotPreserveTheReaderCursor
        }
        first.search("   ")
        guard first.searchResult == nil,
              first.searchHits.isEmpty,
              first.searchQuery.isEmpty,
              first.activeSearchHitIndex == nil else {
            throw FixtureError.emptySearchDidNotClearResults
        }
        first.enqueueProgressSave(sentenceIndex: 0)
        first.enqueueProgressSave(sentenceIndex: 2)
        first.enqueueProgressSave(sentenceIndex: 1)
        await first.flushProgressSaves()
        guard let savedPosition = try await positions.fetch(itemID: item.id),
              savedPosition.charOffset == 16,
              savedPosition.locator?.offsetInBlock == 16,
              savedPosition.locator?.quote.exact.hasPrefix("Second sentence.") == true,
              first.positionVersion == 1,
              first.restoredSentenceIndex == 1 else {
            throw FixtureError.positionWasNotSaved
        }
        await first.addBookmark(sentenceIndex: 1)
        guard first.bookmarks.first?.charOffset == 16,
              first.bookmarks.first?.note == "Second sentence." else {
            throw FixtureError.bookmarkWasNotSaved
        }
        let originalBookmark = try unwrap(first.bookmarks.first)
        await first.renameBookmark(originalBookmark, note: "Middle passage")
        guard first.bookmarks.first?.id == originalBookmark.id,
              first.bookmarks.first?.note == "Middle passage",
              first.bookmarks.first?.version == 2 else {
            throw FixtureError.bookmarkWasNotRenamedInPlace
        }

        let restarted = ReaderViewModel(
            libraryService: library,
            positionService: positions,
            bookmarkService: bookmarks,
            sessionID: "reader-b"
        )
        await restarted.loadItem(id: item.id)
        guard restarted.restoredSentenceIndex == 1,
              restarted.positionVersion == 1 else {
            throw FixtureError.positionWasNotRestored
        }
        guard restarted.bookmarks == first.bookmarks else {
            throw FixtureError.bookmarkWasNotRestored
        }
        guard try await library.fetchItem(id: item.id).progress == 33 else {
            throw FixtureError.summaryProgressWasNotUpdated
        }

        let weightedItem = try await library.importFile(
            data: Data("A. 1234567890.".utf8),
            filename: "weighted-progress.txt"
        )
        let weightedReader = ReaderViewModel(
            libraryService: library,
            positionService: positions,
            bookmarkService: bookmarks,
            sessionID: "reader-weighted"
        )
        await weightedReader.loadItem(id: weightedItem.id)
        guard weightedReader.sentences.map(\.charOffset) == [0, 3] else {
            throw FixtureError.scalarOffsetsWrong
        }
        await weightedReader.saveProgress(sentenceIndex: 1)
        guard try await library.fetchItem(id: weightedItem.id).progress == 21 else {
            throw FixtureError.summaryProgressDidNotUseScalarLength
        }

        let syncedDocumentJSON = try ReadioSDK().parseHTML(
            "<pre>First sentence. Second sentence. Third sentence.</pre>"
        )
        let syncedDocument = try JSONSerialization.jsonObject(
            with: Data(syncedDocumentJSON.utf8)
        ) as! [String: Any]
        _ = try await bookmarks.upsert(
            documentJSON: syncedDocumentJSON,
            request: AppleBookmarkUpsertRequest(
                bookmarkId: "remote-bookmark",
                itemId: item.id,
                charOffset: 33,
                affinity: .forward,
                note: "Remote note",
                createdAtUnixMilliseconds: 20,
                updatedAtUnixMilliseconds: 20,
                baseVersion: 0
            )
        )
        let previousDocument = try ReadioSDK().parseHTML(
            "<pre>Preface. First sentence. Second sentence. Third sentence.</pre>"
        )
        let previousJSON = try JSONSerialization.jsonObject(
            with: Data(previousDocument.utf8)
        ) as! [String: Any]
        let previousRevision = (previousJSON["revision"] as! [String: Any])[
            "revision_digest"
        ] as! String
        let previousLocatorJSON = try ReadioSDK().createTextLocator(
            documentJSON: previousDocument,
            requestJSON: "{\"char_offset\":42,\"affinity\":\"forward\"}"
        )
        let previousLocator = try JSONDecoder().decode(
            AppleTextLocator.self,
            from: Data(previousLocatorJSON.utf8)
        )
        _ = try await positions.save(
            itemID: item.id,
            mutation: AppleReadingPositionMutation(
                documentRevision: previousRevision,
                charOffset: 42,
                affinity: .forward,
                observedAtUnixMilliseconds: 20,
                sessionId: "remote-device",
                mutationId: "remote-position",
                baseVersion: 1,
                locator: previousLocator
            )
        )
        await restarted.refreshReplicaProjection()
        guard restarted.bookmarks.map(\.id).contains("remote-bookmark"),
              restarted.restoredSentenceIndex == 2,
              restarted.positionVersion == 2 else {
            throw FixtureError.syncedProjectionWasNotRefreshed
        }
    }
}

enum FixtureError: Error {
    case scalarOffsetsWrong
    case searchDidNotUseRustFoldingAndSourceRanges
    case searchHitWalkingDidNotPreserveTheReaderCursor
    case emptySearchDidNotClearResults
    case positionWasNotSaved
    case positionWasNotRestored
    case bookmarkWasNotSaved
    case bookmarkWasNotRenamedInPlace
    case bookmarkWasNotRestored
    case summaryProgressWasNotUpdated
    case summaryProgressDidNotUseScalarLength
    case syncedProjectionWasNotRefreshed
    case missingFixtureValue
}

private func unwrap<Value>(_ value: Value?) throws -> Value {
    guard let value else { throw FixtureError.missingFixtureValue }
    return value
}
"#;
