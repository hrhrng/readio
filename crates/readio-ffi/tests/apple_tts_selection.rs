#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_player_uses_the_persisted_native_or_cloud_tts_selection() {
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
        env::temp_dir().join(format!("readio-apple-tts-selection-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("AppleTTSSelectionSmoke.swift");
    let executable = fixture_dir.join("apple-tts-selection-smoke");
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
        "Core/AppleCloudContentSync.swift",
        "Core/AppleCloudService.swift",
        "Networking/LibraryService.swift",
        "Networking/SettingsService.swift",
        "Audio/AudioCache.swift",
        "Core/AppleNativeTTS.swift",
        "Core/AppleCloudTTS.swift",
        "Audio/TTSPlaybackEngine.swift",
        "ViewModels/PlayerViewModel.swift",
        "ViewModels/SettingsViewModel.swift",
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
        .arg("Security")
        .arg("-framework")
        .arg("SwiftUI")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify Apple TTS selection");
    assert!(
        compile.status.success(),
        "the Apple player must compile with persisted TTS selection:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Apple TTS selection fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "the player must honor Native/Cloud preferences without network access:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r#"
import Foundation

@MainActor
final class AuthenticationRequiredTTSProvider: ReadioAppleTTSProvider {
    var displayName: String { "Cloud TTS" }

    func voiceCatalog() -> ReadioNativeVoiceCatalog {
        ReadioNativeVoiceCatalog(voices: [], defaultVoiceID: "cloud")
    }

    func render(_ request: ReadioNativeSpeechRequest) async throws -> ReadioNativeSpeechAudio {
        throw AppleCloudError.authenticationRequired
    }

    func cancelAll() {}
}

@MainActor
final class SelectedNativeTTSProvider: ReadioAppleTTSProvider {
    func voiceCatalog() -> ReadioNativeVoiceCatalog {
        ReadioNativeVoiceCatalog(
            voices: [
                ReadioNativeVoice(
                    id: "native-default",
                    name: "On-device",
                    language: "en-US",
                    gender: nil,
                    quality: .default,
                    isPersonal: false
                ),
            ],
            defaultVoiceID: "native-default"
        )
    }

    func render(_ request: ReadioNativeSpeechRequest) async throws -> ReadioNativeSpeechAudio {
        throw FixtureError.unexpectedRender
    }

    func cancelAll() {}
}

@MainActor
final class GenericFailureTTSProvider: ReadioAppleTTSProvider {
    var displayName: String { "On-device TTS" }

    func voiceCatalog() -> ReadioNativeVoiceCatalog {
        ReadioNativeVoiceCatalog(voices: [], defaultVoiceID: "")
    }

    func render(_ request: ReadioNativeSpeechRequest) async throws -> ReadioNativeSpeechAudio {
        throw FixtureError.voiceUnavailable
    }

    func cancelAll() {}
}

@main
enum AppleTTSSelectionSmoke {
    @MainActor
    static func main() async throws {
        let suiteName = "readio-tts-selection-\(UUID().uuidString)"
        guard let defaults = UserDefaults(suiteName: suiteName) else {
            throw FixtureError.defaultsUnavailable
        }
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let offline = AppleTTSPreferences(defaults: defaults)
        guard offline.mode == .native else {
            throw FixtureError.nativeWasNotTheDefault
        }

        let settings = SettingsViewModel(defaults: defaults)
        settings.readingMode = .readAloud
        settings.readingEffort = .low
        settings.baseTextCharactersPerSecond = 40
        let restoredReading = SettingsViewModel(defaults: defaults)
        let pace = try restoredReading.readingPace()
        guard restoredReading.readingMode == .readAloud,
              restoredReading.readingEffort == .low,
              pace.textCharactersPerSecond == 80,
              pace.audioPlaybackRate == 2,
              pace.usesAudio else {
            throw FixtureError.readingPreferencesWereNotShared
        }
        restoredReading.applyPortableSettings([
            "reading_mode": "auto",
            "reading_effort": "medium",
            "reading_base_cps": "50",
        ])
        let syncedPace = try restoredReading.readingPace()
        guard restoredReading.readingMode == .auto,
              restoredReading.readingEffort == .medium,
              syncedPace.textCharactersPerSecond == 75,
              syncedPace.audioPlaybackRate == 1.5 else {
            throw FixtureError.syncedReadingPreferencesWereNotApplied
        }
        let effortPlayer = PlayerViewModel(
            ttsProvider: SelectedNativeTTSProvider(),
            ttsMode: .native,
            defaults: defaults
        )
        effortPlayer.setupEngine(
            itemId: "effort-book",
            sentences: [
                Sentence(index: 0, text: "Shared pace"),
                Sentence(index: 1, text: "Second sentence"),
            ],
            speed: nil,
            voice: nil,
            title: "Effort"
        )
        guard effortPlayer.currentSpeed == 1.5 else {
            throw FixtureError.readingEffortDidNotReachPlayer
        }
        effortPlayer.engine?.play()
        effortPlayer.updateSentences([
            Sentence(index: 0, text: "New chapter"),
            Sentence(index: 1, text: "Keeps advancing"),
        ], startingAt: 0)
        try await Task.sleep(for: .milliseconds(350))
        guard effortPlayer.engine?.currentSentenceIndex == 1,
              effortPlayer.engine?.isPlaying == true,
              effortPlayer.engine?.ttsError == nil else {
            throw FixtureError.chapterReplacementDidNotResumeAuto
        }
        effortPlayer.cleanup()
        settings.ttsMode = .cloud
        settings.cloudVoiceID = "Mandarin_Warm_Narrator"
        let cloud = AppleTTSPreferences(defaults: defaults)
        let player = PlayerViewModel(
            ttsProvider: AppleTTSProviderFactory.make(preferences: cloud),
            ttsMode: cloud.mode
        )
        await player.loadVoices()
        player.setupEngine(
            itemId: "book-1",
            sentences: [Sentence(index: 0, text: "Hello")],
            speed: 1,
            voice: "com.apple.voice.compact.en-US.Samantha",
            title: "Book"
        )

        guard player.ttsMode == .cloud,
              player.defaultVoiceId == "Mandarin_Warm_Narrator",
              player.currentVoice == "Mandarin_Warm_Narrator",
              player.voices.map(\.voiceId) == ["Mandarin_Warm_Narrator"] else {
            throw FixtureError.cloudSelectionWasIgnored
        }
        player.cleanup()

        let nativePlayer = PlayerViewModel(
            ttsProvider: SelectedNativeTTSProvider(),
            ttsMode: .native
        )
        nativePlayer.setupEngine(
            itemId: "book-1",
            sentences: [Sentence(index: 0, text: "Hello")],
            speed: 1,
            voice: "Mandarin_Warm_Narrator",
            title: "Book"
        )
        guard nativePlayer.currentVoice == "native-default" else {
            throw FixtureError.cloudVoiceContaminatedNativeSelection
        }
        nativePlayer.cleanup()

        AppleDiagnosticLog.shared.clear()
        let authEngine = TTSPlaybackEngine(
            itemId: "book-auth",
            sentences: [
                Sentence(index: 0, text: "One"),
                Sentence(index: 1, text: "Two"),
                Sentence(index: 2, text: "Three"),
            ],
            speed: 1,
            voice: "cloud",
            itemTitle: "Private",
            ttsProvider: AuthenticationRequiredTTSProvider()
        )
        authEngine.play()
        for _ in 0..<60 where authEngine.ttsError == nil {
            try await Task.sleep(for: .milliseconds(100))
        }
        guard let message = authEngine.ttsError,
              message.contains("Sign in"),
              message.contains("Cloud TTS"),
              !authEngine.isPlaying else {
            throw FixtureError.authenticationErrorWasHidden
        }
        let cloudFailureEvents = AppleDiagnosticLog.shared.snapshot()
        guard cloudFailureEvents.contains(where: { event in
                  event.category == .fallback
                      && event.fields["from_provider"] == .string("cloud")
                      && event.fields["to_provider"] == .string("none")
                      && event.fields["outcome"] == .string("stopped")
              }) else {
            throw FixtureError.cloudFallbackNamedWrongProvider
        }
        authEngine.cleanup()

        let nativeFailure = TTSPlaybackEngine(
            itemId: "book-native-failure",
            sentences: [Sentence(index: 0, text: "One")],
            speed: 1,
            voice: "missing",
            itemTitle: "Private",
            ttsProvider: GenericFailureTTSProvider()
        )
        nativeFailure.play()
        for _ in 0..<60 where nativeFailure.ttsError == nil {
            try await Task.sleep(for: .milliseconds(100))
        }
        guard let nativeMessage = nativeFailure.ttsError,
              nativeMessage.contains("On-device TTS"),
              nativeMessage.contains("voiceUnavailable"),
              nativeMessage.contains("choose another voice"),
              !nativeFailure.isPlaying else {
            throw FixtureError.genericProviderErrorWasHidden
        }
        nativeFailure.cleanup()
    }
}

enum FixtureError: Error {
    case defaultsUnavailable
    case nativeWasNotTheDefault
    case readingPreferencesWereNotShared
    case syncedReadingPreferencesWereNotApplied
    case readingEffortDidNotReachPlayer
    case chapterReplacementDidNotResumeAuto
    case cloudSelectionWasIgnored
    case cloudVoiceContaminatedNativeSelection
    case authenticationErrorWasHidden
    case cloudFallbackNamedWrongProvider
    case genericProviderErrorWasHidden
    case voiceUnavailable
    case unexpectedRender
}
"#;
