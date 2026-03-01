import Foundation

// MARK: - VoiceInfo
/// Metadata for a single TTS voice available from the backend.
///
/// Voices are fetched from the MiniMax API by the backend and enriched with
/// parsed language/gender metadata. The backend filters to English ("en") and
/// Chinese ("zh") voices only, sorted alphabetically within each language group.
///
/// **Fields:**
/// - `voice_id`: The unique identifier passed to TTS synthesis requests
///   (e.g., "English_Trustworthy_Man", "Chinese(Mandarin)_Gentle_Woman")
/// - `label`: Human-readable display name derived from `voice_name` or `voice_id`
/// - `language`: ISO-ish language code — "en", "zh", or "other"
/// - `gender`: Inferred from keywords in the voice_id — "male", "female", or nil
/// - `description`: Joined description strings from the MiniMax API, if available
struct VoiceInfo: Codable, Identifiable, Sendable {
    let voiceId: String
    let label: String
    let language: String
    let gender: String?
    let description: String?

    /// Conformance to `Identifiable` using the unique voice identifier.
    var id: String { voiceId }

    // MARK: - CodingKeys
    enum CodingKeys: String, CodingKey {
        case voiceId = "voice_id"
        case label
        case language
        case gender
        case description
    }
}

// MARK: - VoiceListResponse
/// Response from `GET /api/tts/voices` containing all available TTS voices
/// and the server's configured default voice.
///
/// The `default_voice_id` is determined by the backend's `MINIMAX_VOICE_ID`
/// environment variable, falling back to a built-in default. The iOS app
/// uses this to pre-select a voice when the user hasn't chosen one yet.
struct VoiceListResponse: Codable, Sendable {
    /// All available voices, filtered to English and Chinese, sorted alphabetically.
    let voices: [VoiceInfo]

    /// The voice_id that should be selected by default for new items.
    let defaultVoiceId: String

    // MARK: - CodingKeys
    enum CodingKeys: String, CodingKey {
        case voices
        case defaultVoiceId = "default_voice_id"
    }
}
