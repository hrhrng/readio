#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn swift_bridge_calls_the_real_rust_core() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("the FFI crate lives under the repository crates directory");
    let wrapper = repo_root.join("apps/ios/Readio/Readio/Core/ReadioSDK.swift");
    let include_dir = manifest_dir.join("include");
    let deps_dir = env::current_exe()
        .expect("the test executable has a path")
        .parent()
        .expect("the test executable lives in Cargo's deps directory")
        .to_owned();
    let static_library = deps_dir.join("libreadio_ffi.a");
    let fixture_dir = env::temp_dir().join(format!("readio-swift-bridge-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("BridgeSmoke.swift");
    let executable = fixture_dir.join("bridge-smoke");
    fs::write(
        &fixture,
        r#"
import Foundation

@main
struct BridgeSmoke {
    static func main() throws {
        let bridge = ReadioSDK()
        let sample = try bridge.builtinSample(languageTag: "zh-Hans-CN")
        guard sample.itemID == "readio:sample:v1",
              sample.legacyItemIDs == ["6b4121313cc4a35d"],
              sample.locale == "zh",
              sample.source == "readio://sample",
              sample.contentFormat == "plaintext",
              sample.isBuiltinSample,
              !sample.content.isEmpty,
              !sample.markdownSource.isEmpty else {
            throw SmokeError.unexpectedBuiltinSample
        }
        let state = try bridge.createPlaybackState(sentenceCount: 2)
        let stateJSON = try JSONSerialization.jsonObject(with: Data(state.utf8)) as! [String: Any]
        guard stateJSON["sentence_count"] as? Int == 2 else {
            throw SmokeError.unexpectedState
        }

        let pace = try bridge.resolveReadingPace(
            preferencesJSON: "{\"mode\":\"read_aloud\",\"base_text_characters_per_second\":40,\"effort\":\"low\"}"
        )
        let paceJSON = try JSONSerialization.jsonObject(
            with: Data(pace.utf8)
        ) as! [String: Any]
        guard paceJSON["mode"] as? String == "read_aloud",
              paceJSON["text_characters_per_second"] as? Double == 80,
              paceJSON["audio_playback_rate"] as? Double == 2,
              paceJSON["uses_audio"] as? Bool == true else {
            throw SmokeError.unexpectedReadingPace
        }
        let typedPace = try bridge.resolveReadingPace(
            ReadioReadingPreferences(
                mode: .auto,
                baseTextCharactersPerSecond: 40,
                effort: .medium
            )
        )
        guard typedPace.mode == .auto,
              typedPace.effort == .medium,
              typedPace.textCharactersPerSecond == 60,
              typedPace.audioPlaybackRate == 1.5,
              typedPace.advancesAutomatically,
              !typedPace.usesAudio else {
            throw SmokeError.unexpectedTypedReadingPace
        }

        let readingProgress = try bridge.projectReadingProgress(
            ReadioReadingProgressRequest(
                segmentCharacterCounts: [2, 8],
                currentSegment: 1,
                currentSegmentProgressBasisPoints: 5_000,
                timing: .unknown
            )
        )
        guard readingProgress.charOffset == 7,
              readingProgress.charCount == 11,
              readingProgress.progressBasisPoints == 6_363,
              readingProgress.remainingCharacters == 4,
              readingProgress.remainingTime.kind == .unknown,
              readingProgress.remainingTime.seconds == nil else {
            throw SmokeError.unexpectedReadingProgress
        }
        let readingSeek = try bridge.resolveReadingSeek(
            segmentCharacterCounts: [1, 9],
            progressBasisPoints: 5_000
        )
        guard readingSeek.segment == 1,
              readingSeek.segmentProgressBasisPoints == 3_333,
              readingSeek.charOffset == 5 else {
            throw SmokeError.unexpectedReadingSeek
        }
        let audioProgress = try bridge.projectReadingProgress(
            ReadioReadingProgressRequest(
                segmentCharacterCounts: [100, 200, 100],
                currentSegment: 1,
                currentSegmentProgressBasisPoints: 0,
                timing: .observedAudio(
                    playbackRate: 2,
                    currentPositionMilliseconds: 500,
                    durations: [
                        ReadioAudioDurationObservation(
                            segment: 0,
                            durationMilliseconds: 1_000
                        ),
                        ReadioAudioDurationObservation(
                            segment: 1,
                            durationMilliseconds: 2_000
                        ),
                        ReadioAudioDurationObservation(
                            segment: 2,
                            durationMilliseconds: 1_000
                        ),
                    ]
                )
            )
        )
        guard audioProgress.charOffset == 151,
              audioProgress.remainingTime.kind == .observedAudio,
              audioProgress.remainingTime.seconds == 2,
              audioProgress.remainingTime.sampleCount == 3 else {
            throw SmokeError.unexpectedAudioProgress
        }

        let document = try bridge.parseHTML("<h1>Hello</h1><p>Shared core</p>")
        let documentJSON = try JSONSerialization.jsonObject(with: Data(document.utf8)) as! [String: Any]
        guard documentJSON["ir_version"] as? Int == 1 else {
            throw SmokeError.unexpectedDocument
        }

        let htmlReader = try bridge.parseHTMLReader(
            "<h1>Reader title</h1><p>First sentence. Second sentence!</p><pre>skip()</pre>"
        )
        guard htmlReader.irVersion == 1,
              htmlReader.document.blocks.map(\.text) == [
                  "Reader title",
                  "First sentence. Second sentence!",
                  "skip()",
              ],
              htmlReader.textIndex.sentences.map(\.text) == [
                  "Reader title",
                  "First sentence.",
                  "Second sentence!",
              ],
              htmlReader.chapters.map(\.title) == ["Reader title"],
              htmlReader.navigation.map(\.title) == ["Reader title"],
              htmlReader.document.revision.revisionDigest
                == htmlReader.textIndex.documentRevision else {
            throw SmokeError.unexpectedReaderIR
        }

        let plainTextReader = try bridge.parsePlainTextReader(
            "First line\ncontinues.\n\nThird sentence?"
        )
        guard plainTextReader.document.blocks.map(\.text) == [
                  "First line continues.",
                  "Third sentence?",
              ],
              plainTextReader.textIndex.sentences.map(\.text) == [
                  "First line continues.",
                  "Third sentence?",
              ] else {
            throw SmokeError.unexpectedReaderIR
        }

        let projection = try bridge.projectOffset(
            documentJSON: document,
            requestJSON: "{\"char_offset\":2,\"affinity\":\"forward\"}"
        )
        let projectionJSON = try JSONSerialization.jsonObject(
            with: Data(projection.utf8)
        ) as! [String: Any]
        guard projectionJSON["char_offset"] as? Int == 2,
              projectionJSON["offset_in_block"] as? Int == 2 else {
            throw SmokeError.unexpectedProjection
        }

        let positionDocument = try bridge.parseHTML("<p>A🙂中</p><p>B</p>")
        let positionDocumentJSON = try JSONSerialization.jsonObject(
            with: Data(positionDocument.utf8)
        ) as! [String: Any]
        let blocks = positionDocumentJSON["blocks"] as! [[String: Any]]
        let secondBlockID = blocks[1]["id"] as! String
        let resolved = try bridge.resolveBlockOffset(
            documentJSON: positionDocument,
            requestJSON: "{\"block_id\":\"\(secondBlockID)\",\"offset_in_block\":0,\"affinity\":\"forward\"}"
        )
        let resolvedJSON = try JSONSerialization.jsonObject(
            with: Data(resolved.utf8)
        ) as! [String: Any]
        guard resolvedJSON["char_offset"] as? Int == 4 else {
            throw SmokeError.unexpectedResolvedAnchor
        }

        let restored = try bridge.restorePosition(
            documentJSON: positionDocument,
            positionJSON: "{\"item_id\":\"book-1\",\"document_revision\":\"stale\",\"char_offset\":999,\"affinity\":\"backward\",\"observed_at_unix_ms\":10,\"session_id\":\"s\",\"mutation_id\":\"m\",\"base_version\":1}"
        )
        let restoredJSON = try JSONSerialization.jsonObject(
            with: Data(restored.utf8)
        ) as! [String: Any]
        guard restoredJSON["char_offset"] as? Int == 5,
              restoredJSON["revision_changed"] as? Bool == true else {
            throw SmokeError.unexpectedRestored
        }

        let locator = try bridge.createTextLocator(
            documentJSON: positionDocument,
            requestJSON: "{\"char_offset\":4,\"affinity\":\"forward\"}"
        )
        let locatorRestore = try bridge.restoreTextLocator(
            documentJSON: positionDocument,
            locatorJSON: locator
        )
        let locatorRestoreJSON = try JSONSerialization.jsonObject(
            with: Data(locatorRestore.utf8)
        ) as! [String: Any]
        guard locatorRestoreJSON["kind"] as? String == "exact_revision" else {
            throw SmokeError.unexpectedLocator
        }

        let textIndex = try bridge.indexDocumentText(documentJSON: positionDocument)
        let positionRevision = (positionDocumentJSON["revision"] as! [String: Any])["revision_digest"] as! String
        let highlight = try bridge.resolveHighlight(
            documentJSON: positionDocument,
            indexJSON: textIndex,
            cursorJSON: "{\"document_revision\":\"\(positionRevision)\",\"sentence_index\":1,\"word_index\":0}"
        )
        let highlightJSON = try JSONSerialization.jsonObject(
            with: Data(highlight.utf8)
        ) as! [String: Any]
        guard highlightJSON["sentence_range"] as? [Int] == [0, 1] else {
            throw SmokeError.unexpectedHighlight
        }

        let highlightReader = try bridge.parsePlainTextReader("🙂 hello, 世界!")
        let typedHighlightDocument = try bridge.makeHighlightDocument(
            reader: highlightReader
        )
        let typedHighlight = try bridge.resolveHighlight(
            document: typedHighlightDocument,
            cursor: ReadioHighlightCursor(
                documentRevision: typedHighlightDocument.documentRevision,
                sentenceIndex: 0,
                wordIndex: nil,
                wordProgressBasisPoints: 3_400
            )
        )
        guard typedHighlight.sentenceIndex == 0,
              typedHighlight.sentenceRange == ReadioScalarRange(start: 0, end: 12),
              typedHighlight.wordRange == ReadioScalarRange(start: 9, end: 10) else {
            throw SmokeError.unexpectedTypedHighlight
        }

        let searchDocument = try bridge.parseHTML("<p>ＣＩＴＹ and city</p>")
        let searchDocumentJSON = try JSONSerialization.jsonObject(
            with: Data(searchDocument.utf8)
        ) as! [String: Any]
        let revision = (searchDocumentJSON["revision"] as! [String: Any])["revision_digest"] as! String
        let search = try bridge.searchDocument(
            documentJSON: searchDocument,
            requestJSON: "{\"document_revision\":\"\(revision)\",\"query\":\"city\",\"cursor\":null,\"limit\":10}"
        )
        let searchJSON = try JSONSerialization.jsonObject(
            with: Data(search.utf8)
        ) as! [String: Any]
        guard searchJSON["total_occurrences"] as? Int == 2 else {
            throw SmokeError.unexpectedSearch
        }

        let bookmark = try bridge.upsertBookmark(
            documentJSON: positionDocument,
            bookmarksJSON: "[]",
            requestJSON: "{\"bookmark_id\":\"bookmark-1\",\"item_id\":\"book-1\",\"char_offset\":2,\"affinity\":\"forward\",\"note\":\"saved\",\"created_at_unix_ms\":10,\"updated_at_unix_ms\":10,\"base_version\":0}"
        )
        let bookmarkJSON = try JSONSerialization.jsonObject(
            with: Data(bookmark.utf8)
        ) as! [String: Any]
        guard bookmarkJSON["created"] as? Bool == true else {
            throw SmokeError.unexpectedBookmark
        }

        var syncState = bridge.makeSyncState()
        try syncState.stageLocal(
            entitiesJSON: "[{\"entityKind\":\"library_item\",\"entityId\":\"book-1\",\"payload\":{\"id\":\"book-1\",\"title\":\"Native Local\",\"type\":\"txt\",\"progress\":0,\"date\":\"2026-01-01\",\"category\":\"imported\",\"folder_id\":\"f1\",\"source\":\"local:book.txt\",\"file_path\":\"local:book-1\",\"cover_image\":null,\"voice\":null,\"speed\":null,\"current_chapter\":null,\"content_format\":\"plaintext\"}}]"
        )
        let page = try syncState.queryLibrary(queryJSON: "{\"page\":1,\"page_size\":20}")
        let pageJSON = try JSONSerialization.jsonObject(with: Data(page.utf8)) as! [String: Any]
        let pageItems = pageJSON["items"] as! [[String: Any]]
        guard pageItems.first?["title"] as? String == "Native Local" else {
            throw SmokeError.unexpectedLocalLibrary
        }
        let positionTransition = try syncState.saveReadingPosition(
            itemID: "book-1",
            mutationJSON: "{\"document_revision\":\"revision-1\",\"char_offset\":42,\"affinity\":\"forward\",\"observed_at_unix_ms\":1700000000000,\"session_id\":\"apple-a\",\"mutation_id\":\"mutation-a\",\"base_version\":0}"
        )
        let positionTransitionJSON = try JSONSerialization.jsonObject(
            with: Data(positionTransition.utf8)
        ) as! [String: Any]
        guard positionTransitionJSON["version"] as? Int == 1 else {
            throw SmokeError.unexpectedLocalPosition
        }

        let replica = bridge.makeReplica()
        _ = try replica.upsert(
            kind: .setting,
            entityID: "theme",
            payloadJSON: "{\"value\":\"dark\"}"
        )
        guard try !replica.headsBase64().isEmpty else {
            throw SmokeError.unexpectedReplicaHeads
        }
    }
}

enum SmokeError: Error {
    case unexpectedBuiltinSample
    case unexpectedState
    case unexpectedReadingPace
    case unexpectedTypedReadingPace
    case unexpectedReadingProgress
    case unexpectedReadingSeek
    case unexpectedAudioProgress
    case unexpectedDocument
    case unexpectedReaderIR
    case unexpectedProjection
    case unexpectedResolvedAnchor
    case unexpectedRestored
    case unexpectedLocator
    case unexpectedHighlight
    case unexpectedTypedHighlight
    case unexpectedSearch
    case unexpectedBookmark
    case unexpectedLocalLibrary
    case unexpectedLocalPosition
    case unexpectedReplicaHeads
}
"#,
    )
    .expect("the Swift fixture can be written");

    let compile = Command::new("swiftc")
        .arg("-I")
        .arg(&include_dir)
        .arg(&wrapper)
        .arg(&fixture)
        .arg(&static_library)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify the Apple bridge");
    assert!(
        compile.status.success(),
        "the Swift bridge must compile against the Rust static library:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Swift bridge fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "the Swift bridge must return Rust core results:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}
