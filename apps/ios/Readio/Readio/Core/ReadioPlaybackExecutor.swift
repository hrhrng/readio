import Foundation

enum ReadioPlaybackStatus: String, Codable, Equatable, Sendable {
    case idle
    case loading
    case playing
    case paused
    case failed
    case completed
}

struct ReadioPlaybackState: Codable, Equatable, Sendable {
    let status: ReadioPlaybackStatus
    let currentSentence: Int
    let sentenceCount: Int
    let generation: UInt64
    let currentDurationMilliseconds: UInt64?
    let currentPositionMilliseconds: UInt64
    let lastTickMonotonicMilliseconds: UInt64?
    let sleepDeadlineUnixMilliseconds: Int64?
    let error: String?

    enum CodingKeys: String, CodingKey {
        case status
        case currentSentence = "current_sentence"
        case sentenceCount = "sentence_count"
        case generation
        case currentDurationMilliseconds = "current_duration_ms"
        case currentPositionMilliseconds = "current_position_ms"
        case lastTickMonotonicMilliseconds = "last_tick_monotonic_ms"
        case sleepDeadlineUnixMilliseconds = "sleep_deadline_unix_ms"
        case error
    }
}

@MainActor
protocol ReadioPlaybackPlatform: AnyObject {
    var audioPositionMilliseconds: UInt64 { get }

    func fetchAudio(sentence: Int, generation: UInt64, priority: String)
    func playAudio(
        sentence: Int,
        generation: UInt64,
        offsetMilliseconds: UInt64
    )
    func scheduleTick(generation: UInt64, afterMilliseconds: UInt64)
    func pauseAudio()
    func persistProgress(sentence: Int, positionMilliseconds: UInt64)
    func prefetchAudio(sentences: [Int])
    func playbackFailed(sentence: Int, message: String)
    func playbackCompleted()
    func playbackStateChanged(_ state: ReadioPlaybackState)
}

enum ReadioPlaybackExecutorError: Error, Equatable {
    case invalidEvent
    case invalidEffect(String)
}

private struct ReadioPlaybackTransition: Decodable {
    let state: ReadioPlaybackState
    let effects: [ReadioPlaybackEffect]
}

private struct ReadioPlaybackEffect: Decodable {
    let type: String
    let sentence: Int?
    let generation: UInt64?
    let priority: String?
    let offsetMilliseconds: UInt64?
    let afterMilliseconds: UInt64?
    let positionMilliseconds: UInt64?
    let sentences: [Int]?
    let message: String?

    enum CodingKeys: String, CodingKey {
        case type
        case sentence
        case generation
        case priority
        case offsetMilliseconds = "offset_ms"
        case afterMilliseconds = "after_ms"
        case positionMilliseconds = "position_ms"
        case sentences
        case message
    }
}

/// Serial Apple adapter for the portable Rust playback reducer.
///
/// The platform supplies observed AVAudioPlayer/clock facts and executes effects;
/// sentence advancement, generation invalidation, seek, pause, failure, prefetch,
/// and sleep-deadline decisions remain in Rust.
@MainActor
final class ReadioPlaybackExecutor {
    private let sdk: ReadioSDK
    private weak var platform: (any ReadioPlaybackPlatform)?
    private let diagnostics: AppleDiagnosticLog
    private let diagnosticOperationID: String
    private let diagnosticProviderID: String
    private(set) var state: ReadioPlaybackState

    init(
        sentenceCount: Int,
        platform: any ReadioPlaybackPlatform,
        sdk: ReadioSDK = ReadioSDK(),
        diagnostics: AppleDiagnosticLog = .shared,
        diagnosticProviderID: String = "apple_native"
    ) throws {
        self.sdk = sdk
        self.platform = platform
        self.diagnostics = diagnostics
        self.diagnosticOperationID = diagnostics.beginOperation()
        self.diagnosticProviderID = diagnosticProviderID
        let stateJSON = try sdk.createPlaybackState(sentenceCount: sentenceCount)
        self.state = try JSONDecoder().decode(
            ReadioPlaybackState.self,
            from: Data(stateJSON.utf8)
        )
        platform.playbackStateChanged(state)
    }

    func play() throws {
        try dispatch(type: "play_requested")
    }

    func pause() throws {
        try dispatch(
            type: "pause_requested",
            fields: [
                "audio_position_ms": platform?.audioPositionMilliseconds ??
                    state.currentPositionMilliseconds
            ]
        )
    }

    func seek(
        sentence: Int,
        positionMilliseconds: UInt64 = 0,
        autoplay: Bool
    ) throws {
        try dispatch(
            type: "seek_requested",
            fields: [
                "sentence": sentence,
                "position_ms": positionMilliseconds,
                "autoplay": autoplay,
            ]
        )
    }

    func audioSourceChanged() throws {
        try dispatch(type: "audio_source_changed")
    }

    func audioReady(
        sentence: Int,
        generation: UInt64,
        durationMilliseconds: UInt64
    ) throws {
        try dispatch(
            type: "audio_ready",
            fields: [
                "sentence": sentence,
                "generation": generation,
                "duration_ms": durationMilliseconds,
            ]
        )
    }

    func audioFailed(sentence: Int, generation: UInt64, message: String) throws {
        try dispatch(
            type: "audio_failed",
            fields: [
                "sentence": sentence,
                "generation": generation,
                "message": message,
            ]
        )
    }

    func clockTick(
        generation: UInt64,
        monotonicMilliseconds: UInt64,
        unixMilliseconds: Int64
    ) throws {
        try dispatch(
            type: "clock_tick",
            fields: [
                "generation": generation,
                "monotonic_ms": monotonicMilliseconds,
                "unix_ms": unixMilliseconds,
                "audio_position_ms": platform?.audioPositionMilliseconds ??
                    state.currentPositionMilliseconds,
            ]
        )
    }

    func setSleepDeadline(unixMilliseconds: Int64) throws {
        try dispatch(
            type: "sleep_timer_set",
            fields: ["deadline_unix_ms": unixMilliseconds]
        )
    }

    func clearSleepDeadline() throws {
        try dispatch(type: "sleep_timer_cleared")
    }

    func audioEnded(sentence: Int, generation: UInt64) throws {
        try dispatch(
            type: "audio_ended",
            fields: [
                "sentence": sentence,
                "generation": generation,
            ]
        )
    }

    private func dispatch(type: String, fields: [String: Any] = [:]) throws {
        var event = fields
        event["type"] = type
        guard JSONSerialization.isValidJSONObject(event) else {
            throw ReadioPlaybackExecutorError.invalidEvent
        }
        let eventData = try JSONSerialization.data(withJSONObject: event)
        guard let eventJSON = String(data: eventData, encoding: .utf8) else {
            throw ReadioPlaybackExecutorError.invalidEvent
        }
        let transitionJSON = try sdk.reducePlayback(
            stateJSON: String(decoding: try JSONEncoder().encode(state), as: UTF8.self),
            eventJSON: eventJSON
        )
        let transition = try JSONDecoder().decode(
            ReadioPlaybackTransition.self,
            from: Data(transitionJSON.utf8)
        )
        state = transition.state
        platform?.playbackStateChanged(state)
        let eventGeneration = (fields["generation"] as? UInt64) ?? state.generation
        diagnostics.record(
            operationID: diagnosticOperationID,
            component: "apple-player",
            category: .playback,
            name: "clock_sample",
            fields: [
                "mode": .string("read_aloud"),
                "generation": .number(Double(eventGeneration)),
                "sentence_index": .number(Double(state.currentSentence)),
                "media_position_ms": .number(Double(
                    platform?.audioPositionMilliseconds ?? state.currentPositionMilliseconds
                )),
                "media_duration_ms": .number(Double(
                    state.currentDurationMilliseconds ?? 0
                )),
                "playing": .boolean(state.status == .playing),
            ]
        )
        if type == "audio_failed" {
            diagnostics.record(
                operationID: diagnosticOperationID,
                component: "apple-player",
                category: .fallback,
                name: "provider_fallback_stopped",
                fields: [
                    "from_provider": .string(diagnosticProviderID),
                    "to_provider": .string("none"),
                    "attempt": .number(1),
                    "outcome": .string("stopped"),
                    "reason_code": .string("provider_failed"),
                ]
            )
        }
        for effect in transition.effects {
            try apply(effect)
        }
    }

    private func apply(_ effect: ReadioPlaybackEffect) throws {
        guard let platform else { return }
        switch effect.type {
        case "fetch_audio":
            guard let sentence = effect.sentence,
                  let generation = effect.generation,
                  let priority = effect.priority else {
                throw ReadioPlaybackExecutorError.invalidEffect(effect.type)
            }
            platform.fetchAudio(
                sentence: sentence,
                generation: generation,
                priority: priority
            )
        case "play_audio":
            guard let sentence = effect.sentence,
                  let generation = effect.generation,
                  let offset = effect.offsetMilliseconds else {
                throw ReadioPlaybackExecutorError.invalidEffect(effect.type)
            }
            platform.playAudio(
                sentence: sentence,
                generation: generation,
                offsetMilliseconds: offset
            )
        case "schedule_tick":
            guard let generation = effect.generation,
                  let after = effect.afterMilliseconds else {
                throw ReadioPlaybackExecutorError.invalidEffect(effect.type)
            }
            platform.scheduleTick(generation: generation, afterMilliseconds: after)
        case "pause_audio":
            platform.pauseAudio()
        case "persist_progress":
            guard let sentence = effect.sentence,
                  let position = effect.positionMilliseconds else {
                throw ReadioPlaybackExecutorError.invalidEffect(effect.type)
            }
            platform.persistProgress(
                sentence: sentence,
                positionMilliseconds: position
            )
        case "prefetch_audio":
            guard let sentences = effect.sentences else {
                throw ReadioPlaybackExecutorError.invalidEffect(effect.type)
            }
            platform.prefetchAudio(sentences: sentences)
        case "playback_failed":
            guard let sentence = effect.sentence,
                  let message = effect.message else {
                throw ReadioPlaybackExecutorError.invalidEffect(effect.type)
            }
            platform.playbackFailed(sentence: sentence, message: message)
        case "playback_completed":
            platform.playbackCompleted()
        default:
            throw ReadioPlaybackExecutorError.invalidEffect(effect.type)
        }
    }
}
