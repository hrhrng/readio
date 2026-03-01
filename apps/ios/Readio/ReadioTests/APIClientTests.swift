import XCTest
@testable import Readio

// ---------------------------------------------------------------------------
// MARK: - APIClient unit tests
// Validates URL construction, JSON coding strategies, and error mapping.
// ---------------------------------------------------------------------------

final class APIClientTests: XCTestCase {

    // MARK: - Endpoint path construction

    /// Verify that `APIEndpoints.libraryItems` builds correct query strings
    /// when multiple filter/pagination parameters are provided.
    func testLibraryItemsEndpointWithParams() {
        let path = APIEndpoints.libraryItems(
            category: "imported",
            search: "swift",
            type: "epub",
            page: 2,
            pageSize: 10,
            sortBy: "title",
            sortOrder: "asc"
        )

        // Should start with the base path
        XCTAssertTrue(path.hasPrefix("/api/library/items"))

        // Should contain all provided query parameters
        XCTAssertTrue(path.contains("category=imported"), "Missing category param")
        XCTAssertTrue(path.contains("search=swift"), "Missing search param")
        XCTAssertTrue(path.contains("type=epub"), "Missing type param")
        XCTAssertTrue(path.contains("page=2"), "Missing page param")
        XCTAssertTrue(path.contains("page_size=10"), "Missing page_size param")
        XCTAssertTrue(path.contains("sort_by=title"), "Missing sort_by param")
        XCTAssertTrue(path.contains("sort_order=asc"), "Missing sort_order param")
    }

    /// Verify endpoint with no optional params produces a clean base path.
    func testLibraryItemsEndpointNoParams() {
        let path = APIEndpoints.libraryItems()
        XCTAssertEqual(path, "/api/library/items")
    }

    /// Verify item-specific endpoints include the ID.
    func testItemSpecificEndpoints() {
        let id = "doc-abc123"
        XCTAssertEqual(APIEndpoints.libraryItem(id: id), "/api/library/items/doc-abc123")
        XCTAssertEqual(APIEndpoints.updateProgress(id: id), "/api/library/items/doc-abc123/progress")
        XCTAssertEqual(APIEndpoints.updateVoice(id: id), "/api/library/items/doc-abc123/voice")
        XCTAssertEqual(APIEndpoints.updateSpeed(id: id), "/api/library/items/doc-abc123/speed")
        XCTAssertEqual(APIEndpoints.updateChapter(id: id), "/api/library/items/doc-abc123/chapter")
    }

    /// Verify TTS endpoints.
    func testTTSEndpoints() {
        XCTAssertEqual(APIEndpoints.ttsJobs, "/api/tts/jobs")
        XCTAssertEqual(APIEndpoints.ttsJob(id: "job-1"), "/api/tts/jobs/job-1")
        XCTAssertEqual(APIEndpoints.ttsJobWithAudio(id: "job-1"), "/api/tts/jobs/job-1?include_audio=true")
        XCTAssertEqual(APIEndpoints.cancelSession(id: "sess-1"), "/api/tts/sessions/sess-1/cancel")
        XCTAssertEqual(APIEndpoints.voices, "/api/tts/voices")
    }

    // MARK: - JSON decoding

    /// Verify that the JSON decoder handles snake_case → camelCase conversion
    /// correctly for a `LibraryItemSummary` response.
    func testSnakeCaseDecoding() throws {
        let json = """
        {
            "id": "doc-001",
            "title": "Test Book",
            "type": "epub",
            "progress": 42,
            "date": "2025-01-15T10:30:00Z",
            "category": "imported",
            "folder_id": "f1",
            "source": "upload:test.epub",
            "file_path": "files/test.epub",
            "cover_image": null,
            "voice": "English_calm_woman",
            "speed": 1.25,
            "content_format": "json"
        }
        """.data(using: .utf8)!

        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase

        let item = try decoder.decode(LibraryItemSummary.self, from: json)
        XCTAssertEqual(item.id, "doc-001")
        XCTAssertEqual(item.title, "Test Book")
        XCTAssertEqual(item.type, .epub)
        XCTAssertEqual(item.progress, 42)
        XCTAssertEqual(item.folderId, "f1")
        XCTAssertEqual(item.speed, 1.25)
        XCTAssertEqual(item.contentFormat, "json")
    }

    /// Verify that paginated response wrapper decodes correctly.
    func testPaginatedResponseDecoding() throws {
        let json = """
        {
            "items": [],
            "total": 0,
            "page": 1,
            "page_size": 20,
            "total_pages": 0
        }
        """.data(using: .utf8)!

        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase

        let response = try decoder.decode(LibraryItemsResponse.self, from: json)
        XCTAssertEqual(response.items.count, 0)
        XCTAssertEqual(response.total, 0)
        XCTAssertEqual(response.page, 1)
        XCTAssertEqual(response.pageSize, 20)
        XCTAssertEqual(response.totalPages, 0)
    }

    /// Verify TTS job status decoding with all optional fields present.
    func testTTSJobStatusDecoding() throws {
        let json = """
        {
            "job_id": "job-xyz",
            "session_id": "sess-123",
            "item_id": "doc-001",
            "chapter_id": "main",
            "priority": "user",
            "status": "completed",
            "provider": "minimax",
            "trace_id": "trace-456",
            "duration_ms": 3200.5,
            "sample_rate": 24000,
            "error": null,
            "cache_hit": true,
            "created_at_ms": 1700000000000,
            "started_at_ms": 1700000000100,
            "finished_at_ms": 1700000003300,
            "audio_base64": "SGVsbG8="
        }
        """.data(using: .utf8)!

        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase

        let status = try decoder.decode(TTSJobStatusResponse.self, from: json)
        XCTAssertEqual(status.jobId, "job-xyz")
        XCTAssertEqual(status.status, .completed)
        XCTAssertEqual(status.priority, .user)
        XCTAssertTrue(status.cacheHit)
        XCTAssertEqual(status.durationMs, 3200.5)
        XCTAssertEqual(status.audioBase64, "SGVsbG8=")
    }

    // MARK: - ReadingStatus computation

    func testReadingStatus() throws {
        let json = """
        {
            "id": "a", "title": "T", "content": "text",
            "type": "txt", "progress": 0, "date": "2025-01-01",
            "category": "imported", "folder_id": "f1", "source": "test"
        }
        """.data(using: .utf8)!
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase

        var item = try decoder.decode(LibraryItem.self, from: json)
        XCTAssertEqual(item.readingStatus, .new)

        // Manually adjust progress to test other states
        // (In production, progress comes from the server)
        let json50 = json.replacing("\"progress\": 0", with: "\"progress\": 50")
        item = try decoder.decode(LibraryItem.self, from: json50)
        XCTAssertEqual(item.readingStatus, .reading)

        let json100 = json.replacing("\"progress\": 0", with: "\"progress\": 100")
        item = try decoder.decode(LibraryItem.self, from: json100)
        XCTAssertEqual(item.readingStatus, .finished)
    }
}

// Helper extension for test data manipulation
private extension Data {
    func replacing(_ target: String, with replacement: String) -> Data {
        let str = String(data: self, encoding: .utf8)!
        return str.replacingOccurrences(of: target, with: replacement).data(using: .utf8)!
    }
}
