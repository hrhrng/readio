#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_engine_rejects_audio_that_finishes_after_pause() {
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
    let fixture_dir = env::temp_dir().join(format!(
        "readio-apple-playback-engine-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("ApplePlaybackEngineSmoke.swift");
    let executable = fixture_dir.join("apple-playback-engine-smoke");
    fs::write(&fixture, SWIFT_FIXTURE).expect("the Swift fixture can be written");

    let sources = [
        "Models/Sentence.swift",
        "Audio/AudioCache.swift",
        "Core/ReadioSDK.swift",
        "Core/AppleDiagnostics.swift",
        "Core/ReadioPlaybackExecutor.swift",
        "Core/ReadioTextAdvanceExecutor.swift",
        "Core/AppleNativeTTS.swift",
        "Audio/TTSPlaybackEngine.swift",
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
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify Apple playback ownership");
    assert!(
        compile.status.success(),
        "the Apple playback engine must compile with the Rust executor:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the Apple playback engine fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "a paused generation must reject late audio:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r#"
import Foundation

@MainActor
final class SuspendedTTSProvider: ReadioAppleTTSProvider {
    private var continuations: [String: CheckedContinuation<ReadioNativeSpeechAudio, Error>] = [:]
    private(set) var renderCount = 0

    func voiceCatalog() -> ReadioNativeVoiceCatalog {
        ReadioNativeVoiceCatalog(voices: [], defaultVoiceID: "fixture")
    }

    func render(_ request: ReadioNativeSpeechRequest) async throws -> ReadioNativeSpeechAudio {
        renderCount += 1
        return try await withCheckedThrowingContinuation { continuation in
            continuations[request.text] = continuation
        }
    }

    func hasPendingRender(for text: String) -> Bool {
        continuations[text] != nil
    }

    func completePendingRender(for text: String) {
        continuations.removeValue(forKey: text)?.resume(
            returning: ReadioNativeSpeechAudio(
                data: Data([0]),
                durationMilliseconds: 1_000,
                sampleRate: 44_100
            )
        )
    }

    func cancelAll() {
        let pending = continuations.values
        continuations.removeAll()
        for continuation in pending {
            continuation.resume(throwing: CancellationError())
        }
    }
}

@MainActor
final class RejectingTTSProvider: ReadioAppleTTSProvider {
    private(set) var renderCount = 0

    func voiceCatalog() -> ReadioNativeVoiceCatalog {
        ReadioNativeVoiceCatalog(voices: [], defaultVoiceID: "fixture")
    }

    func render(_ request: ReadioNativeSpeechRequest) async throws -> ReadioNativeSpeechAudio {
        renderCount += 1
        throw FixtureError.unexpectedAudioRender
    }

    func cancelAll() {}
}

@MainActor
final class MisleadingDurationTTSProvider: ReadioAppleTTSProvider {
    func voiceCatalog() -> ReadioNativeVoiceCatalog {
        ReadioNativeVoiceCatalog(voices: [], defaultVoiceID: "fixture")
    }

    func render(_ request: ReadioNativeSpeechRequest) async throws -> ReadioNativeSpeechAudio {
        ReadioNativeSpeechAudio(
            data: silentWave(durationMilliseconds: 2_000),
            durationMilliseconds: 600_000,
            sampleRate: 8_000
        )
    }

    func cancelAll() {}
}

func appendLittleEndian<T: FixedWidthInteger>(_ value: T, to data: inout Data) {
    var littleEndian = value.littleEndian
    withUnsafeBytes(of: &littleEndian) { bytes in
        data.append(contentsOf: bytes)
    }
}

func silentWave(durationMilliseconds: Int) -> Data {
    let sampleRate: UInt32 = 8_000
    let channelCount: UInt16 = 1
    let bitsPerSample: UInt16 = 16
    let sampleCount = UInt32(durationMilliseconds) * sampleRate / 1_000
    let dataSize = sampleCount * UInt32(bitsPerSample / 8)
    var data = Data("RIFF".utf8)
    appendLittleEndian(UInt32(36) + dataSize, to: &data)
    data.append(Data("WAVEfmt ".utf8))
    appendLittleEndian(UInt32(16), to: &data)
    appendLittleEndian(UInt16(1), to: &data)
    appendLittleEndian(channelCount, to: &data)
    appendLittleEndian(sampleRate, to: &data)
    appendLittleEndian(sampleRate * UInt32(channelCount) * UInt32(bitsPerSample / 8), to: &data)
    appendLittleEndian(channelCount * bitsPerSample / 8, to: &data)
    appendLittleEndian(bitsPerSample, to: &data)
    data.append(Data("data".utf8))
    appendLittleEndian(dataSize, to: &data)
    data.append(Data(repeating: 0, count: Int(dataSize)))
    return data
}

@main
enum ApplePlaybackEngineSmoke {
    @MainActor
    static func main() async throws {
        let provider = SuspendedTTSProvider()
        let engine = TTSPlaybackEngine(
            itemId: "book-1",
            sentences: [
                Sentence(index: 0, text: "One"),
                Sentence(index: 1, text: "Two"),
            ],
            speed: 1,
            voice: "fixture",
            itemTitle: "Fixture",
            ttsProvider: provider
        )

        engine.play()
        for _ in 0..<100 where !provider.hasPendingRender(for: "One") {
            await Task.yield()
        }
        guard provider.hasPendingRender(for: "One") else {
            throw FixtureError.renderDidNotStart
        }

        engine.pause()
        provider.completePendingRender(for: "One")
        try await Task.sleep(for: .milliseconds(250))

        guard !engine.isPlaying,
              !engine.isLoading,
              engine.currentSentenceIndex == 0,
              engine.ttsError == nil else {
            throw FixtureError.lateAudioWasConsumed
        }
        engine.cleanup()

        let chapterProvider = SuspendedTTSProvider()
        var replacementPersists: [Int] = []
        let chapterEngine = TTSPlaybackEngine(
            itemId: "book-chapters",
            sentences: [
                Sentence(index: 0, text: "Old chapter"),
                Sentence(index: 1, text: "Old ending"),
            ],
            speed: 1,
            voice: "fixture",
            itemTitle: "Chapters",
            ttsProvider: chapterProvider,
            onPersistProgress: { sentence in
                replacementPersists.append(sentence)
            }
        )
        chapterEngine.play()
        for _ in 0..<100 where !chapterProvider.hasPendingRender(for: "Old chapter") {
            await Task.yield()
        }
        guard chapterProvider.hasPendingRender(for: "Old chapter") else {
            throw FixtureError.renderDidNotStart
        }
        chapterEngine.updateSentences([
            Sentence(index: 0, text: "New chapter"),
            Sentence(index: 1, text: "New ending"),
        ])
        guard replacementPersists.isEmpty else {
            throw FixtureError.oldChapterProgressWasPersistedIntoTheNewDocument
        }
        chapterEngine.setSleepDeadline(unixMilliseconds: 50_000)
        guard chapterEngine.sleepDeadlineUnixMilliseconds == 50_000 else {
            throw FixtureError.sleepDeadlineWasNotProjected
        }
        chapterEngine.clearSleepDeadline()
        guard chapterEngine.sleepDeadlineUnixMilliseconds == nil else {
            throw FixtureError.sleepDeadlineWasNotProjected
        }
        chapterEngine.cleanup()

        let interruptionProvider = SuspendedTTSProvider()
        var interruptionPersists: [Int] = []
        let interruptionEngine = TTSPlaybackEngine(
            itemId: "book-interruption",
            sentences: [Sentence(index: 0, text: "Interrupted")],
            speed: 1,
            voice: "fixture",
            itemTitle: "Interruption",
            ttsProvider: interruptionProvider,
            onPersistProgress: { sentence in
                interruptionPersists.append(sentence)
            }
        )
        interruptionEngine.play()
        for _ in 0..<100 where !interruptionProvider.hasPendingRender(for: "Interrupted") {
            await Task.yield()
        }
        interruptionEngine.audioInterruptionBegan()
        guard !interruptionEngine.isPlaying,
              !interruptionEngine.isLoading,
              interruptionPersists == [0] else {
            throw FixtureError.interruptionDidNotPauseThroughRust
        }
        interruptionEngine.audioInterruptionEnded(shouldResume: true)
        guard interruptionEngine.isLoading else {
            throw FixtureError.resumableInterruptionLostPlaybackIntent
        }
        interruptionEngine.audioRouteBecameUnavailable()
        guard !interruptionEngine.isPlaying,
              !interruptionEngine.isLoading else {
            throw FixtureError.routeLossDidNotPauseThroughRust
        }
        interruptionEngine.cleanup()

        let rejectingProvider = RejectingTTSProvider()
        var persistedSentences: [Int] = []
        let autoEngine = TTSPlaybackEngine(
            itemId: "book-auto",
            sentences: [
                Sentence(index: 0, text: "A"),
                Sentence(index: 1, text: "B"),
            ],
            speed: 1,
            voice: nil,
            itemTitle: "Auto",
            ttsProvider: rejectingProvider,
            readingMode: .auto,
            textCharactersPerSecond: 4_000,
            onPersistProgress: { sentence in
                persistedSentences.append(sentence)
            }
        )
        autoEngine.play()
        try await Task.sleep(for: .milliseconds(350))
        guard autoEngine.currentSentenceIndex == 1,
              rejectingProvider.renderCount == 0,
              persistedSentences == [1] else {
            throw FixtureError.autoReadingUsedAudioPolicy
        }
        autoEngine.cleanup()

        let observedAudioEngine = TTSPlaybackEngine(
            itemId: "book-observed-audio",
            sentences: [
                Sentence(index: 0, text: "First"),
                Sentence(index: 1, text: "Next!"),
            ],
            speed: 1,
            voice: nil,
            itemTitle: "Observed audio",
            ttsProvider: MisleadingDurationTTSProvider(),
            readingMode: .readAloud
        )
        observedAudioEngine.play()
        for _ in 0..<200 where observedAudioEngine.estimatedRemainingSeconds < 0 {
            try await Task.sleep(for: .milliseconds(5))
        }
        guard observedAudioEngine.isPlaying,
              observedAudioEngine.estimatedRemainingSeconds > 0,
              observedAudioEngine.estimatedRemainingSeconds < 10 else {
            throw FixtureError.providerDurationWasAcceptedAsETAEvidence
        }
        observedAudioEngine.cleanup()

        let highlightReader = try ReadioSDK().parsePlainTextReader(
            "Ignored. 🙂 hello, 世界!"
        )
        let highlightDocument = try ReadioSDK().makeHighlightDocument(
            reader: highlightReader
        )
        let highlightEngine = TTSPlaybackEngine(
            itemId: "book-rust-highlight",
            sentences: [
                Sentence(
                    index: 0,
                    text: "🙂 hello, 世界!",
                    charOffset: 9,
                    readerIndex: 1
                ),
            ],
            speed: 1,
            voice: nil,
            itemTitle: "Rust highlight",
            highlightDocument: highlightDocument,
            ttsProvider: MisleadingDurationTTSProvider(),
            readingMode: .readAloud
        )
        highlightEngine.play()
        for _ in 0..<200 where !highlightEngine.isPlaying {
            try await Task.sleep(for: .milliseconds(5))
        }
        highlightEngine.pause()
        // Scalar seek rounds to the nearest character. 42% resolves to 5/12
        // (4166 bp), where Rust selects `世` but the legacy 4-token Swift
        // tokenizer would still select `hello,`.
        highlightEngine.seekToProgress(0.42)
        guard highlightEngine.currentWordRange == ReadioScalarRange(start: 9, end: 10) else {
            throw FixtureError.highlightWasNotResolvedByRust
        }
        highlightEngine.cleanup()

        let scalarSeekEngine = TTSPlaybackEngine(
            itemId: "book-scalar-seek",
            sentences: [
                Sentence(index: 0, text: String(repeating: "A", count: 100)),
                Sentence(index: 1, text: "B"),
                Sentence(index: 2, text: "C"),
            ],
            speed: 1,
            voice: nil,
            itemTitle: "Scalar seek",
            ttsProvider: rejectingProvider,
            readingMode: .manual,
            textCharactersPerSecond: 40
        )
        scalarSeekEngine.seekToProgress(0.5)
        guard scalarSeekEngine.currentSentenceIndex == 0,
              scalarSeekEngine.progressBasisPoints == 5_000,
              scalarSeekEngine.estimatedRemainingSeconds == -1 else {
            throw FixtureError.scalarProgressWasNotProjectedByRust
        }
        scalarSeekEngine.cleanup()

        let autoProjectionEngine = TTSPlaybackEngine(
            itemId: "book-auto-projection",
            sentences: [
                Sentence(index: 0, text: "A"),
                Sentence(index: 1, text: "123456789"),
            ],
            speed: 1,
            voice: nil,
            itemTitle: "Auto projection",
            ttsProvider: rejectingProvider,
            readingMode: .auto,
            textCharactersPerSecond: 2
        )
        guard autoProjectionEngine.progressBasisPoints == 0,
              autoProjectionEngine.estimatedRemainingSeconds == 6 else {
            throw FixtureError.autoETAWasNotProjectedByRust
        }
        autoProjectionEngine.cleanup()

        let modeProvider = SuspendedTTSProvider()
        let modeEngine = TTSPlaybackEngine(
            itemId: "book-live-mode",
            sentences: [
                Sentence(index: 0, text: "First"),
                Sentence(index: 1, text: "Second"),
                Sentence(index: 2, text: "Third"),
            ],
            speed: 1,
            voice: "fixture",
            itemTitle: "Live mode",
            ttsProvider: modeProvider,
            readingMode: .manual,
            textCharactersPerSecond: 4_000
        )
        modeEngine.seekToSentence(1)
        modeEngine.updateReadingPace(ReadioReadingPace(
            mode: .auto,
            effort: .high,
            multiplier: 1,
            textCharactersPerSecond: 4_000,
            audioPlaybackRate: 1,
            advancesAutomatically: true,
            usesAudio: false
        ))
        guard modeEngine.readingMode == .auto,
              modeEngine.currentSentenceIndex == 1,
              modeEngine.isPlaying else {
            throw FixtureError.manualToAutoDidNotResumeAtTheVisibleSentence
        }

        modeEngine.updateReadingPace(ReadioReadingPace(
            mode: .readAloud,
            effort: .low,
            multiplier: 2,
            textCharactersPerSecond: 80,
            audioPlaybackRate: 2,
            advancesAutomatically: true,
            usesAudio: true
        ))
        for _ in 0..<100 where !modeProvider.hasPendingRender(for: "Second") {
            await Task.yield()
        }
        guard modeEngine.readingMode == .readAloud,
              modeEngine.currentSentenceIndex == 1,
              modeEngine.isLoading,
              modeProvider.hasPendingRender(for: "Second") else {
            throw FixtureError.autoToAudioDidNotTransferTheActiveCursor
        }

        modeEngine.updateReadingPace(ReadioReadingPace(
            mode: .manual,
            effort: .low,
            multiplier: 2,
            textCharactersPerSecond: 80,
            audioPlaybackRate: 2,
            advancesAutomatically: false,
            usesAudio: false
        ))
        modeProvider.completePendingRender(for: "Second")
        try await Task.sleep(for: .milliseconds(250))
        guard modeEngine.readingMode == .manual,
              modeEngine.currentSentenceIndex == 1,
              !modeEngine.isPlaying,
              !modeEngine.isLoading,
              modeEngine.ttsError == nil else {
            throw FixtureError.lateAudioTookOverAfterModeTransfer
        }
        modeEngine.cleanup()
    }
}

enum FixtureError: Error {
    case renderDidNotStart
    case lateAudioWasConsumed
    case unexpectedAudioRender
    case autoReadingUsedAudioPolicy
    case scalarProgressWasNotProjectedByRust
    case autoETAWasNotProjectedByRust
    case providerDurationWasAcceptedAsETAEvidence
    case highlightWasNotResolvedByRust
    case oldChapterProgressWasPersistedIntoTheNewDocument
    case interruptionDidNotPauseThroughRust
    case resumableInterruptionLostPlaybackIntent
    case routeLossDidNotPauseThroughRust
    case sleepDeadlineWasNotProjected
    case manualToAutoDidNotResumeAtTheVisibleSentence
    case autoToAudioDidNotTransferTheActiveCursor
    case lateAudioTookOverAfterModeTransfer
}
"#;
