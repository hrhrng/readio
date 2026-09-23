import Foundation

enum ReadioTextAdvanceStatus: String, Codable, Equatable, Sendable {
    case idle
    case running
    case paused
    case completed
}

struct ReadioTextAdvanceState: Codable, Equatable, Sendable {
    let status: ReadioTextAdvanceStatus
    let currentSentence: Int
    let sentenceCount: Int
    let generation: UInt64
    let deadlineMonotonicMilliseconds: UInt64?
    let remainingMilliseconds: UInt64?

    enum CodingKeys: String, CodingKey {
        case status
        case currentSentence = "current_sentence"
        case sentenceCount = "sentence_count"
        case generation
        case deadlineMonotonicMilliseconds = "deadline_monotonic_ms"
        case remainingMilliseconds = "remaining_ms"
    }
}

@MainActor
protocol ReadioTextAdvancePlatform: AnyObject {
    var monotonicMilliseconds: UInt64 { get }

    func scheduleAdvance(generation: UInt64, afterMilliseconds: UInt64)
    func cancelScheduledAdvance()
    func persistTextProgress(sentence: Int)
    func textAdvanceCompleted()
    func textAdvanceStateChanged(_ state: ReadioTextAdvanceState)
}

enum ReadioTextAdvanceExecutorError: Error, Equatable {
    case invalidEvent
    case invalidEffect(String)
}

private struct ReadioTextAdvanceTransition: Decodable {
    let state: ReadioTextAdvanceState
    let effects: [ReadioTextAdvanceEffect]
}

private struct ReadioTextAdvanceEffect: Decodable {
    let type: String
    let generation: UInt64?
    let afterMilliseconds: UInt64?
    let sentence: Int?

    enum CodingKeys: String, CodingKey {
        case type
        case generation
        case afterMilliseconds = "after_ms"
        case sentence
    }
}

/// Apple adapter for Rust-owned automatic text deadlines and sentence changes.
@MainActor
final class ReadioTextAdvanceExecutor {
    private let sdk: ReadioSDK
    private weak var platform: (any ReadioTextAdvancePlatform)?
    private var characterCounts: [UInt64]
    private var charactersPerSecond: Double
    private(set) var state: ReadioTextAdvanceState

    init(
        characterCounts: [Int],
        charactersPerSecond: Double,
        platform: any ReadioTextAdvancePlatform,
        sdk: ReadioSDK = ReadioSDK()
    ) throws {
        self.sdk = sdk
        self.platform = platform
        self.characterCounts = characterCounts.map { UInt64(max(0, $0)) }
        self.charactersPerSecond = charactersPerSecond
        let stateJSON = try sdk.createTextAdvanceState(
            sentenceCount: characterCounts.count
        )
        self.state = try JSONDecoder().decode(
            ReadioTextAdvanceState.self,
            from: Data(stateJSON.utf8)
        )
        platform.textAdvanceStateChanged(state)
    }

    func start() throws {
        try dispatch(
            type: "start_requested",
            fields: [
                "character_count": characterCount(at: state.currentSentence),
                "text_characters_per_second": charactersPerSecond,
                "monotonic_ms": platform?.monotonicMilliseconds ?? 0,
            ]
        )
    }

    func pause() throws {
        try dispatch(
            type: "pause_requested",
            fields: ["monotonic_ms": platform?.monotonicMilliseconds ?? 0]
        )
    }

    func seek(sentence: Int, autoplay: Bool) throws {
        let target = clampedSentence(sentence)
        try dispatch(
            type: "seek_requested",
            fields: [
                "sentence": target,
                "character_count": characterCount(at: target),
                "text_characters_per_second": charactersPerSecond,
                "monotonic_ms": platform?.monotonicMilliseconds ?? 0,
                "autoplay": autoplay,
            ]
        )
    }

    /// Updates the Rust event input without replacing the executor state.
    /// The next seek/start owns the single generation change for a live pace
    /// update, so callers do not need a throwaway executor or duplicate seek.
    func updateCharactersPerSecond(_ newValue: Double) {
        charactersPerSecond = newValue
    }

    func advanceDue(generation: UInt64) throws {
        try dispatch(
            type: "advance_due",
            fields: [
                "generation": generation,
                "monotonic_ms": platform?.monotonicMilliseconds ?? 0,
                "next_character_count": characterCount(
                    at: state.currentSentence + 1
                ),
                "text_characters_per_second": charactersPerSecond,
            ]
        )
    }

    private func dispatch(type: String, fields: [String: Any]) throws {
        var event = fields
        event["type"] = type
        guard JSONSerialization.isValidJSONObject(event) else {
            throw ReadioTextAdvanceExecutorError.invalidEvent
        }
        let eventJSON = String(
            decoding: try JSONSerialization.data(withJSONObject: event),
            as: UTF8.self
        )
        let stateJSON = String(
            decoding: try JSONEncoder().encode(state),
            as: UTF8.self
        )
        let transitionJSON = try sdk.reduceTextAdvance(
            stateJSON: stateJSON,
            eventJSON: eventJSON
        )
        let transition = try JSONDecoder().decode(
            ReadioTextAdvanceTransition.self,
            from: Data(transitionJSON.utf8)
        )
        state = transition.state
        platform?.textAdvanceStateChanged(state)
        for effect in transition.effects {
            try apply(effect)
        }
    }

    private func apply(_ effect: ReadioTextAdvanceEffect) throws {
        guard let platform else { return }
        switch effect.type {
        case "schedule_advance":
            guard let generation = effect.generation,
                  let after = effect.afterMilliseconds else {
                throw ReadioTextAdvanceExecutorError.invalidEffect(effect.type)
            }
            platform.scheduleAdvance(
                generation: generation,
                afterMilliseconds: after
            )
        case "cancel_scheduled_advance":
            platform.cancelScheduledAdvance()
        case "persist_progress":
            guard let sentence = effect.sentence else {
                throw ReadioTextAdvanceExecutorError.invalidEffect(effect.type)
            }
            platform.persistTextProgress(sentence: sentence)
        case "text_advance_completed":
            platform.textAdvanceCompleted()
        default:
            throw ReadioTextAdvanceExecutorError.invalidEffect(effect.type)
        }
    }

    private func characterCount(at sentence: Int) -> UInt64 {
        characterCounts.indices.contains(sentence) ? characterCounts[sentence] : 0
    }

    private func clampedSentence(_ sentence: Int) -> Int {
        guard !characterCounts.isEmpty else { return 0 }
        return min(max(0, sentence), characterCounts.count - 1)
    }
}
