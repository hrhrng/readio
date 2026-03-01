import Foundation
import SwiftUI

// MARK: - PlayerViewModel
/// Thin wrapper around `TTSPlaybackEngine` that provides view-friendly state
/// and manages the engine lifecycle (creation, configuration, cleanup).
///
/// The `PlayerViewModel` sits between the player UI and the engine layer:
///
/// ```
/// PlayerView  ←→  PlayerViewModel  ←→  TTSPlaybackEngine
///                       ↕                      ↕
///                  TTSService              AVAudioPlayer
///                  LibraryService          AudioCache
///                  SettingsService         MPNowPlayingInfoCenter
/// ```
///
/// ## Responsibilities
///
/// 1. **Engine lifecycle:** Creates a new `TTSPlaybackEngine` when an item is
///    opened for playback, and tears it down when the user navigates away.
///    The engine is nullable because it's only created after the item's content
///    and sentences have been loaded by the `ReaderViewModel`.
///
/// 2. **Voice management:** Fetches available voices from the backend and tracks
///    the per-item voice override and the global default voice. When the user
///    changes the voice, updates both the server (persisted) and the engine (live).
///
/// 3. **Speed management:** Tracks the per-item speed override and the global
///    default (1.0). Speed changes are persisted to the server and applied to
///    the engine immediately (which clears its cache and re-synthesizes).
///
/// 4. **View-friendly state:** Surfaces engine state (isPlaying, currentSentenceIndex,
///    etc.) for binding in SwiftUI views. The engine uses @Observable so property
///    changes propagate automatically through the view hierarchy.
///
/// ## Why a separate ViewModel?
///
/// The engine is a pure playback orchestrator — it doesn't know about voice lists,
/// server-side persistence, or UI configuration. This ViewModel bridges those
/// concerns, keeping the engine focused and testable.
@MainActor
@Observable
final class PlayerViewModel {

    // MARK: - Published State

    /// The active TTS playback engine. `nil` when no item is loaded for playback.
    /// Views can observe engine properties (isPlaying, currentSentenceIndex, etc.)
    /// directly through this reference since the engine is also @Observable.
    var engine: TTSPlaybackEngine? = nil

    /// All available TTS voices fetched from the backend.
    /// Displayed in the voice picker UI grouped by language.
    var voices: [VoiceInfo] = []

    /// The server's configured default voice ID. Used when a specific item
    /// doesn't have a voice override set.
    var defaultVoiceId: String = ""

    /// The currently effective voice for the loaded item.
    /// Reads from the item's per-item override, falling back to `defaultVoiceId`.
    var currentVoice: String? = nil

    /// The currently effective playback speed for the loaded item.
    /// Reads from the item's per-item override, falling back to 1.0.
    var currentSpeed: Double = 1.0

    // MARK: - Dependencies

    /// Service for fetching TTS voices from the backend.
    private let ttsService = TTSService()

    /// Service for persisting voice and speed changes to the backend.
    private let libraryService = LibraryService()

    /// The library item ID the engine is currently configured for.
    /// Used to persist voice/speed changes to the correct item.
    private var currentItemId: String? = nil

    // MARK: - Public API

    /// Create and configure a new TTS playback engine for the given item.
    ///
    /// If an engine already exists for a different item, it is cleaned up first.
    /// The engine is created with the item's sentences, speed, and voice settings,
    /// ready for the user to tap "play".
    ///
    /// - Parameters:
    ///   - itemId: The library item ID being played.
    ///   - sentences: The extracted sentence array from the reader.
    ///   - speed: Per-item speed override, or `nil` to use the default (1.0).
    ///   - voice: Per-item voice override, or `nil` to use `defaultVoiceId`.
    ///   - title: Item title for Now Playing display.
    func setupEngine(
        itemId: String,
        sentences: [Sentence],
        speed: Double?,
        voice: String?,
        title: String
    ) {
        // Clean up the previous engine if switching items
        if currentItemId != itemId {
            engine?.cleanup()
        }

        currentItemId = itemId

        // Resolve effective speed and voice from per-item overrides or defaults
        let effectiveSpeed = speed ?? 1.0
        let effectiveVoice = voice ?? (defaultVoiceId.isEmpty ? nil : defaultVoiceId)

        currentSpeed = effectiveSpeed
        currentVoice = effectiveVoice

        // Create the new engine
        engine = TTSPlaybackEngine(
            itemId: itemId,
            sentences: sentences,
            speed: effectiveSpeed,
            voice: effectiveVoice,
            itemTitle: title
        )
    }

    /// Fetch the list of available TTS voices from the backend.
    ///
    /// Should be called once on app launch or when the player screen first appears.
    /// The voice list rarely changes, so it's safe to cache in memory for the
    /// session lifetime.
    func loadVoices() async {
        do {
            let response = try await ttsService.fetchVoices()
            voices = response.voices
            defaultVoiceId = response.defaultVoiceId

            // If no per-item voice is set, use the default
            if currentVoice == nil && !defaultVoiceId.isEmpty {
                currentVoice = defaultVoiceId
            }
        } catch {
            // Voice loading failure is non-fatal — the user can still play with
            // the server's default voice; they just can't see/change voice options.
            print("[PlayerViewModel] Failed to load voices: \(error)")
        }
    }

    /// Change the TTS voice for the current item.
    ///
    /// Updates three layers:
    /// 1. **Server:** PATCHes the item's voice override so it persists across sessions.
    /// 2. **Engine:** Updates the live engine, which clears its cache and re-synthesizes
    ///    the current sentence if playing.
    /// 3. **Local state:** Updates `currentVoice` for the UI.
    ///
    /// - Parameter voiceId: The new voice ID, or `nil` to reset to the server default.
    func setVoice(_ voiceId: String?) async {
        currentVoice = voiceId
        engine?.updateVoice(voiceId)

        // Persist to server (fire-and-forget — UI has already updated optimistically)
        if let itemId = currentItemId {
            try? await libraryService.updateVoice(id: itemId, voice: voiceId)
        }
    }

    /// Change the playback speed for the current item.
    ///
    /// Updates three layers:
    /// 1. **Server:** PATCHes the item's speed override so it persists across sessions.
    /// 2. **Engine:** Updates the live engine, which clears its cache and re-synthesizes
    ///    the current sentence if playing (TTS audio is speed-specific).
    /// 3. **Local state:** Updates `currentSpeed` for the UI.
    ///
    /// - Parameter speed: The new speed multiplier (e.g., 0.5, 1.0, 1.5, 2.0).
    func setSpeed(_ speed: Double) async {
        currentSpeed = speed
        engine?.updateSpeed(speed)

        // Persist to server (fire-and-forget — UI has already updated optimistically)
        if let itemId = currentItemId {
            try? await libraryService.updateSpeed(id: itemId, speed: speed)
        }
    }

    /// Tear down the current engine and release all resources.
    ///
    /// Called when the player screen is dismissed or the app is backgrounded
    /// with no active playback.
    func cleanup() {
        engine?.cleanup()
        engine = nil
        currentItemId = nil
    }
}
