import Foundation

// MARK: - LibraryService
/// Service layer for all library CRUD operations against the Readio backend.
///
/// Encapsulates the `APIClient` calls with domain-specific method signatures,
/// hiding endpoint path construction and request body formatting from the
/// view model layer.
///
/// **Usage in view models:**
/// ```swift
/// let service = LibraryService()
/// let response = try await service.fetchItems(page: 1, pageSize: 20)
/// let item = try await service.fetchItem(id: "doc-abc123")
/// ```
///
/// **Thread safety:**
/// All methods are async and can be called from any actor context. The
/// underlying `APIClient` uses `URLSession` which manages its own dispatch.
final class LibraryService: Sendable {
    /// The API client used for all HTTP requests.
    private let client: APIClient

    /// Creates a new library service instance.
    /// - Parameter client: The API client to use. Defaults to the shared singleton.
    init(client: APIClient = .shared) {
        self.client = client
    }

    // MARK: - Fetch Items (List)

    /// Fetches a paginated list of library items matching the given filters.
    ///
    /// This returns `LibraryItemSummary` objects (without content) for efficient
    /// list rendering. Use `fetchItem(id:)` to get the full content when the
    /// user opens an item.
    ///
    /// - Parameters:
    ///   - category: Filter by category ("imported" or "podcasts"). Nil = all categories.
    ///   - search: Full-text search query. Nil = no search filter.
    ///   - folderId: Filter by folder. Nil = all folders.
    ///   - type: Filter by document type ("web", "txt", "pdf", "epub"). Nil = all types.
    ///   - page: 1-based page number. Default = 1.
    ///   - pageSize: Items per page (1–100). Default = 20.
    ///   - sortBy: Sort field (e.g., "created_at", "title"). Default = "created_at".
    ///   - sortOrder: Sort direction ("asc" or "desc"). Default = "desc".
    ///   - progressMin: Minimum progress percentage. Nil = no lower bound.
    ///   - progressMax: Maximum progress percentage. Nil = no upper bound.
    /// - Returns: Paginated response with items and pagination metadata.
    /// - Throws: `APIError` on network or server failure.
    func fetchItems(
        category: String? = nil,
        search: String? = nil,
        folderId: String? = nil,
        type: String? = nil,
        page: Int? = nil,
        pageSize: Int? = nil,
        sortBy: String? = nil,
        sortOrder: String? = nil,
        progressMin: Int? = nil,
        progressMax: Int? = nil
    ) async throws -> LibraryItemsResponse {
        let path = APIEndpoints.libraryItems(
            category: category,
            search: search,
            folderId: folderId,
            type: type,
            page: page,
            pageSize: pageSize,
            sortBy: sortBy,
            sortOrder: sortOrder,
            progressMin: progressMin,
            progressMax: progressMax
        )
        return try await client.get(path)
    }

    // MARK: - Fetch Single Item

    /// Fetches the full details of a single library item, including its content.
    ///
    /// The `content` field can be very large for full documents. For EPUB items,
    /// it contains a JSON-encoded array of chapters with structured content blocks.
    ///
    /// - Parameter id: The library item's unique identifier.
    /// - Returns: The full library item with content.
    /// - Throws: `APIError.httpError(404, ...)` if the item doesn't exist.
    func fetchItem(id: String) async throws -> LibraryItem {
        return try await client.get(APIEndpoints.libraryItem(id: id))
    }

    // MARK: - Delete Item

    /// Deletes a library item and its associated file (if any) from the server.
    ///
    /// This is a destructive, non-reversible operation. The backend will:
    /// 1. Remove the database record
    /// 2. Delete the associated file from disk (EPUB/PDF)
    ///
    /// - Parameter id: The library item's unique identifier.
    /// - Throws: `APIError.httpError(404, ...)` if the item doesn't exist.
    func deleteItem(id: String) async throws {
        try await client.delete(APIEndpoints.libraryItem(id: id))
    }

    // MARK: - Update Progress

    /// Updates the reading progress for a library item.
    ///
    /// Progress is an integer percentage (0–100). The backend validates the range.
    /// This is typically called:
    /// - When the user manually scrolls to a new position
    /// - Periodically during TTS playback to save the user's position
    /// - When the user finishes reading (progress = 100)
    ///
    /// - Parameters:
    ///   - id: The library item's unique identifier.
    ///   - progress: New progress value (0–100).
    /// - Throws: `APIError.httpError(404, ...)` if the item doesn't exist.
    func updateProgress(id: String, progress: Int) async throws {
        try await client.patch(
            APIEndpoints.updateProgress(id: id),
            body: ["progress": progress]
        )
    }

    // MARK: - Update Voice

    /// Updates the per-item TTS voice override.
    ///
    /// When set, this voice is used instead of the user's default voice for
    /// TTS playback of this specific item. Pass nil to clear the override
    /// and revert to the default voice.
    ///
    /// - Parameters:
    ///   - id: The library item's unique identifier.
    ///   - voice: Voice ID string (e.g., "English_Trustworthy_Man") or nil to clear.
    /// - Throws: `APIError` on failure.
    func updateVoice(id: String, voice: String?) async throws {
        try await client.patch(
            APIEndpoints.updateVoice(id: id),
            dictionary: ["voice": voice]
        )
    }

    // MARK: - Update Speed

    /// Updates the per-item TTS playback speed override.
    ///
    /// When set, this speed multiplier is used instead of the user's default
    /// speed for this specific item. Pass nil to clear the override.
    ///
    /// - Parameters:
    ///   - id: The library item's unique identifier.
    ///   - speed: Speed multiplier (e.g., 1.0, 1.5, 2.0) or nil to clear.
    /// - Throws: `APIError` on failure.
    func updateSpeed(id: String, speed: Double?) async throws {
        // Use the SpeedUpdate struct because speed is a Double? (not String?),
        // so we can't use the dictionary-based patch method.
        try await client.patch(
            APIEndpoints.updateSpeed(id: id),
            body: SpeedUpdate(speed: speed)
        )
    }

    // MARK: - Update Chapter

    /// Updates the current chapter bookmark for an EPUB item.
    ///
    /// This saves the user's position so they can resume reading from the
    /// same chapter. The chapter ID corresponds to `EpubChapter.id`.
    ///
    /// - Parameters:
    ///   - id: The library item's unique identifier.
    ///   - chapter: Chapter ID (e.g., "chapter3.xhtml") or nil to clear.
    /// - Throws: `APIError` on failure.
    func updateChapter(id: String, chapter: String?) async throws {
        try await client.patch(
            APIEndpoints.updateChapter(id: id),
            dictionary: ["chapter": chapter]
        )
    }

    // MARK: - Import File

    /// Imports a document from raw file data via multipart upload.
    ///
    /// The backend will:
    /// 1. Detect the file format from the filename extension
    /// 2. Extract text content (parse PDF, EPUB, etc.)
    /// 3. Save the original file for binary formats (EPUB/PDF)
    /// 4. Create a new library item record
    ///
    /// **Size limits:** The backend enforces a configurable max file size
    /// (default 50MB). Oversized files receive a 413 response.
    ///
    /// - Parameters:
    ///   - data: Raw bytes of the file to upload.
    ///   - filename: Original filename with extension (e.g., "book.epub").
    ///   - folderId: Target folder ID. Default = "f1".
    ///   - category: Item category. Default = "imported".
    ///   - title: Optional title override. If nil, the backend extracts from the document.
    /// - Returns: The newly created library item.
    /// - Throws: `APIError.httpError(400, ...)` for unsupported formats,
    ///           `APIError.httpError(413, ...)` for oversized files.
    func importFile(
        data: Data,
        filename: String,
        folderId: String = "f1",
        category: String = "imported",
        title: String? = nil
    ) async throws -> LibraryItem {
        var fields: [String: String] = [
            "folder_id": folderId,
            "category": category,
        ]
        if let title {
            fields["title"] = title
        }

        return try await client.uploadMultipart(
            APIEndpoints.importFile,
            fileData: data,
            filename: filename,
            fields: fields
        )
    }

    // MARK: - Import URL

    /// Imports a document by fetching content from a URL.
    ///
    /// The backend will:
    /// 1. Fetch the URL content
    /// 2. Extract readable text using its content extraction pipeline
    /// 3. Create a new library item with the extracted content
    ///
    /// - Parameters:
    ///   - url: The web URL to import content from.
    ///   - folderId: Target folder ID. Default = "f1".
    ///   - category: Item category. Default = "imported".
    ///   - title: Optional title override. If nil, extracted from the page.
    /// - Returns: The newly created library item.
    /// - Throws: `APIError.httpError(400, ...)` if no readable content found,
    ///           `APIError.httpError(502, ...)` if the URL can't be fetched.
    func importURL(
        url: String,
        folderId: String = "f1",
        category: String = "imported",
        title: String? = nil
    ) async throws -> LibraryItem {
        let body = ImportURLBody(
            url: url,
            folderId: folderId,
            category: category,
            title: title
        )
        return try await client.post(APIEndpoints.importURL, body: body)
    }
}

// MARK: - Private Request Body Types
// These small structs exist solely for JSON serialization of PATCH/POST bodies.
// They're private to this file because no other code needs them.

/// Request body for `PATCH /api/library/items/{id}/speed`.
/// Separate struct needed because the speed value can be nil (JSON null).
private struct SpeedUpdate: Encodable {
    let speed: Double?
}

/// Request body for `POST /api/library/import/url`.
/// Maps to the backend's `ImportUrlRequest` Pydantic model.
private struct ImportURLBody: Encodable {
    let url: String
    let folderId: String
    let category: String
    let title: String?

    enum CodingKeys: String, CodingKey {
        case url
        case folderId = "folder_id"
        case category
        case title
    }
}
