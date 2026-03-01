import Foundation

// MARK: - LibraryItemSummary
/// Lightweight library item model used in paginated list responses.
///
/// Identical to `LibraryItem` but **without the `content` field**. The list
/// endpoint `GET /api/library/items` returns these to avoid transferring
/// potentially megabytes of document text for items that are only displayed
/// as cards/rows in the library grid.
///
/// **When to use which model:**
/// - `LibraryItemSummary` — library list, search results, recent items
/// - `LibraryItem` — reader/player screen where full content is needed
///
/// The `content_format` field is included here (unlike `current_chapter`)
/// so the list UI can show format-specific badges or icons without fetching
/// the full item.
struct LibraryItemSummary: Codable, Identifiable, Sendable {
    let id: String
    let title: String
    let type: LibraryItemType
    let progress: Int
    let date: String
    let category: LibraryItemCategory
    let folderId: String
    let source: String
    let filePath: String?
    let coverImage: String?
    let voice: String?
    let speed: Double?
    let contentFormat: String?

    // MARK: - Computed Properties

    /// Derives reading status from progress, same logic as `LibraryItem.readingStatus`.
    var readingStatus: ReadingStatus {
        if progress <= 0 { return .new_ }
        if progress >= 100 { return .finished }
        return .reading
    }

    // MARK: - CodingKeys
    // Maps snake_case JSON keys from the API to idiomatic Swift camelCase.
    enum CodingKeys: String, CodingKey {
        case id
        case title
        case type
        case progress
        case date
        case category
        case folderId = "folder_id"
        case source
        case filePath = "file_path"
        case coverImage = "cover_image"
        case voice
        case speed
        case contentFormat = "content_format"
    }
}
