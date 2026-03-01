import Foundation

// MARK: - TTSJobPriority
/// Priority level for TTS synthesis jobs in the server's work queue.
///
/// The backend TTS job manager uses priority to order its internal queue:
/// - `user`: Triggered by explicit user action (tap play, seek to sentence).
///   These are processed immediately and jump ahead of prefetch jobs.
/// - `prefetch`: Proactively queued by the client to pre-render upcoming
///   sentences/chunks so playback feels seamless. Lower priority than user jobs.
enum TTSJobPriority: String, Codable, Sendable {
    case user
    case prefetch
}

// MARK: - TTSJobStatus
/// Lifecycle status of a TTS synthesis job on the server.
///
/// Jobs follow this state machine:
///   queued → running → completed
///                    → failed
///   queued → cancelled
///   running → cancelled
///
/// The client polls `GET /api/tts/jobs/{id}` until status is a terminal state
/// (completed, failed, or cancelled), then either plays the audio or shows an error.
enum TTSJobStatus: String, Codable, Sendable {
    /// Job is waiting in the queue to be picked up by a worker.
    case queued
    /// Job is actively being synthesized by a TTS provider.
    case running
    /// Synthesis succeeded; audio data is available (request with `include_audio=true`).
    case completed
    /// Synthesis failed; the `error` field contains the failure reason.
    case failed
    /// Job was cancelled before completion (by user navigation or session teardown).
    case cancelled
}

// MARK: - TTSRequest
/// The text-to-speech synthesis parameters embedded within a job creation request.
///
/// This matches the backend's `TTSRequest` Pydantic model. Only `text` is required;
/// `voice` and `speed` fall back to server defaults when nil.
struct TTSRequest: Codable, Sendable {
    /// The text content to synthesize into audio. Must be non-empty.
    let text: String

    /// TTS playback speed multiplier (e.g., 1.0 = normal, 1.5 = 50% faster).
    /// When nil, the server uses its configured default speed.
    let speed: Double?

    /// Voice identifier to use for synthesis (e.g., "English_Trustworthy_Man").
    /// When nil, the server uses the default voice from settings.
    let voice: String?
}

// MARK: - TTSJobCreateParams
/// Request body for `POST /api/tts/jobs` to enqueue a new TTS synthesis job.
///
/// **Session model:** Each playback session has a unique `session_id`. When the
/// user navigates away or starts reading a different item, the client cancels
/// the old session via `POST /api/tts/sessions/{session_id}/cancel`, which
/// bulk-cancels all pending jobs for that session.
///
/// **Deduplication:** The backend deduplicates jobs by (session_id, item_id,
/// chapter_id, text hash), returning the existing job if one matches.
struct TTSJobCreateParams: Codable, Sendable {
    /// Unique identifier for the current playback session. Used for bulk cancellation.
    let sessionId: String

    /// Library item ID this job belongs to.
    let itemId: String

    /// Chapter identifier within the item (use "default" for non-EPUB items).
    let chapterId: String

    /// Processing priority — "user" jobs are served before "prefetch" jobs.
    let priority: TTSJobPriority

    /// Optional TTS provider override (e.g., "minimax", "edge", "elevenlabs").
    /// When nil, the server uses its fallback chain.
    let provider: String?

    /// The synthesis parameters: text, optional voice, and optional speed.
    let request: TTSRequest

    // MARK: - CodingKeys
    enum CodingKeys: String, CodingKey {
        case sessionId = "session_id"
        case itemId = "item_id"
        case chapterId = "chapter_id"
        case priority
        case provider
        case request
    }
}

// MARK: - TTSJobStatusResponse
/// Response from `GET /api/tts/jobs/{id}` and `POST /api/tts/jobs`.
///
/// Contains the full lifecycle state of a TTS job. When polling for completion:
/// 1. Create a job → receive this response with status = .queued
/// 2. Poll `GET /api/tts/jobs/{id}` until status is .completed or .failed
/// 3. If completed, poll again with `?include_audio=true` to get `audio_base64`
///
/// **Timing fields:** All `*_ms` timestamps are Unix epoch milliseconds (not seconds),
/// matching JavaScript's `Date.now()` convention used by the web frontend.
struct TTSJobStatusResponse: Codable, Identifiable, Sendable {
    /// Server-generated unique job identifier.
    let jobId: String

    /// Session this job belongs to (for bulk cancellation).
    let sessionId: String

    /// Library item this job is synthesizing audio for.
    let itemId: String

    /// Chapter within the item (or "default" for non-EPUB content).
    let chapterId: String

    /// Job priority level.
    let priority: TTSJobPriority

    /// Current lifecycle status of the job.
    let status: TTSJobStatus

    /// TTS provider that handled/is handling this job (e.g., "minimax", "edge").
    let provider: String?

    /// Provider-specific trace ID for debugging/logging.
    let traceId: String?

    /// Duration of the generated audio in milliseconds (available after completion).
    let durationMs: Double?

    /// Sample rate of the generated audio in Hz (e.g., 32000).
    let sampleRate: Int?

    /// Error message if the job failed.
    let error: String?

    /// Whether this result was served from the server's in-memory cache.
    let cacheHit: Bool

    /// Unix timestamp (ms) when the job was created/enqueued.
    let createdAtMs: Double

    /// Unix timestamp (ms) when the job started processing (nil if still queued).
    let startedAtMs: Double?

    /// Unix timestamp (ms) when the job finished (completed, failed, or cancelled).
    let finishedAtMs: Double?

    /// Base64-encoded audio data. Only present when requested with `include_audio=true`
    /// and the job status is `.completed`.
    let audioBase64: String?

    /// Conformance to `Identifiable` using the server-generated job ID.
    var id: String { jobId }

    // MARK: - CodingKeys
    enum CodingKeys: String, CodingKey {
        case jobId = "job_id"
        case sessionId = "session_id"
        case itemId = "item_id"
        case chapterId = "chapter_id"
        case priority
        case status
        case provider
        case traceId = "trace_id"
        case durationMs = "duration_ms"
        case sampleRate = "sample_rate"
        case error
        case cacheHit = "cache_hit"
        case createdAtMs = "created_at_ms"
        case startedAtMs = "started_at_ms"
        case finishedAtMs = "finished_at_ms"
        case audioBase64 = "audio_base64"
    }
}
