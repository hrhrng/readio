import Foundation

// MARK: - VoiceInfo
/// Metadata for a TTS voice available from the Apple SDK.
///
/// `PlayerViewModel` maps installed `AVSpeechSynthesisVoice` values into this
/// view-facing shape without exposing AVFoundation to SwiftUI.
///
/// **Fields:**
/// - `voice_id`: Stable system voice identifier used by `AVSpeechSynthesisVoice`
/// - `label`: Human-readable system voice name
/// - `language`: Base BCP 47 language used by the picker, such as "en" or "zh"
/// - `gender`: System-provided gender metadata when available
/// - `description`: Quality and personal-voice traits when relevant
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
