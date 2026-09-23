import Foundation

enum AppleBookmarkAffinity: String, Codable, Equatable, Sendable {
    case forward
    case backward
}

struct AppleBookmark: Codable, Equatable, Identifiable, Sendable {
    let id: String
    let itemId: String
    let charOffset: UInt64
    let affinity: AppleBookmarkAffinity
    let note: String
    let createdAtUnixMilliseconds: Int64
    let updatedAtUnixMilliseconds: Int64
    let version: UInt64

    enum CodingKeys: String, CodingKey {
        case id
        case itemId = "item_id"
        case charOffset = "char_offset"
        case affinity
        case note
        case createdAtUnixMilliseconds = "created_at_unix_ms"
        case updatedAtUnixMilliseconds = "updated_at_unix_ms"
        case version
    }
}

struct AppleBookmarkUpsertRequest: Codable, Equatable, Sendable {
    let bookmarkId: String
    let itemId: String
    let charOffset: UInt64
    let affinity: AppleBookmarkAffinity
    let note: String
    let createdAtUnixMilliseconds: Int64
    let updatedAtUnixMilliseconds: Int64
    let baseVersion: UInt64

    enum CodingKeys: String, CodingKey {
        case bookmarkId = "bookmark_id"
        case itemId = "item_id"
        case charOffset = "char_offset"
        case affinity
        case note
        case createdAtUnixMilliseconds = "created_at_unix_ms"
        case updatedAtUnixMilliseconds = "updated_at_unix_ms"
        case baseVersion = "base_version"
    }
}

struct AppleBookmarkTransition: Codable, Equatable, Sendable {
    let bookmark: AppleBookmark
    let created: Bool
}

/// Typed Apple façade that commits only transitions produced by Rust.
struct AppleBookmarkService: Sendable {
    private let localStore: AppleLocalReplicaStore
    private let sdk = ReadioSDK()

    init(localStore: AppleLocalReplicaStore = .shared) {
        self.localStore = localStore
    }

    func list(itemID: String) async throws -> [AppleBookmark] {
        let json = try await localStore.bookmarksJSON(itemID: itemID)
        return try JSONDecoder().decode([AppleBookmark].self, from: Data(json.utf8))
    }

    func upsert(
        documentJSON: String,
        request: AppleBookmarkUpsertRequest
    ) async throws -> AppleBookmarkTransition {
        let bookmarksJSON = String(
            decoding: try JSONEncoder().encode(try await list(itemID: request.itemId)),
            as: UTF8.self
        )
        let requestJSON = String(
            decoding: try JSONEncoder().encode(request),
            as: UTF8.self
        )
        let transitionJSON = try sdk.upsertBookmark(
            documentJSON: documentJSON,
            bookmarksJSON: bookmarksJSON,
            requestJSON: requestJSON
        )
        let transition = try JSONDecoder().decode(
            AppleBookmarkTransition.self,
            from: Data(transitionJSON.utf8)
        )
        let payload = try JSONSerialization.jsonObject(
            with: JSONEncoder().encode(transition.bookmark)
        )
        let entity = [[
            "entityKind": "bookmark",
            "entityId": transition.bookmark.id,
            "payload": payload,
        ] as [String: Any]]
        try await localStore.stageLocal(
            entitiesJSON: String(
                decoding: try JSONSerialization.data(withJSONObject: entity),
                as: UTF8.self
            )
        )
        return transition
    }

    func delete(bookmarkID: String) async throws {
        let entity = [[
            "entityKind": "bookmark",
            "entityId": bookmarkID,
        ]]
        try await localStore.deleteLocal(
            entitiesJSON: String(
                decoding: try JSONSerialization.data(withJSONObject: entity),
                as: UTF8.self
            )
        )
    }
}
