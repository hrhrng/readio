import Foundation

enum ApplePositionAffinity: String, Codable, Equatable, Sendable {
    case forward
    case backward
}

struct AppleTextQuote: Codable, Equatable, Sendable {
    let prefix: String
    let exact: String
    let suffix: String
    let offsetInExact: UInt64

    enum CodingKeys: String, CodingKey {
        case prefix
        case exact
        case suffix
        case offsetInExact = "offset_in_exact"
    }
}

struct AppleTextLocator: Codable, Equatable, Sendable {
    let version: UInt16
    let documentRevision: String
    let charOffset: UInt64
    let blockId: String
    let offsetInBlock: UInt64
    let affinity: ApplePositionAffinity
    let quote: AppleTextQuote

    enum CodingKeys: String, CodingKey {
        case version
        case documentRevision = "document_revision"
        case charOffset = "char_offset"
        case blockId = "block_id"
        case offsetInBlock = "offset_in_block"
        case affinity
        case quote
    }
}

struct AppleReadingPositionMutation: Codable, Equatable, Sendable {
    let documentRevision: String
    let charOffset: UInt64
    let affinity: ApplePositionAffinity
    let observedAtUnixMilliseconds: Int64
    let sessionId: String
    let mutationId: String
    let baseVersion: UInt64
    let locator: AppleTextLocator?

    init(
        documentRevision: String,
        charOffset: UInt64,
        affinity: ApplePositionAffinity,
        observedAtUnixMilliseconds: Int64,
        sessionId: String,
        mutationId: String,
        baseVersion: UInt64,
        locator: AppleTextLocator? = nil
    ) {
        self.documentRevision = documentRevision
        self.charOffset = charOffset
        self.affinity = affinity
        self.observedAtUnixMilliseconds = observedAtUnixMilliseconds
        self.sessionId = sessionId
        self.mutationId = mutationId
        self.baseVersion = baseVersion
        self.locator = locator
    }

    enum CodingKeys: String, CodingKey {
        case documentRevision = "document_revision"
        case charOffset = "char_offset"
        case affinity
        case observedAtUnixMilliseconds = "observed_at_unix_ms"
        case sessionId = "session_id"
        case mutationId = "mutation_id"
        case baseVersion = "base_version"
        case locator
    }
}

struct AppleReadingPositionRecord: Codable, Equatable, Sendable {
    let documentRevision: String
    let charOffset: UInt64
    let affinity: ApplePositionAffinity
    let observedAtUnixMilliseconds: Int64
    let sessionId: String
    let mutationId: String
    let baseVersion: UInt64
    let version: UInt64
    let locator: AppleTextLocator?

    enum CodingKeys: String, CodingKey {
        case documentRevision = "document_revision"
        case charOffset = "char_offset"
        case affinity
        case observedAtUnixMilliseconds = "observed_at_unix_ms"
        case sessionId = "session_id"
        case mutationId = "mutation_id"
        case baseVersion = "base_version"
        case version
        case locator
    }
}

enum ApplePositionMutationStatus: String, Codable, Equatable, Sendable {
    case applied
    case conflict
}

enum ApplePositionCandidateOrigin: String, Codable, Equatable, Sendable {
    case current
    case incoming
}

struct ApplePositionCandidate: Codable, Equatable, Sendable {
    let origin: ApplePositionCandidateOrigin
    let documentRevision: String
    let charOffset: UInt64
    let affinity: ApplePositionAffinity
    let observedAtUnixMilliseconds: Int64
    let sessionId: String
    let mutationId: String
    let baseVersion: UInt64
    let version: UInt64
    let locator: AppleTextLocator?

    enum CodingKeys: String, CodingKey {
        case origin
        case documentRevision = "document_revision"
        case charOffset = "char_offset"
        case affinity
        case observedAtUnixMilliseconds = "observed_at_unix_ms"
        case sessionId = "session_id"
        case mutationId = "mutation_id"
        case baseVersion = "base_version"
        case version
        case locator
    }
}

struct ApplePositionMutationResponse: Codable, Equatable, Sendable {
    let status: ApplePositionMutationStatus
    let idempotentReplay: Bool
    let version: UInt64
    let position: AppleReadingPositionRecord?
    let conflictId: String?
    let candidates: [ApplePositionCandidate]

    enum CodingKeys: String, CodingKey {
        case status
        case idempotentReplay = "idempotent_replay"
        case version
        case position
        case conflictId = "conflict_id"
        case candidates
    }
}

/// Typed Apple façade over the Rust reading-position state machine.
struct AppleReadingPositionService: Sendable {
    private let localStore: AppleLocalReplicaStore

    init(localStore: AppleLocalReplicaStore = .shared) {
        self.localStore = localStore
    }

    func fetch(itemID: String) async throws -> AppleReadingPositionRecord? {
        let json = try await localStore.readingPositionJSON(itemID: itemID)
        return try JSONDecoder().decode(
            AppleReadingPositionRecord?.self,
            from: Data(json.utf8)
        )
    }

    func save(
        itemID: String,
        mutation: AppleReadingPositionMutation
    ) async throws -> ApplePositionMutationResponse {
        let mutationJSON = String(
            decoding: try JSONEncoder().encode(mutation),
            as: UTF8.self
        )
        let responseJSON = try await localStore.saveReadingPosition(
            itemID: itemID,
            mutationJSON: mutationJSON
        )
        return try JSONDecoder().decode(
            ApplePositionMutationResponse.self,
            from: Data(responseJSON.utf8)
        )
    }
}
