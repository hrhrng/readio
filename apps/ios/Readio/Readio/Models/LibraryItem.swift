import Foundation

// MARK: - LibraryItemType
// Represents the original document format. The backend uses this to determine
// how content should be parsed, rendered, and served to the frontend.
// - web: content scraped from a URL (may contain HTML)
// - txt: plain text document
// - pdf: PDF file (binary stored on server, rendered via file endpoint)
// - epub: EPUB e-book (binary stored on server, chapters parsed into blocks)
enum LibraryItemType: String, Codable, CaseIterable, Sendable {
    case web
    case txt
    case pdf
    case epub
}

// MARK: - LibraryItemCategory
// Organizational category for library items. Currently the backend supports
// two top-level categories; items default to "imported" when added via file
// upload or URL import.
enum LibraryItemCategory: String, Codable, CaseIterable, Sendable {
    case imported
    case podcasts
}

// MARK: - ReadingStatus
// Derived from the integer `progress` field (0–100). This is a convenience
// enum used for UI display (badges, filters, sorting) rather than stored
// directly in the database.
enum ReadingStatus: String, Sendable {
    /// The item has never been opened (progress == 0)
    case new_ = "new"
    /// The item is partially read (0 < progress < 100)
    case reading
    /// The item has been fully read (progress >= 100)
    case finished
}

// MARK: - LibraryItem
/// Full library item model returned by the detail endpoint `GET /api/library/items/{id}`.
///
/// This includes the `content` field which can be very large for full documents.
/// For list views, use `LibraryItemSummary` instead to avoid transferring content
/// that won't be displayed in a table/grid.
///
/// **API mapping:**
/// - `id`: Server-generated unique identifier (e.g., "doc-a1b2c3d4e5f6")
/// - `content`: The full document text or structured JSON (for EPUB chapters)
/// - `type`: Document format — determines which reader view to use
/// - `progress`: Integer 0–100 representing reading completion percentage
/// - `date`: ISO 8601 string of when the item was created/imported
/// - `category`: Organizational bucket (imported vs. podcasts)
/// - `folder_id`: ID of the folder containing this item (default "f1")
/// - `source`: Origin URL or "upload:filename.ext" or "Manual Input"
/// - `file_path`: Relative server path to the original binary file (EPUB/PDF only)
/// - `cover_image`: Base64 data URL or remote URL for the cover image
/// - `voice`: TTS voice ID override for this specific item (nil = use default)
/// - `speed`: TTS playback speed override (nil = use default, e.g. 1.0)
/// - `current_chapter`: ID of the chapter the user was last reading (EPUB only)
/// - `content_format`: Hint about content encoding (e.g., "html" for rich web content)
struct LibraryItem: Codable, Identifiable, Sendable {
    let id: String
    let title: String
    let content: String
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
    let currentChapter: String?
    let contentFormat: String?

    // MARK: - Computed Properties

    /// Derives a high-level reading status from the raw progress percentage.
    /// Mirrors the TypeScript `getReadingStatus()` helper in the web frontend:
    ///   - progress == 0     → .new_
    ///   - 0 < progress < 100 → .reading
    ///   - progress >= 100   → .finished
    var readingStatus: ReadingStatus {
        if progress <= 0 { return .new_ }
        if progress >= 100 { return .finished }
        return .reading
    }

    // MARK: - CodingKeys
    // The API uses snake_case JSON keys; we map them to Swift camelCase properties.
    enum CodingKeys: String, CodingKey {
        case id
        case title
        case content
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
        case currentChapter = "current_chapter"
        case contentFormat = "content_format"
    }
}
