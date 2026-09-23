import AVFoundation
import Foundation

enum AppleTTSMode: String, CaseIterable, Sendable {
    case native
    case cloud
}

struct AppleTTSPreferences: Equatable, Sendable {
    static let defaultCloudVoiceID = "English_expressive_narrator"

    private enum Keys {
        static let mode = "readio.tts.mode"
        static let cloudVoiceID = "readio.tts.cloudVoiceID"
    }

    let mode: AppleTTSMode
    let cloudVoiceID: String

    init(defaults: UserDefaults = .standard) {
        mode = defaults.string(forKey: Keys.mode)
            .flatMap(AppleTTSMode.init(rawValue:)) ?? .native
        let storedVoice = defaults.string(forKey: Keys.cloudVoiceID)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        cloudVoiceID = storedVoice.flatMap { $0.isEmpty ? nil : $0 }
            ?? Self.defaultCloudVoiceID
    }
}

enum ReadioNativeVoiceQuality: String, Sendable {
    case `default`
    case enhanced
    case premium
}

struct ReadioNativeVoice: Equatable, Sendable {
    let id: String
    let name: String
    let language: String
    let gender: String?
    let quality: ReadioNativeVoiceQuality
    let isPersonal: Bool
}

struct ReadioNativeVoiceCatalog: Equatable, Sendable {
    let voices: [ReadioNativeVoice]
    let defaultVoiceID: String
}

struct ReadioNativeSpeechRequest: Equatable, Sendable {
    let text: String
    let voiceID: String?
    let language: String?
    let rateMultiplier: Float
    let pitchMultiplier: Float
    let volume: Float
}

struct ReadioNativeSpeechAudio: Equatable, Sendable {
    let data: Data
    let durationMilliseconds: Double
    let sampleRate: Double
}

enum ReadioNativeTTSError: Error, Equatable {
    case emptyText
    case unsupportedAudioBuffer
    case emptyAudio
}

/// Provider-neutral error presentation boundary. Playback depends only on this
/// protocol; optional providers map their own failures without being imported.
protocol ReadioTTSUserPresentableError: Error {
    var ttsUserMessage: String { get }
}

@MainActor
protocol ReadioAppleTTSProvider: AnyObject {
    var displayName: String { get }
    var diagnosticProviderID: String { get }
    func voiceCatalog() -> ReadioNativeVoiceCatalog
    func render(_ request: ReadioNativeSpeechRequest) async throws -> ReadioNativeSpeechAudio
    func cancelAll()
}

extension ReadioAppleTTSProvider {
    var displayName: String { "TTS provider" }
    var diagnosticProviderID: String {
        displayName == "Cloud TTS" ? "cloud" : "apple_native"
    }
}

@MainActor
final class AppleNativeTTSProvider: ReadioAppleTTSProvider {
    let displayName = "On-device TTS"
    let diagnosticProviderID = "apple_native"

    private var jobs: [UUID: NativeSpeechJob] = [:]
    private let diagnostics: AppleDiagnosticLog

    init(diagnostics: AppleDiagnosticLog = .shared) {
        self.diagnostics = diagnostics
    }

    func voiceCatalog() -> ReadioNativeVoiceCatalog {
        let systemVoices = AVSpeechSynthesisVoice.speechVoices()
        let voices = systemVoices
            .map(Self.mapVoice)
            .sorted {
                ($0.language, $0.name, $0.id) < ($1.language, $1.name, $1.id)
            }
        let preferred = AVSpeechSynthesisVoice(
            language: AVSpeechSynthesisVoice.currentLanguageCode()
        ) ?? systemVoices.first
        return ReadioNativeVoiceCatalog(
            voices: voices,
            defaultVoiceID: preferred?.identifier ?? voices.first?.id ?? ""
        )
    }

    func makeUtterance(_ request: ReadioNativeSpeechRequest) -> AVSpeechUtterance {
        let utterance = AVSpeechUtterance(string: request.text)
        utterance.voice = request.voiceID.flatMap(AVSpeechSynthesisVoice.init(identifier:))
            ?? request.language.flatMap(AVSpeechSynthesisVoice.init(language:))
        utterance.rate = min(
            AVSpeechUtteranceMaximumSpeechRate,
            max(
                AVSpeechUtteranceMinimumSpeechRate,
                AVSpeechUtteranceDefaultSpeechRate * request.rateMultiplier
            )
        )
        utterance.pitchMultiplier = min(2, max(0.5, request.pitchMultiplier))
        utterance.volume = min(1, max(0, request.volume))
        return utterance
    }

    func render(_ request: ReadioNativeSpeechRequest) async throws -> ReadioNativeSpeechAudio {
        let operationID = diagnostics.beginOperation()
        let jobID = UUID()
        let diagnosticJobID = jobID.uuidString.lowercased()
        diagnostics.record(
            operationID: operationID,
            component: "apple-native-tts",
            category: .tts,
            name: "job_requested",
            fields: [
                "provider": .string("apple_native"),
                "job_id": .string(diagnosticJobID),
            ]
        )
        guard !request.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            diagnostics.record(
                operationID: operationID,
                component: "apple-native-tts",
                category: .tts,
                name: "job_failed",
                fields: [
                    "provider": .string("apple_native"),
                    "job_id": .string(diagnosticJobID),
                    "outcome": .string("failed"),
                    "reason_code": .string("empty_text"),
                ]
            )
            throw ReadioNativeTTSError.emptyText
        }

        let outputURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("readio-native-tts-\(jobID.uuidString).caf")
        let synthesizer = AVSpeechSynthesizer()
        let capture = NativeSpeechCapture(outputURL: outputURL)
        jobs[jobID] = NativeSpeechJob(synthesizer: synthesizer, capture: capture)

        defer {
            jobs.removeValue(forKey: jobID)
            try? FileManager.default.removeItem(at: outputURL)
        }

        do {
            let audio = try await withTaskCancellationHandler {
                try await withCheckedThrowingContinuation { continuation in
                    capture.begin(continuation)
                    synthesizer.write(makeUtterance(request)) { buffer in
                        capture.receive(buffer)
                    }
                }
            } onCancel: {
                Task { @MainActor [weak self] in
                    self?.cancel(jobID)
                }
            }
            diagnostics.record(
                operationID: operationID,
                component: "apple-native-tts",
                category: .tts,
                name: "job_completed",
                fields: [
                    "provider": .string("apple_native"),
                    "job_id": .string(diagnosticJobID),
                    "audio_duration_ms": .number(audio.durationMilliseconds),
                    "outcome": .string("completed"),
                ]
            )
            return audio
        } catch {
            diagnostics.record(
                operationID: operationID,
                component: "apple-native-tts",
                category: .tts,
                name: "job_failed",
                fields: [
                    "provider": .string("apple_native"),
                    "job_id": .string(diagnosticJobID),
                    "outcome": .string("failed"),
                    "reason_code": .string("provider_failed"),
                ]
            )
            throw error
        }
    }

    func cancelAll() {
        let active = jobs.values
        jobs.removeAll()
        for job in active {
            job.capture.cancel()
            job.synthesizer.stopSpeaking(at: .immediate)
        }
    }

    private func cancel(_ jobID: UUID) {
        guard let job = jobs.removeValue(forKey: jobID) else { return }
        job.capture.cancel()
        job.synthesizer.stopSpeaking(at: .immediate)
    }

    private static func mapVoice(_ voice: AVSpeechSynthesisVoice) -> ReadioNativeVoice {
        let quality: ReadioNativeVoiceQuality = switch voice.quality {
        case .enhanced: .enhanced
        case .premium: .premium
        default: .default
        }
        let gender: String? = switch voice.gender {
        case .male: "male"
        case .female: "female"
        default: nil
        }
        let isPersonal: Bool
        if #available(iOS 17.0, macOS 14.0, *) {
            isPersonal = voice.voiceTraits.contains(.isPersonalVoice)
        } else {
            isPersonal = false
        }
        return ReadioNativeVoice(
            id: voice.identifier,
            name: voice.name,
            language: voice.language,
            gender: gender,
            quality: quality,
            isPersonal: isPersonal
        )
    }
}

private struct NativeSpeechJob {
    let synthesizer: AVSpeechSynthesizer
    let capture: NativeSpeechCapture
}

private final class NativeSpeechCapture: @unchecked Sendable {
    private let outputURL: URL
    private let lock = NSLock()
    private var continuation: CheckedContinuation<ReadioNativeSpeechAudio, Error>?
    private var audioFile: AVAudioFile?
    private var frameCount: AVAudioFramePosition = 0
    private var sampleRate: Double = 0
    private var isFinished = false

    init(outputURL: URL) {
        self.outputURL = outputURL
    }

    func begin(_ continuation: CheckedContinuation<ReadioNativeSpeechAudio, Error>) {
        lock.withLock {
            self.continuation = continuation
        }
    }

    func receive(_ buffer: AVAudioBuffer) {
        var completion: (
            CheckedContinuation<ReadioNativeSpeechAudio, Error>,
            Result<ReadioNativeSpeechAudio, Error>
        )?

        lock.lock()
        guard !isFinished else {
            lock.unlock()
            return
        }

        do {
            guard let buffer = buffer as? AVAudioPCMBuffer else {
                throw ReadioNativeTTSError.unsupportedAudioBuffer
            }
            if buffer.frameLength == 0 {
                isFinished = true
                audioFile = nil
                guard frameCount > 0, sampleRate > 0 else {
                    throw ReadioNativeTTSError.emptyAudio
                }
                let audio = ReadioNativeSpeechAudio(
                    data: try Data(contentsOf: outputURL),
                    durationMilliseconds: Double(frameCount) / sampleRate * 1_000,
                    sampleRate: sampleRate
                )
                if let continuation {
                    self.continuation = nil
                    completion = (continuation, .success(audio))
                }
            } else {
                if audioFile == nil {
                    audioFile = try AVAudioFile(
                        forWriting: outputURL,
                        settings: buffer.format.settings
                    )
                    sampleRate = buffer.format.sampleRate
                }
                try audioFile?.write(from: buffer)
                frameCount += AVAudioFramePosition(buffer.frameLength)
            }
        } catch {
            isFinished = true
            audioFile = nil
            if let continuation {
                self.continuation = nil
                completion = (continuation, .failure(error))
            }
        }
        lock.unlock()

        if let (continuation, result) = completion {
            continuation.resume(with: result)
        }
    }

    func cancel() {
        let continuation = lock.withLock {
            guard !isFinished else { return nil as CheckedContinuation<ReadioNativeSpeechAudio, Error>? }
            isFinished = true
            audioFile = nil
            defer { self.continuation = nil }
            return self.continuation
        }
        continuation?.resume(throwing: CancellationError())
    }
}
