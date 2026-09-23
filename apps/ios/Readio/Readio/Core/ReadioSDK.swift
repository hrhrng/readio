import Foundation
import ReadioCore

enum ReadioSDKError: Error, Equatable {
    case invalidSentenceCount(Int)
    case core(String)
    case missingValue
}

enum ReadioReplicaEntityKind: String, Sendable {
    case libraryItem = "library_item"
    case readingPosition = "reading_position"
    case bookmark
    case setting
}

enum ReadioReadingMode: String, Codable, CaseIterable, Hashable, Sendable {
    case manual
    case auto
    case readAloud = "read_aloud"
}

enum ReadioEffortLevel: String, Codable, CaseIterable, Hashable, Sendable {
    case minimal
    case low
    case medium
    case high
    case xhigh
    case max
}

struct ReadioReadingPreferences: Encodable, Sendable {
    let mode: ReadioReadingMode
    let baseTextCharactersPerSecond: Double
    let effort: ReadioEffortLevel

    enum CodingKeys: String, CodingKey {
        case mode
        case baseTextCharactersPerSecond = "base_text_characters_per_second"
        case effort
    }
}

struct ReadioReadingPace: Decodable, Equatable, Sendable {
    let mode: ReadioReadingMode
    let effort: ReadioEffortLevel
    let multiplier: Double
    let textCharactersPerSecond: Double
    let audioPlaybackRate: Double
    let advancesAutomatically: Bool
    let usesAudio: Bool

    enum CodingKeys: String, CodingKey {
        case mode
        case effort
        case multiplier
        case textCharactersPerSecond = "text_characters_per_second"
        case audioPlaybackRate = "audio_playback_rate"
        case advancesAutomatically = "advances_automatically"
        case usesAudio = "uses_audio"
    }
}

struct ReadioAudioDurationObservation: Codable, Equatable, Sendable {
    let segment: Int
    let durationMilliseconds: UInt64

    enum CodingKeys: String, CodingKey {
        case segment
        case durationMilliseconds = "duration_ms"
    }
}

enum ReadioReadingTimingEvidence: Codable, Equatable, Sendable {
    case unknown
    case textRate(charactersPerSecond: Double)
    case observedAudio(
        playbackRate: Double,
        currentPositionMilliseconds: UInt64,
        durations: [ReadioAudioDurationObservation]
    )

    private enum Kind: String, Codable {
        case unknown
        case textRate = "text_rate"
        case observedAudio = "observed_audio"
    }

    private enum CodingKeys: String, CodingKey {
        case type
        case charactersPerSecond = "characters_per_second"
        case playbackRate = "playback_rate"
        case currentPositionMilliseconds = "current_position_ms"
        case durations
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Kind.self, forKey: .type) {
        case .unknown:
            self = .unknown
        case .textRate:
            self = try .textRate(
                charactersPerSecond: container.decode(
                    Double.self,
                    forKey: .charactersPerSecond
                )
            )
        case .observedAudio:
            self = try .observedAudio(
                playbackRate: container.decode(Double.self, forKey: .playbackRate),
                currentPositionMilliseconds: container.decode(
                    UInt64.self,
                    forKey: .currentPositionMilliseconds
                ),
                durations: container.decode(
                    [ReadioAudioDurationObservation].self,
                    forKey: .durations
                )
            )
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .unknown:
            try container.encode(Kind.unknown, forKey: .type)
        case let .textRate(charactersPerSecond):
            try container.encode(Kind.textRate, forKey: .type)
            try container.encode(charactersPerSecond, forKey: .charactersPerSecond)
        case let .observedAudio(playbackRate, currentPositionMilliseconds, durations):
            try container.encode(Kind.observedAudio, forKey: .type)
            try container.encode(playbackRate, forKey: .playbackRate)
            try container.encode(
                currentPositionMilliseconds,
                forKey: .currentPositionMilliseconds
            )
            try container.encode(durations, forKey: .durations)
        }
    }
}

struct ReadioReadingProgressRequest: Encodable, Equatable, Sendable {
    let segmentCharacterCounts: [UInt64]
    let currentSegment: Int
    let currentSegmentProgressBasisPoints: UInt16
    let timing: ReadioReadingTimingEvidence

    enum CodingKeys: String, CodingKey {
        case segmentCharacterCounts = "segment_character_counts"
        case currentSegment = "current_segment"
        case currentSegmentProgressBasisPoints = "current_segment_progress_basis_points"
        case timing
    }
}

enum ReadioRemainingTimeKind: String, Decodable, Equatable, Sendable {
    case unknown
    case textRate = "text_rate"
    case observedAudio = "observed_audio"
}

struct ReadioRemainingTimeProjection: Decodable, Equatable, Sendable {
    let kind: ReadioRemainingTimeKind
    let seconds: UInt64?
    let sampleCount: UInt16

    enum CodingKeys: String, CodingKey {
        case kind
        case seconds
        case sampleCount = "sample_count"
    }
}

struct ReadioReadingProgressProjection: Decodable, Equatable, Sendable {
    let charOffset: UInt64
    let charCount: UInt64
    let progressBasisPoints: UInt16
    let remainingCharacters: UInt64
    let remainingTime: ReadioRemainingTimeProjection

    enum CodingKeys: String, CodingKey {
        case charOffset = "char_offset"
        case charCount = "char_count"
        case progressBasisPoints = "progress_basis_points"
        case remainingCharacters = "remaining_characters"
        case remainingTime = "remaining_time"
    }
}

struct ReadioReadingSeekCursor: Decodable, Equatable, Sendable {
    let segment: Int
    let segmentProgressBasisPoints: UInt16
    let charOffset: UInt64

    enum CodingKeys: String, CodingKey {
        case segment
        case segmentProgressBasisPoints = "segment_progress_basis_points"
        case charOffset = "char_offset"
    }
}

struct ReadioScalarRange: Codable, Equatable, Sendable {
    let start: UInt64
    let end: UInt64

    init(start: UInt64, end: UInt64) {
        self.start = start
        self.end = end
    }

    init(from decoder: Decoder) throws {
        var values = try decoder.unkeyedContainer()
        start = try values.decode(UInt64.self)
        end = try values.decode(UInt64.self)
        guard values.isAtEnd, start <= end else {
            throw DecodingError.dataCorruptedError(
                in: values,
                debugDescription: "a scalar range must contain two ordered offsets"
            )
        }
    }

    func encode(to encoder: Encoder) throws {
        var values = encoder.unkeyedContainer()
        try values.encode(start)
        try values.encode(end)
    }
}

enum ReadioDocumentBlockType: String, Codable, Equatable, Sendable {
    case heading
    case paragraph
    case quote
    case code
    case image
}

struct ReadioDocumentBlockKind: Codable, Equatable, Sendable {
    let type: ReadioDocumentBlockType
    let level: UInt8?
    let assetID: String?

    enum CodingKeys: String, CodingKey {
        case type
        case level
        case assetID = "asset_id"
    }
}

struct ReadioDocumentInlineSpan: Codable, Equatable, Sendable {
    let range: ReadioScalarRange
    let bold: Bool
    let italic: Bool
}

enum ReadioDocumentAssetKind: String, Codable, Equatable, Sendable {
    case image
}

struct ReadioDocumentAsset: Codable, Equatable, Sendable {
    let id: String
    let kind: ReadioDocumentAssetKind
    let source: String
    let alt: String
}

struct ReadioDocumentAnchor: Codable, Equatable, Sendable {
    let sourceID: String
    let blockID: String

    enum CodingKeys: String, CodingKey {
        case sourceID = "source_id"
        case blockID = "block_id"
    }
}

struct ReadioDocumentBlock: Codable, Equatable, Identifiable, Sendable {
    let id: String
    let kind: ReadioDocumentBlockKind
    let text: String
    let inlineSpans: [ReadioDocumentInlineSpan]

    enum CodingKeys: String, CodingKey {
        case id
        case kind
        case text
        case inlineSpans = "inline_spans"
    }
}

struct ReadioDocumentRevision: Codable, Equatable, Sendable {
    let contentIdentity: ReadioContentIdentity
    let irVersion: UInt32
    let parserVersion: String
    let revisionDigest: String
    let charCount: UInt64

    enum CodingKeys: String, CodingKey {
        case contentIdentity = "content_identity"
        case irVersion = "ir_version"
        case parserVersion = "parser_version"
        case revisionDigest = "revision_digest"
        case charCount = "char_count"
    }
}

struct ReadioDocumentIR: Codable, Equatable, Sendable {
    let irVersion: UInt32
    let revision: ReadioDocumentRevision
    let blocks: [ReadioDocumentBlock]
    let assets: [ReadioDocumentAsset]
    let anchors: [ReadioDocumentAnchor]

    enum CodingKeys: String, CodingKey {
        case irVersion = "ir_version"
        case revision
        case blocks
        case assets
        case anchors
    }

    var canonicalText: String {
        blocks.map(\.text).joined(separator: "\n")
    }
}

struct ReadioIndexedWord: Codable, Equatable, Sendable {
    let index: UInt32
    let range: ReadioScalarRange
    let charRange: ReadioScalarRange
    let text: String

    enum CodingKeys: String, CodingKey {
        case index
        case range
        case charRange = "char_range"
        case text
    }
}

struct ReadioIndexedSentence: Codable, Equatable, Identifiable, Sendable {
    let id: String
    let index: UInt32
    let blockID: String
    let range: ReadioScalarRange
    let charRange: ReadioScalarRange
    let text: String
    let words: [ReadioIndexedWord]

    enum CodingKeys: String, CodingKey {
        case id
        case index
        case blockID = "block_id"
        case range
        case charRange = "char_range"
        case text
        case words
    }
}

struct ReadioDocumentTextIndex: Codable, Equatable, Sendable {
    let version: UInt16
    let documentRevision: String
    let sentences: [ReadioIndexedSentence]

    enum CodingKeys: String, CodingKey {
        case version
        case documentRevision = "document_revision"
        case sentences
    }
}

struct ReadioReaderChapter: Codable, Equatable, Identifiable, Sendable {
    let id: String
    let title: String
    let blockRange: ReadioScalarRange
    let sentenceRange: ReadioScalarRange
    let charRange: ReadioScalarRange

    enum CodingKeys: String, CodingKey {
        case id
        case title
        case blockRange = "block_range"
        case sentenceRange = "sentence_range"
        case charRange = "char_range"
    }
}

struct ReadioReaderNavigation: Codable, Equatable, Identifiable, Sendable {
    let id: String
    let title: String
    let depth: Int
    let chapterID: String
    let blockID: String?
    let sentenceIndex: UInt32?

    enum CodingKeys: String, CodingKey {
        case id
        case title
        case depth
        case chapterID = "chapter_id"
        case blockID = "block_id"
        case sentenceIndex = "sentence_index"
    }
}

/// The one parser-owned value projected by SwiftUI/AppKit. All offsets are
/// half-open Unicode-scalar ranges fixed by Rust before crossing the C ABI.
struct ReadioReaderIR: Codable, Equatable, Sendable {
    let irVersion: UInt16
    let document: ReadioDocumentIR
    let textIndex: ReadioDocumentTextIndex
    let chapters: [ReadioReaderChapter]
    let navigation: [ReadioReaderNavigation]

    enum CodingKeys: String, CodingKey {
        case irVersion = "ir_version"
        case document
        case textIndex = "text_index"
        case chapters
        case navigation
    }
}

struct ReadioHighlightCursor: Encodable, Equatable, Sendable {
    let documentRevision: String
    let sentenceIndex: UInt32
    let wordIndex: UInt32?
    let wordProgressBasisPoints: UInt16?

    enum CodingKeys: String, CodingKey {
        case documentRevision = "document_revision"
        case sentenceIndex = "sentence_index"
        case wordIndex = "word_index"
        case wordProgressBasisPoints = "word_progress_basis_points"
    }
}

struct ReadioHighlightTarget: Decodable, Equatable, Sendable {
    let sentenceID: String
    let sentenceIndex: UInt32
    let blockID: String
    let sentenceRange: ReadioScalarRange
    let wordRange: ReadioScalarRange?
    let sentenceCharacterRange: ReadioScalarRange
    let wordCharacterRange: ReadioScalarRange?

    enum CodingKeys: String, CodingKey {
        case sentenceID = "sentence_id"
        case sentenceIndex = "sentence_index"
        case blockID = "block_id"
        case sentenceRange = "sentence_range"
        case wordRange = "word_range"
        case sentenceCharacterRange = "sentence_char_range"
        case wordCharacterRange = "word_char_range"
    }

    var sentenceLocalWordRange: ReadioScalarRange? {
        guard let wordRange,
              wordRange.start >= sentenceRange.start,
              wordRange.end <= sentenceRange.end else { return nil }
        return ReadioScalarRange(
            start: wordRange.start - sentenceRange.start,
            end: wordRange.end - sentenceRange.start
        )
    }
}

struct ReadioHighlightDocument: Sendable {
    let documentRevision: String
    fileprivate let documentJSON: String
    fileprivate let indexJSON: String
}

private struct ReadioReadingSeekRequest: Encodable {
    let segmentCharacterCounts: [UInt64]
    let progressBasisPoints: UInt16

    enum CodingKeys: String, CodingKey {
        case segmentCharacterCounts = "segment_character_counts"
        case progressBasisPoints = "progress_basis_points"
    }
}

struct ReadioContentIdentity: Codable, Equatable, Sendable {
    let algorithm: String
    let byteLength: UInt64
    let digest: String

    enum CodingKeys: String, CodingKey {
        case algorithm
        case byteLength = "byte_length"
        case digest
    }
}

enum ReadioImportOwnership: String, Codable, Sendable {
    case copy
    case link
    case move
}

struct ReadioImportRecord: Codable, Equatable, Sendable {
    let itemId: String
    let contentIdentity: ReadioContentIdentity
    let ownership: ReadioImportOwnership
    let sourceKey: String
    let storageKey: String

    enum CodingKeys: String, CodingKey {
        case itemId = "item_id"
        case contentIdentity = "content_identity"
        case ownership
        case sourceKey = "source_key"
        case storageKey = "storage_key"
    }
}

struct ReadioImportRequest: Encodable, Sendable {
    let contentIdentity: ReadioContentIdentity
    let ownership: ReadioImportOwnership
    let sourceKey: String
    let ownedStorageKey: String
    var replaceOwnership: Bool = false
    let existing: ReadioImportRecord?

    enum CodingKeys: String, CodingKey {
        case contentIdentity = "content_identity"
        case ownership
        case sourceKey = "source_key"
        case ownedStorageKey = "owned_storage_key"
        case replaceOwnership = "replace_ownership"
        case existing
    }
}

struct ReadioImportEffect: Decodable, Equatable, Sendable {
    enum Kind: String, Decodable, Sendable {
        case writeOwnedContent = "write_owned_content"
        case deleteSourceAfterCommit = "delete_source_after_commit"
        case deleteOwnedContent = "delete_owned_content"
    }

    let type: Kind
    let sourceKey: String?
    let storageKey: String?

    enum CodingKeys: String, CodingKey {
        case type
        case sourceKey = "source_key"
        case storageKey = "storage_key"
    }
}

struct ReadioImportPlan: Decodable, Equatable, Sendable {
    enum Outcome: String, Decodable, Sendable {
        case created
        case reused
        case updated
    }

    let outcome: Outcome
    let record: ReadioImportRecord
    let effects: [ReadioImportEffect]
}

struct ReadioBuiltinSample: Decodable, Equatable, Sendable {
    let itemID: String
    let legacyItemIDs: [String]
    let locale: String
    let title: String
    let author: String
    let source: String
    let contentFormat: String
    let content: String
    let markdownSource: String
    let isBuiltinSample: Bool

    enum CodingKeys: String, CodingKey {
        case itemID = "item_id"
        case legacyItemIDs = "legacy_item_ids"
        case locale
        case title
        case author
        case source
        case contentFormat = "content_format"
        case content
        case markdownSource = "markdown_source"
        case isBuiltinSample = "is_builtin_sample"
    }
}

enum ReadioDiagnosticCategory: String, Codable, Equatable, Sendable {
    case `import`
    case parsing
    case sync
    case tts
    case fallback
    case playback
}

enum ReadioDiagnosticFieldValue: Codable, Equatable, Sendable {
    case string(String)
    case number(Double)
    case boolean(Bool)

    init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer()
        if let boolean = try? value.decode(Bool.self) {
            self = .boolean(boolean)
        } else if let number = try? value.decode(Double.self) {
            self = .number(number)
        } else {
            self = try .string(value.decode(String.self))
        }
    }

    func encode(to encoder: Encoder) throws {
        var value = encoder.singleValueContainer()
        switch self {
        case .string(let string):
            try value.encode(string)
        case .number(let number):
            try value.encode(number)
        case .boolean(let boolean):
            try value.encode(boolean)
        }
    }
}

struct ReadioDiagnosticEventInput: Codable, Equatable, Sendable {
    let timestampUnixMilliseconds: UInt64
    let operationID: String
    let component: String
    let category: ReadioDiagnosticCategory
    let name: String
    let fields: [String: ReadioDiagnosticFieldValue]

    enum CodingKeys: String, CodingKey {
        case timestampUnixMilliseconds = "timestamp_unix_ms"
        case operationID = "operation_id"
        case component
        case category
        case name
        case fields
    }
}

struct ReadioSupportBundleRequest: Encodable, Equatable, Sendable {
    let generatedAtUnixMilliseconds: UInt64
    let platform: String
    let appVersion: String
    let events: [ReadioDiagnosticEventInput]

    enum CodingKeys: String, CodingKey {
        case generatedAtUnixMilliseconds = "generated_at_unix_ms"
        case platform
        case appVersion = "app_version"
        case events
    }
}

struct ReadioSupportBundleRedaction: Codable, Equatable, Sendable {
    let contentIncluded: Bool
    let secretsIncluded: Bool
    let droppedFieldCount: UInt64
    let droppedEventCount: UInt64

    enum CodingKeys: String, CodingKey {
        case contentIncluded = "content_included"
        case secretsIncluded = "secrets_included"
        case droppedFieldCount = "dropped_field_count"
        case droppedEventCount = "dropped_event_count"
    }
}

struct ReadioDiagnosticEvent: Codable, Equatable, Sendable {
    let timestampUnixMilliseconds: UInt64
    let operationID: String
    let component: String
    let category: ReadioDiagnosticCategory
    let name: String
    let fields: [String: ReadioDiagnosticFieldValue]

    enum CodingKeys: String, CodingKey {
        case timestampUnixMilliseconds = "timestamp_unix_ms"
        case operationID = "operation_id"
        case component
        case category
        case name
        case fields
    }
}

struct ReadioSupportBundle: Codable, Equatable, Sendable {
    let schemaVersion: UInt32
    let generatedAtUnixMilliseconds: UInt64
    let platform: String
    let appVersion: String
    let redaction: ReadioSupportBundleRedaction
    let events: [ReadioDiagnosticEvent]

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case generatedAtUnixMilliseconds = "generated_at_unix_ms"
        case platform
        case appVersion = "app_version"
        case redaction
        case events
    }
}

extension JSONEncoder {
    static var readioSupportBundle: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        return encoder
    }
}

/// Owns the unsafe C boundary while keeping the rest of the Apple app on JSON values.
/// Audio, clocks, networking, and persistence stay platform dependencies that execute
/// effects returned by the Rust reducer.
struct ReadioSDK: Sendable {
    func buildSupportBundle(
        _ request: ReadioSupportBundleRequest
    ) throws -> ReadioSupportBundle {
        let requestJSON = String(
            decoding: try JSONEncoder().encode(request),
            as: UTF8.self
        )
        let output = try requestJSON.withCString { request in
            try consumeFFIResult(readio_build_support_bundle(request))
        }
        return try JSONDecoder().decode(
            ReadioSupportBundle.self,
            from: Data(output.utf8)
        )
    }

    func builtinSample(languageTag: String) throws -> ReadioBuiltinSample {
        let output = try languageTag.withCString { languageTag in
            try consumeFFIResult(readio_builtin_sample(languageTag))
        }
        return try JSONDecoder().decode(
            ReadioBuiltinSample.self,
            from: Data(output.utf8)
        )
    }

    func makeSyncState(serializedJSON: String = "{}") -> ReadioSyncState {
        ReadioSyncState(serializedJSON: serializedJSON)
    }

    func makeReplica() -> ReadioReplica {
        guard let handle = readio_replica_new() else {
            preconditionFailure("the Rust SDK must return a live replica handle")
        }
        return ReadioReplica(handle: handle)
    }

    func makeReplica(snapshotBase64: String) throws -> ReadioReplica {
        try snapshotBase64.withCString { snapshot in
            let result = readio_replica_from_snapshot(snapshot)
            defer { readio_string_free(result.error) }

            if let error = result.error {
                throw ReadioSDKError.core(String(cString: error))
            }
            guard let handle = result.value else {
                throw ReadioSDKError.missingValue
            }
            return ReadioReplica(handle: handle)
        }
    }

    func createPlaybackState(sentenceCount: Int) throws -> String {
        guard let sentenceCount = UInt32(exactly: sentenceCount) else {
            throw ReadioSDKError.invalidSentenceCount(sentenceCount)
        }
        return try consumeFFIResult(readio_create_playback_state(sentenceCount))
    }

    func reducePlayback(
        policyJSON: String? = nil,
        stateJSON: String,
        eventJSON: String
    ) throws -> String {
        try withOptionalCString(policyJSON) { policy in
            try stateJSON.withCString { state in
                try eventJSON.withCString { event in
                    try consumeFFIResult(readio_reduce_playback(policy, state, event))
                }
            }
        }
    }

    func resolveReadingPace(preferencesJSON: String) throws -> String {
        try preferencesJSON.withCString { preferences in
            try consumeFFIResult(readio_resolve_reading_pace(preferences))
        }
    }

    func resolveReadingPace(
        _ preferences: ReadioReadingPreferences
    ) throws -> ReadioReadingPace {
        let input = try JSONEncoder().encode(preferences)
        let output = try resolveReadingPace(
            preferencesJSON: String(decoding: input, as: UTF8.self)
        )
        return try JSONDecoder().decode(
            ReadioReadingPace.self,
            from: Data(output.utf8)
        )
    }

    func projectReadingProgress(
        _ request: ReadioReadingProgressRequest
    ) throws -> ReadioReadingProgressProjection {
        let requestJSON = String(
            decoding: try JSONEncoder().encode(request),
            as: UTF8.self
        )
        let output = try requestJSON.withCString { request in
            try consumeFFIResult(readio_project_reading_progress(request))
        }
        return try JSONDecoder().decode(
            ReadioReadingProgressProjection.self,
            from: Data(output.utf8)
        )
    }

    func resolveReadingSeek(
        segmentCharacterCounts: [UInt64],
        progressBasisPoints: UInt16
    ) throws -> ReadioReadingSeekCursor {
        let request = ReadioReadingSeekRequest(
            segmentCharacterCounts: segmentCharacterCounts,
            progressBasisPoints: progressBasisPoints
        )
        let requestJSON = String(
            decoding: try JSONEncoder().encode(request),
            as: UTF8.self
        )
        let output = try requestJSON.withCString { request in
            try consumeFFIResult(readio_resolve_reading_seek(request))
        }
        return try JSONDecoder().decode(
            ReadioReadingSeekCursor.self,
            from: Data(output.utf8)
        )
    }

    func createTextAdvanceState(sentenceCount: Int) throws -> String {
        guard let sentenceCount = UInt32(exactly: sentenceCount) else {
            throw ReadioSDKError.invalidSentenceCount(sentenceCount)
        }
        return try consumeFFIResult(readio_create_text_advance_state(sentenceCount))
    }

    func reduceTextAdvance(stateJSON: String, eventJSON: String) throws -> String {
        try stateJSON.withCString { state in
            try eventJSON.withCString { event in
                try consumeFFIResult(readio_reduce_text_advance(state, event))
            }
        }
    }

    func contentIdentity(_ data: Data) throws -> ReadioContentIdentity {
        let output = try data.withUnsafeBytes { buffer in
            let bytes = buffer.bindMemory(to: UInt8.self)
            return try consumeFFIResult(
                readio_content_identity(bytes.baseAddress, bytes.count)
            )
        }
        return try JSONDecoder().decode(
            ReadioContentIdentity.self,
            from: Data(output.utf8)
        )
    }

    func planImport(_ request: ReadioImportRequest) throws -> ReadioImportPlan {
        let requestJSON = String(
            decoding: try JSONEncoder().encode(request),
            as: UTF8.self
        )
        let output = try requestJSON.withCString { request in
            try consumeFFIResult(readio_plan_import(request))
        }
        return try JSONDecoder().decode(ReadioImportPlan.self, from: Data(output.utf8))
    }

    func planForget(_ record: ReadioImportRecord) throws -> [ReadioImportEffect] {
        let recordJSON = String(
            decoding: try JSONEncoder().encode(record),
            as: UTF8.self
        )
        let output = try recordJSON.withCString { record in
            try consumeFFIResult(readio_plan_forget(record))
        }
        return try JSONDecoder().decode([ReadioImportEffect].self, from: Data(output.utf8))
    }

    func parseHTML(_ html: String, optionsJSON: String? = nil) throws -> String {
        try html.withCString { html in
            try withOptionalCString(optionsJSON) { options in
                try consumeFFIResult(readio_parse_html(html, options))
            }
        }
    }

    func parseHTMLReader(
        _ html: String,
        optionsJSON: String? = nil
    ) throws -> ReadioReaderIR {
        let output = try html.withCString { html in
            try withOptionalCString(optionsJSON) { options in
                try consumeFFIResult(readio_parse_html_reader(html, options))
            }
        }
        return try decodeReaderIR(output)
    }

    func parseMarkdownReader(
        _ markdown: String,
        optionsJSON: String? = nil
    ) throws -> ReadioReaderIR {
        let output = try markdown.withCString { markdown in
            try withOptionalCString(optionsJSON) { options in
                try consumeFFIResult(readio_parse_markdown_reader(markdown, options))
            }
        }
        return try decodeReaderIR(output)
    }

    func parsePlainTextReader(
        _ text: String,
        optionsJSON: String? = nil
    ) throws -> ReadioReaderIR {
        let output = try text.withCString { text in
            try withOptionalCString(optionsJSON) { options in
                try consumeFFIResult(readio_parse_plain_text_reader(text, options))
            }
        }
        return try decodeReaderIR(output)
    }

    func parsePDF(_ data: Data) throws -> String {
        try data.withUnsafeBytes { buffer in
            let bytes = buffer.bindMemory(to: UInt8.self)
            return try consumeFFIResult(
                readio_parse_pdf(bytes.baseAddress, bytes.count)
            )
        }
    }

    func parseEPUB(_ data: Data) throws -> String {
        try data.withUnsafeBytes { buffer in
            let bytes = buffer.bindMemory(to: UInt8.self)
            return try consumeFFIResult(
                readio_parse_epub(bytes.baseAddress, bytes.count)
            )
        }
    }

    func extractEPUBAsset(_ data: Data, assetID: String) throws -> Data {
        try data.withUnsafeBytes { buffer in
            let bytes = buffer.bindMemory(to: UInt8.self)
            return try assetID.withCString { assetID in
                let result = readio_extract_epub_asset(
                    bytes.baseAddress,
                    bytes.count,
                    assetID
                )
                defer {
                    readio_bytes_free(result.value, result.length)
                    readio_string_free(result.error)
                }
                if let error = result.error {
                    throw ReadioSDKError.core(String(cString: error))
                }
                guard let value = result.value else {
                    throw ReadioSDKError.missingValue
                }
                return Data(bytes: value, count: result.length)
            }
        }
    }

    func projectOffset(documentJSON: String, requestJSON: String) throws -> String {
        try documentJSON.withCString { document in
            try requestJSON.withCString { request in
                try consumeFFIResult(readio_project_offset(document, request))
            }
        }
    }

    func createTextLocator(documentJSON: String, requestJSON: String) throws -> String {
        try documentJSON.withCString { document in
            try requestJSON.withCString { request in
                try consumeFFIResult(readio_create_text_locator(document, request))
            }
        }
    }

    func restoreTextLocator(documentJSON: String, locatorJSON: String) throws -> String {
        try documentJSON.withCString { document in
            try locatorJSON.withCString { locator in
                try consumeFFIResult(readio_restore_text_locator(document, locator))
            }
        }
    }

    func indexDocumentText(documentJSON: String) throws -> String {
        try documentJSON.withCString { document in
            try consumeFFIResult(readio_index_document_text(document))
        }
    }

    func makeHighlightDocument(
        reader: ReadioReaderIR
    ) throws -> ReadioHighlightDocument {
        let revision = reader.document.revision.revisionDigest
        guard reader.textIndex.documentRevision == revision else {
            throw ReadioSDKError.core("reader document and text index revisions differ")
        }
        return ReadioHighlightDocument(
            documentRevision: revision,
            documentJSON: String(
                decoding: try JSONEncoder().encode(reader.document),
                as: UTF8.self
            ),
            indexJSON: String(
                decoding: try JSONEncoder().encode(reader.textIndex),
                as: UTF8.self
            )
        )
    }

    func resolveHighlight(
        documentJSON: String,
        indexJSON: String,
        cursorJSON: String
    ) throws -> String {
        try documentJSON.withCString { document in
            try indexJSON.withCString { index in
                try cursorJSON.withCString { cursor in
                    try consumeFFIResult(readio_resolve_highlight(document, index, cursor))
                }
            }
        }
    }

    func resolveHighlight(
        document: ReadioHighlightDocument,
        cursor: ReadioHighlightCursor
    ) throws -> ReadioHighlightTarget {
        let cursorJSON = String(
            decoding: try JSONEncoder().encode(cursor),
            as: UTF8.self
        )
        let output = try resolveHighlight(
            documentJSON: document.documentJSON,
            indexJSON: document.indexJSON,
            cursorJSON: cursorJSON
        )
        return try JSONDecoder().decode(
            ReadioHighlightTarget.self,
            from: Data(output.utf8)
        )
    }

    func resolveBlockOffset(documentJSON: String, requestJSON: String) throws -> String {
        try documentJSON.withCString { document in
            try requestJSON.withCString { request in
                try consumeFFIResult(readio_resolve_block_offset(document, request))
            }
        }
    }

    func restorePosition(documentJSON: String, positionJSON: String) throws -> String {
        try documentJSON.withCString { document in
            try positionJSON.withCString { position in
                try consumeFFIResult(readio_restore_position(document, position))
            }
        }
    }

    func searchDocument(documentJSON: String, requestJSON: String) throws -> String {
        try documentJSON.withCString { document in
            try requestJSON.withCString { request in
                try consumeFFIResult(readio_search_document(document, request))
            }
        }
    }

    func upsertBookmark(
        documentJSON: String,
        bookmarksJSON: String,
        requestJSON: String
    ) throws -> String {
        try documentJSON.withCString { document in
            try bookmarksJSON.withCString { bookmarks in
                try requestJSON.withCString { request in
                    try consumeFFIResult(readio_upsert_bookmark(document, bookmarks, request))
                }
            }
        }
    }

    func planSyncPush(
        snapshotJSON: String,
        batchJSON: String,
        serverTimestampUnixMilliseconds: UInt64,
        startingCursor: UInt64
    ) throws -> String {
        try snapshotJSON.withCString { snapshot in
            try batchJSON.withCString { batch in
                try consumeFFIResult(
                    readio_plan_sync_push(
                        snapshot,
                        batch,
                        serverTimestampUnixMilliseconds,
                        startingCursor
                    )
                )
            }
        }
    }

    private func decodeReaderIR(_ json: String) throws -> ReadioReaderIR {
        try JSONDecoder().decode(ReadioReaderIR.self, from: Data(json.utf8))
    }

    private func withOptionalCString<T>(
        _ value: String?,
        _ body: (UnsafePointer<CChar>?) throws -> T
    ) rethrows -> T {
        guard let value else {
            return try body(nil)
        }
        return try value.withCString(body)
    }

}

struct ReadioSyncState: Sendable {
    private(set) var serializedJSON: String

    mutating func stageLocal(entitiesJSON: String) throws {
        try reduce(eventJSON: #"{"type":"stage_local","entities":\#(entitiesJSON)}"#)
    }

    mutating func deleteLocal(entitiesJSON: String) throws {
        try reduce(eventJSON: #"{"type":"delete_local","entities":\#(entitiesJSON)}"#)
    }

    mutating func importRemote(changesJSON: String) throws {
        try reduce(eventJSON: #"{"type":"import_remote","changes":\#(changesJSON)}"#)
    }

    mutating func acknowledge(changeIDs: [String]) throws {
        let data = try JSONEncoder().encode(changeIDs)
        guard let changeIDsJSON = String(data: data, encoding: .utf8) else {
            throw ReadioSDKError.missingValue
        }
        try reduce(eventJSON: #"{"type":"acknowledge","change_ids":\#(changeIDsJSON)}"#)
    }

    func viewJSON() throws -> String {
        try serializedJSON.withCString { state in
            try consumeFFIResult(readio_sync_view(state))
        }
    }

    func queryLibrary(queryJSON: String) throws -> String {
        try serializedJSON.withCString { state in
            try queryJSON.withCString { query in
                try consumeFFIResult(readio_sync_query_library(state, query))
            }
        }
    }

    func libraryItem(itemID: String) throws -> String {
        try serializedJSON.withCString { state in
            try itemID.withCString { itemID in
                try consumeFFIResult(readio_sync_library_item(state, itemID))
            }
        }
    }

    func settingsJSON() throws -> String {
        try serializedJSON.withCString { state in
            try consumeFFIResult(readio_sync_settings(state))
        }
    }

    func bookmarksJSON(itemID: String) throws -> String {
        try serializedJSON.withCString { state in
            try itemID.withCString { itemID in
                try consumeFFIResult(readio_sync_bookmarks(state, itemID))
            }
        }
    }

    func readingPosition(itemID: String) throws -> String {
        try serializedJSON.withCString { state in
            try itemID.withCString { itemID in
                try consumeFFIResult(readio_sync_reading_position(state, itemID))
            }
        }
    }

    mutating func saveReadingPosition(
        itemID: String,
        mutationJSON: String
    ) throws -> String {
        let transitionJSON = try serializedJSON.withCString { state in
            try itemID.withCString { itemID in
                try mutationJSON.withCString { mutation in
                    try consumeFFIResult(
                        readio_sync_save_reading_position(state, itemID, mutation)
                    )
                }
            }
        }
        guard let transition = try JSONSerialization.jsonObject(
            with: Data(transitionJSON.utf8)
        ) as? [String: Any],
            let stateJSON = transition["stateJSON"] as? String,
            let response = transition["response"],
            JSONSerialization.isValidJSONObject(response)
        else {
            throw ReadioSDKError.missingValue
        }
        let responseData = try JSONSerialization.data(withJSONObject: response)
        guard let responseJSON = String(data: responseData, encoding: .utf8) else {
            throw ReadioSDKError.missingValue
        }
        serializedJSON = stateJSON
        return responseJSON
    }

    private mutating func reduce(eventJSON: String) throws {
        serializedJSON = try serializedJSON.withCString { state in
            try eventJSON.withCString { event in
                try consumeFFIResult(readio_sync_reduce(state, event))
            }
        }
    }
}

final class ReadioReplica {
    private let handle: OpaquePointer

    fileprivate init(handle: OpaquePointer) {
        self.handle = handle
    }

    deinit {
        readio_replica_free(handle)
    }

    func upsert(
        kind: ReadioReplicaEntityKind,
        entityID: String,
        payloadJSON: String
    ) throws -> String {
        try kind.rawValue.withCString { kind in
            try entityID.withCString { entityID in
                try payloadJSON.withCString { payload in
                    try consumeFFIResult(readio_replica_upsert(handle, kind, entityID, payload))
                }
            }
        }
    }

    func importChange(_ changeBase64: String) throws {
        try changeBase64.withCString { change in
            _ = try consumeFFIResult(readio_replica_import_change(handle, change))
        }
    }

    func stateJSON() throws -> String {
        try consumeFFIResult(readio_replica_state(handle))
    }

    func snapshotBase64() throws -> String {
        try consumeFFIResult(readio_replica_snapshot(handle))
    }

    func headsBase64() throws -> String {
        try consumeFFIResult(readio_replica_heads(handle))
    }
}

private func consumeFFIResult(_ result: ReadioFfiResult) throws -> String {
    defer {
        readio_string_free(result.value)
        readio_string_free(result.error)
    }

    if let error = result.error {
        throw ReadioSDKError.core(String(cString: error))
    }
    guard let value = result.value else {
        throw ReadioSDKError.missingValue
    }
    return String(cString: value)
}
