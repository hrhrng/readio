import Foundation

// MARK: - TTSService
/// Service layer for TTS (text-to-speech) synthesis operations.
///
/// The TTS system uses an asynchronous job queue model:
/// 1. **Create** a job via `createJob()` → receives a queued job
/// 2. **Poll** the job via `pollJob()` until status is terminal (completed/failed/cancelled)
/// 3. **Fetch audio** via `pollJob(includeAudio: true)` when completed
/// 4. **Cancel** via `cancelSession()` when navigating away
///
/// **Session management:**
/// Each playback session has a unique `session_id`. When the user switches items
/// or chapters, the old session is cancelled (bulk-cancelling all its pending jobs)
/// before starting a new one. This prevents stale audio from being synthesized.
///
/// **Prefetching:**
/// The client can proactively queue jobs for upcoming sentences with
/// `priority: .prefetch`. These are processed after all `priority: .user` jobs,
/// ensuring the current sentence is always synthesized first.
///
/// **Usage example:**
/// ```swift
/// let ttsService = TTSService()
///
/// // Create a job for the current sentence
/// let job = try await ttsService.createJob(params: TTSJobCreateParams(
///     sessionId: "session-123",
///     itemId: "doc-abc",
///     chapterId: "chapter1",
///     priority: .user,
///     provider: nil,
///     request: TTSRequest(text: "Hello world.", speed: 1.0, voice: "English_Trustworthy_Man")
/// ))
///
/// // Poll until complete
/// var status = job
/// while status.status == .queued || status.status == .running {
///     try await Task.sleep(for: .milliseconds(300))
///     status = try await ttsService.pollJob(id: status.jobId)
/// }
///
/// // Fetch the audio data
/// if status.status == .completed {
///     let withAudio = try await ttsService.pollJob(id: status.jobId, includeAudio: true)
///     // Decode withAudio.audioBase64 and play it
/// }
/// ```
final class TTSService: Sendable {
    /// The API client used for all HTTP requests.
    private let client: APIClient

    /// Creates a new TTS service instance.
    /// - Parameter client: The API client to use. Defaults to the shared singleton.
    init(client: APIClient = .shared) {
        self.client = client
    }

    // MARK: - Create Job

    /// Enqueues a new TTS synthesis job on the server.
    ///
    /// The server will:
    /// 1. Check its cache for a matching (text + voice + speed) result
    /// 2. If cached, return immediately with `status: .completed` and `cache_hit: true`
    /// 3. Otherwise, add to the work queue and return with `status: .queued`
    ///
    /// **Deduplication:** If a job with the same (session_id, item_id, chapter_id, text)
    /// already exists, the server returns the existing job instead of creating a duplicate.
    ///
    /// - Parameter params: Job creation parameters including text, voice, and priority.
    /// - Returns: The job status response (may already be completed if cached).
    /// - Throws: `APIError.httpError(400, ...)` for invalid parameters,
    ///           `APIError.httpError(503, ...)` if no TTS provider is available.
    func createJob(params: TTSJobCreateParams) async throws -> TTSJobStatusResponse {
        return try await client.post(APIEndpoints.ttsJobs, body: params)
    }

    // MARK: - Poll Job

    /// Polls the current status of a TTS job.
    ///
    /// Call this repeatedly (with a short delay) until the status is terminal:
    /// - `.completed` — audio is ready (set `includeAudio: true` to get it)
    /// - `.failed` — check the `error` field for the failure reason
    /// - `.cancelled` — job was cancelled by the user or session teardown
    ///
    /// **Performance note:** Avoid setting `includeAudio: true` until the job
    /// is actually completed. The base64-encoded audio data can be several
    /// hundred KB per sentence, so only fetch it when needed.
    ///
    /// - Parameters:
    ///   - id: The job ID returned from `createJob()`.
    ///   - includeAudio: When true and job is completed, includes `audio_base64` in the response.
    /// - Returns: Current job status with optional audio data.
    /// - Throws: `APIError.httpError(404, ...)` if the job doesn't exist (may have expired).
    func pollJob(id: String, includeAudio: Bool = false) async throws -> TTSJobStatusResponse {
        let path = includeAudio
            ? APIEndpoints.ttsJobWithAudio(id: id)
            : APIEndpoints.ttsJob(id: id)
        return try await client.get(path)
    }

    // MARK: - Cancel Session

    /// Cancels all pending TTS jobs for a session, with optional exceptions.
    ///
    /// This is called when the user navigates away from the current item or
    /// chapter. It bulk-cancels all queued/running jobs for the session to
    /// free up server resources.
    ///
    /// **Selective cancellation:** The `keepItemId` and `keepChapterId` parameters
    /// allow keeping jobs that match specific criteria. This is useful when
    /// switching chapters within the same item — you want to cancel jobs for
    /// the old chapter but keep any that were prefetched for the new one.
    ///
    /// - Parameters:
    ///   - id: The session ID to cancel jobs for.
    ///   - keepItemId: Optional item ID whose jobs should NOT be cancelled.
    ///   - keepChapterId: Optional chapter ID whose jobs should NOT be cancelled.
    /// - Throws: `APIError` on network or server failure.
    func cancelSession(id: String, keepItemId: String? = nil, keepChapterId: String? = nil) async throws {
        let body = CancelSessionBody(
            keepItemId: keepItemId,
            keepChapterId: keepChapterId
        )
        try await client.post(APIEndpoints.cancelSession(id: id), body: body)
    }

    // MARK: - Fetch Voices

    /// Fetches the list of all available TTS voices from the server.
    ///
    /// The response includes:
    /// - A sorted array of voices filtered to English and Chinese
    /// - The server's configured default voice ID
    ///
    /// **Caching:** Voice lists rarely change, so the view model should cache
    /// this response and only refresh it on explicit user action or app launch.
    ///
    /// - Returns: Voice list with default voice identifier.
    /// - Throws: `APIError` on network or server failure.
    func fetchVoices() async throws -> VoiceListResponse {
        return try await client.get(APIEndpoints.voices)
    }
}

// MARK: - Private Request Body Types

/// Request body for `POST /api/tts/sessions/{id}/cancel`.
/// Maps to the backend's `TTSSessionCancelRequest` Pydantic model.
private struct CancelSessionBody: Encodable {
    let keepItemId: String?
    let keepChapterId: String?

    enum CodingKeys: String, CodingKey {
        case keepItemId = "keep_item_id"
        case keepChapterId = "keep_chapter_id"
    }
}
