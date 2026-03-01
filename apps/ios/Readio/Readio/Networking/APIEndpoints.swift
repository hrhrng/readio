import Foundation

// MARK: - APIEndpoints
/// Type-safe endpoint path builders for all Readio API routes.
///
/// Centralizes URL path construction so that:
/// 1. Endpoint paths are defined in one place (single source of truth)
/// 2. Parameter encoding is handled consistently
/// 3. Typos in URL strings are caught at compile time
///
/// **Convention:**
/// - Static computed properties for parameterless endpoints
/// - Static methods for endpoints requiring path parameters or query strings
/// - All paths start with "/api/" to match the FastAPI router prefix
///
/// **Relationship to backend routes:**
/// Each endpoint here corresponds to a `@router.get/post/patch/delete` handler
/// in `apps/api/app/api/routes.py`. The path strings must match exactly.
enum APIEndpoints {

    // MARK: - Library Items

    /// Builds the path for `GET /api/library/items` with optional query parameters.
    ///
    /// All parameters are optional; only non-nil values are included in the query string.
    /// The backend defaults to: page=1, pageSize=20, sortBy="created_at", sortOrder="desc".
    ///
    /// - Parameters:
    ///   - category: Filter by category ("imported" or "podcasts")
    ///   - search: Full-text search query across title and content
    ///   - folderId: Filter by folder ID (e.g., "f1")
    ///   - type: Filter by document type ("web", "txt", "pdf", "epub")
    ///   - page: 1-based page number
    ///   - pageSize: Items per page (1–100)
    ///   - sortBy: Sort field (e.g., "created_at", "title", "progress")
    ///   - sortOrder: Sort direction ("asc" or "desc")
    ///   - progressMin: Minimum progress filter (0–100)
    ///   - progressMax: Maximum progress filter (0–100)
    /// - Returns: URL path with query string (e.g., "/api/library/items?page=1&page_size=20")
    static func libraryItems(
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
    ) -> String {
        var components = URLComponents(string: "/api/library/items")!
        var queryItems: [URLQueryItem] = []

        // Append each non-nil parameter as a query item.
        // Using a helper to keep the code DRY.
        if let category { queryItems.append(URLQueryItem(name: "category", value: category)) }
        if let search { queryItems.append(URLQueryItem(name: "search", value: search)) }
        if let folderId { queryItems.append(URLQueryItem(name: "folder_id", value: folderId)) }
        if let type { queryItems.append(URLQueryItem(name: "type", value: type)) }
        if let page { queryItems.append(URLQueryItem(name: "page", value: String(page))) }
        if let pageSize { queryItems.append(URLQueryItem(name: "page_size", value: String(pageSize))) }
        if let sortBy { queryItems.append(URLQueryItem(name: "sort_by", value: sortBy)) }
        if let sortOrder { queryItems.append(URLQueryItem(name: "sort_order", value: sortOrder)) }
        if let progressMin { queryItems.append(URLQueryItem(name: "progress_min", value: String(progressMin))) }
        if let progressMax { queryItems.append(URLQueryItem(name: "progress_max", value: String(progressMax))) }

        if !queryItems.isEmpty {
            components.queryItems = queryItems
        }

        // URLComponents.string percent-encodes special characters automatically.
        return components.string ?? "/api/library/items"
    }

    /// Path for fetching a single library item's full details (including content).
    /// `GET /api/library/items/{id}`
    static func libraryItem(id: String) -> String {
        "/api/library/items/\(id)"
    }

    /// Path for updating a library item's reading progress.
    /// `PATCH /api/library/items/{id}/progress`
    /// Body: `{"progress": 42}`
    static func updateProgress(id: String) -> String {
        "/api/library/items/\(id)/progress"
    }

    /// Path for updating a library item's TTS voice override.
    /// `PATCH /api/library/items/{id}/voice`
    /// Body: `{"voice": "English_Trustworthy_Man"}` or `{"voice": null}`
    static func updateVoice(id: String) -> String {
        "/api/library/items/\(id)/voice"
    }

    /// Path for updating a library item's TTS speed override.
    /// `PATCH /api/library/items/{id}/speed`
    /// Body: `{"speed": 1.5}` or `{"speed": null}`
    static func updateSpeed(id: String) -> String {
        "/api/library/items/\(id)/speed"
    }

    /// Path for updating a library item's current chapter (EPUB bookmark).
    /// `PATCH /api/library/items/{id}/chapter`
    /// Body: `{"chapter": "chapter3.xhtml"}` or `{"chapter": null}`
    static func updateChapter(id: String) -> String {
        "/api/library/items/\(id)/chapter"
    }

    // MARK: - Library Import

    /// Path for importing a document via file upload (multipart form POST).
    /// `POST /api/library/import/file`
    /// Accepts: file, category, folder_id, title, cover_image
    static var importFile: String {
        "/api/library/import/file"
    }

    /// Path for importing a document from a URL.
    /// `POST /api/library/import/url`
    /// Body: `{"url": "https://...", "category": "imported", "folder_id": "f1"}`
    static var importURL: String {
        "/api/library/import/url"
    }

    // MARK: - TTS Jobs

    /// Path for creating a new TTS synthesis job.
    /// `POST /api/tts/jobs`
    static var ttsJobs: String {
        "/api/tts/jobs"
    }

    /// Path for polling a TTS job's status (without audio data).
    /// `GET /api/tts/jobs/{id}`
    static func ttsJob(id: String) -> String {
        "/api/tts/jobs/\(id)"
    }

    /// Path for polling a TTS job's status WITH the audio data included.
    /// `GET /api/tts/jobs/{id}?include_audio=true`
    ///
    /// Only use this when the job status is `.completed` and you need the audio.
    /// The audio_base64 field can be very large, so avoid requesting it unnecessarily.
    static func ttsJobWithAudio(id: String) -> String {
        "/api/tts/jobs/\(id)?include_audio=true"
    }

    /// Path for cancelling all pending jobs in a TTS session.
    /// `POST /api/tts/sessions/{id}/cancel`
    /// Body: `{"keep_item_id": "...", "keep_chapter_id": "..."}`
    static func cancelSession(id: String) -> String {
        "/api/tts/sessions/\(id)/cancel"
    }

    // MARK: - TTS Voices

    /// Path for listing all available TTS voices.
    /// `GET /api/tts/voices`
    static var voices: String {
        "/api/tts/voices"
    }

    // MARK: - User Settings

    /// Path for reading/updating user settings (key-value store).
    /// `GET /api/settings` — returns all settings as a JSON object
    /// `PATCH /api/settings` — updates specific keys
    static var settings: String {
        "/api/settings"
    }
}
