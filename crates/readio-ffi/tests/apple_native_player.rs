#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_player_consumes_the_native_sdk_voice_catalog_without_http() {
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
        env::temp_dir().join(format!("readio-apple-native-player-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("NativePlayerSmoke.swift");
    let executable = fixture_dir.join("native-player-smoke");
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
        "Models/VoiceInfo.swift",
        "Networking/APIEndpoints.swift",
        "Networking/APIClient.swift",
        "Core/ReadioSDK.swift",
        "Core/AppleDiagnostics.swift",
        "Core/ReadioPlaybackExecutor.swift",
        "Core/ReadioTextAdvanceExecutor.swift",
        "Core/AppleReplicaSync.swift",
        "Core/AppleLocalReplicaStore.swift",
        "Networking/LibraryService.swift",
        "Audio/AudioCache.swift",
        "Core/AppleNativeTTS.swift",
        "Audio/TTSPlaybackEngine.swift",
        "ViewModels/PlayerViewModel.swift",
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
        .arg("AVFoundation")
        .arg("-framework")
        .arg("MediaPlayer")
        .arg("-framework")
        .arg("SwiftUI")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify native Apple player wiring");
    assert!(
        compile.status.success(),
        "the Apple player must accept the SDK TTS provider boundary:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the native player fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "the player must project native voice metadata without HTTP:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r#"
import Foundation

@MainActor
final class FixtureNativeTTSProvider: ReadioAppleTTSProvider {
    func voiceCatalog() -> ReadioNativeVoiceCatalog {
        ReadioNativeVoiceCatalog(
            voices: [
                ReadioNativeVoice(
                    id: "native-en",
                    name: "English Premium",
                    language: "en-US",
                    gender: "female",
                    quality: .premium,
                    isPersonal: false
                ),
                ReadioNativeVoice(
                    id: "native-zh",
                    name: "中文个人声音",
                    language: "zh-CN",
                    gender: nil,
                    quality: .enhanced,
                    isPersonal: true
                ),
            ],
            defaultVoiceID: "native-zh"
        )
    }

    func render(_ request: ReadioNativeSpeechRequest) async throws -> ReadioNativeSpeechAudio {
        throw FixtureError.unexpectedRender
    }

    func cancelAll() {}
}

@main
enum NativePlayerSmoke {
    @MainActor
    static func main() async throws {
        let viewModel = PlayerViewModel(ttsProvider: FixtureNativeTTSProvider())
        await viewModel.loadVoices()

        guard viewModel.defaultVoiceId == "native-zh",
              viewModel.currentVoice == "native-zh",
              viewModel.voices.map(\.language) == ["en", "zh"],
              viewModel.voices[0].description == "Premium",
              viewModel.voices[1].description == "Enhanced · Personal Voice" else {
            throw FixtureError.invalidCatalogProjection
        }

        let suiteName = "readio-live-reading-preferences-\(UUID().uuidString)"
        guard let defaults = UserDefaults(suiteName: suiteName) else {
            throw FixtureError.unavailableDefaults
        }
        defer { defaults.removePersistentDomain(forName: suiteName) }
        defaults.set("manual", forKey: AppSettingsKeys.readingMode)
        defaults.set("high", forKey: AppSettingsKeys.readingEffort)
        defaults.set(
            40.0,
            forKey: AppSettingsKeys.readingBaseCharactersPerSecond
        )

        let live = PlayerViewModel(
            ttsProvider: FixtureNativeTTSProvider(),
            defaults: defaults
        )
        let liveReader = try ReadioSDK().parsePlainTextReader("One. Two.")
        var livePersists: [Int] = []
        live.setupEngine(
            itemId: "book-live-preferences",
            sentences: [
                Sentence(index: 0, text: "One"),
                Sentence(index: 1, text: "Two"),
            ],
            speed: nil,
            voice: nil,
            title: "Live preferences",
            highlightDocument: try ReadioSDK().makeHighlightDocument(
                reader: liveReader
            ),
            onPersistProgress: { sentence in
                livePersists.append(sentence)
            }
        )
        live.engine?.seekToSentence(1)
        live.setReadingMode(.auto)
        live.setReadingEffort(.low)

        guard live.readingMode == .auto,
              live.readingEffort == .low,
              live.currentSpeed == 2,
              live.engine?.readingMode == .auto,
              live.engine?.currentSentenceIndex == 1,
              live.engine?.isPlaying == true,
              defaults.string(forKey: AppSettingsKeys.readingMode) == "auto",
              defaults.string(forKey: AppSettingsKeys.readingEffort) == "low" else {
            throw FixtureError.liveReadingPreferencesWereNotApplied
        }

        let persistsBeforeEquivalentRefresh = livePersists
        live.refreshReadingPreferences()
        guard livePersists == persistsBeforeEquivalentRefresh else {
            throw FixtureError.equivalentPreferencesMovedTheCursorAgain
        }

        defaults.set("manual", forKey: AppSettingsKeys.readingMode)
        defaults.set("max", forKey: AppSettingsKeys.readingEffort)
        live.refreshReadingPreferences()
        guard live.readingMode == .manual,
              live.readingEffort == .max,
              live.currentSpeed == 0.7,
              live.engine?.readingMode == .manual,
              live.engine?.currentSentenceIndex == 1,
              live.engine?.isPlaying == false else {
            throw FixtureError.externalReadingPreferencesWereNotApplied
        }

        live.updateSentences(
            [
                Sentence(index: 0, text: "Chapter heading"),
                Sentence(index: 1, text: "Fragment target"),
                Sentence(index: 2, text: "Chapter ending"),
            ],
            startingAt: 1
        )
        guard live.engine?.currentSentenceIndex == 1 else {
            throw FixtureError.chapterFragmentTargetWasReset
        }
        live.cleanup()
    }
}

enum FixtureError: Error {
    case unexpectedRender
    case invalidCatalogProjection
    case unavailableDefaults
    case liveReadingPreferencesWereNotApplied
    case equivalentPreferencesMovedTheCursorAgain
    case externalReadingPreferencesWereNotApplied
    case chapterFragmentTargetWasReset
}
"#;
