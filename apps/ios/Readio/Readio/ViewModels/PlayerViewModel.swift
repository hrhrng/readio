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
///              AppleNativeTTS             AVAudioPlayer
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
/// 2. **Voice management:** Discovers installed voices through the Apple SDK and tracks
///    the per-item voice override and the global default voice. When the user
///    changes the voice, updates both persisted item metadata and the engine (live).
///
/// 3. **Speed management:** Tracks the per-item speed override and the global
///    default (1.0). Speed changes are persisted and applied live by
///    `AVAudioPlayer` without invalidating canonical synthesized audio.
///
/// 4. **View-friendly state:** Surfaces engine state (isPlaying, currentSentenceIndex,
///    etc.) for binding in SwiftUI views. The engine uses @Observable so property
///    changes propagate automatically through the view hierarchy.
///
/// ## Why a separate ViewModel?
///
/// The engine is a pure playback orchestrator — it doesn't know about voice lists,
/// item-metadata persistence, or UI configuration. This ViewModel bridges those
/// concerns, keeping the engine focused and testable.
@MainActor
@Observable
final class PlayerViewModel {

    // MARK: - Published State

    /// The active TTS playback engine. `nil` when no item is loaded for playback.
    /// Views can observe engine properties (isPlaying, currentSentenceIndex, etc.)
    /// directly through this reference since the engine is also @Observable.
    var engine: TTSPlaybackEngine? = nil

    /// All installed system TTS voices exposed by the Apple SDK.
    /// Displayed in the voice picker UI grouped by language.
    var voices: [VoiceInfo] = []

    /// The system's preferred default voice ID. Used when a specific item
    /// doesn't have a voice override set.
    var defaultVoiceId: String = ""

    /// The currently effective voice for the loaded item.
    /// Reads from the item's per-item override, falling back to `defaultVoiceId`.
    var currentVoice: String? = nil

    /// The currently effective playback speed for the loaded item.
    /// Reads from the item's per-item override, falling back to 1.0.
    var currentSpeed: Double = 1.0

    /// Selected effect implementation. Native is the offline-first default;
    /// cloud is only constructed after the user explicitly persists that choice.
    private(set) var ttsMode: AppleTTSMode
    private(set) var readingMode: ReadioReadingMode
    private(set) var readingEffort: ReadioEffortLevel

    // MARK: - Dependencies

    /// One SDK provider instance owns voice discovery and render cancellation.
    private let ttsProvider: any ReadioAppleTTSProvider
    private let defaults: UserDefaults
    private var defaultPlaybackRate: Double
    private var defaultTextCharactersPerSecond: Double
    private var baseTextCharactersPerSecond: Double

    /// Service for persisting voice and speed changes to the backend.
    private let libraryService = LibraryService()

    /// The library item ID the engine is currently configured for.
    /// Used to persist voice/speed changes to the correct item.
    private var currentItemId: String? = nil

    init(
        ttsProvider: (any ReadioAppleTTSProvider)? = nil,
        ttsMode: AppleTTSMode = .native,
        defaults: UserDefaults = .standard
    ) {
        self.ttsProvider = ttsProvider ?? AppleNativeTTSProvider()
        self.ttsMode = ttsMode
        self.defaults = defaults
        let mode = ReadioReadingMode(
            rawValue: defaults.string(forKey: AppSettingsKeys.readingMode) ?? "manual"
        ) ?? .manual
        let effort = ReadioEffortLevel(
            rawValue: defaults.string(forKey: AppSettingsKeys.readingEffort) ?? "high"
        ) ?? .high
        let storedTextRate = defaults.double(
            forKey: AppSettingsKeys.readingBaseCharactersPerSecond
        )
        let preferences = ReadioReadingPreferences(
            mode: mode,
            baseTextCharactersPerSecond: storedTextRate > 0 ? storedTextRate : 46,
            effort: effort
        )
        let pace = try? ReadioSDK().resolveReadingPace(preferences)
        self.readingMode = pace?.mode ?? mode
        self.readingEffort = pace?.effort ?? effort
        self.defaultPlaybackRate = pace?.audioPlaybackRate ?? 1
        self.defaultTextCharactersPerSecond =
            pace?.textCharactersPerSecond ?? 46
        self.baseTextCharactersPerSecond = preferences.baseTextCharactersPerSecond
    }

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
        title: String,
        highlightDocument: ReadioHighlightDocument? = nil,
        onPersistProgress: @escaping @MainActor (Int) -> Void = { _ in }
    ) {
        // Clean up the previous engine if switching items
        if currentItemId != itemId {
            engine?.cleanup()
        }

        currentItemId = itemId

        // Resolve effective speed and voice from per-item overrides or defaults
        let effectiveSpeed = speed ?? defaultPlaybackRate
        let catalog = ttsProvider.voiceCatalog()
        let effectiveVoice: String?
        if ttsMode == .cloud {
            // Item voice overrides historically contain Apple voice identifiers.
            // Never leak those provider-specific IDs into a cloud synthesis job.
            let cloudVoice = catalog.defaultVoiceID
            effectiveVoice = cloudVoice.isEmpty ? nil : cloudVoice
        } else if let voice,
                  catalog.voices.contains(where: { $0.id == voice }) {
            effectiveVoice = voice
        } else {
            // The reverse transition matters too: an item may retain a cloud
            // voice ID after the device returns to its installed voice catalog.
            let nativeDefault = defaultVoiceId.isEmpty
                ? catalog.defaultVoiceID
                : defaultVoiceId
            effectiveVoice = nativeDefault.isEmpty ? nil : nativeDefault
        }

        currentSpeed = effectiveSpeed
        currentVoice = effectiveVoice
        let baseTextRate = defaultTextCharactersPerSecond
            / max(defaultPlaybackRate, 0.01)

        // Create the new engine
        engine = TTSPlaybackEngine(
            itemId: itemId,
            sentences: sentences,
            speed: effectiveSpeed,
            voice: effectiveVoice,
            itemTitle: title,
            highlightDocument: highlightDocument,
            ttsProvider: ttsProvider,
            readingMode: readingMode,
            textCharactersPerSecond: baseTextRate * effectiveSpeed,
            onPersistProgress: onPersistProgress
        )
    }

    /// Load the installed default, enhanced, premium, and authorized personal voices.
    ///
    /// Should be called once on app launch or when the player screen first appears.
    /// The voice list rarely changes, so it's safe to cache in memory for the
    /// session lifetime.
    func loadVoices() async {
        let catalog = ttsProvider.voiceCatalog()
        voices = catalog.voices.map { voice in
            let language = voice.language
                .split(separator: "-", maxSplits: 1)
                .first
                .map { String($0).lowercased() } ?? "other"
            let traits = [
                voice.quality == .default ? nil : voice.quality.rawValue.capitalized,
                voice.isPersonal ? "Personal Voice" : nil,
            ].compactMap { $0 }
            return VoiceInfo(
                voiceId: voice.id,
                label: voice.name,
                language: language,
                gender: voice.gender,
                description: traits.isEmpty ? nil : traits.joined(separator: " · ")
            )
        }
        defaultVoiceId = catalog.defaultVoiceID

        if currentVoice == nil && !defaultVoiceId.isEmpty {
            currentVoice = defaultVoiceId
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
    /// - Parameter voiceId: The new voice ID, or `nil` to use the catalog default.
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
    /// 2. **Engine:** Updates the live `AVAudioPlayer` rate without clearing the cache
    ///    or moving the current media cursor.
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

    /// Applies the shared Rust pace projection to an already-running engine and
    /// persists the peer reading mode locally for the next launch.
    func setReadingMode(_ mode: ReadioReadingMode) {
        defaults.set(mode.rawValue, forKey: AppSettingsKeys.readingMode)
        applyReadingPreferences(mode: mode, effort: readingEffort)
    }

    /// One six-level effort value drives both the text deadline and live audio
    /// rate through the Rust pace projection.
    func setReadingEffort(_ effort: ReadioEffortLevel) {
        defaults.set(effort.rawValue, forKey: AppSettingsKeys.readingEffort)
        applyReadingPreferences(mode: readingMode, effort: effort)
    }

    /// Re-reads preferences changed by another settings surface or scene.
    func refreshReadingPreferences() {
        let mode = ReadioReadingMode(
            rawValue: defaults.string(forKey: AppSettingsKeys.readingMode) ?? "manual"
        ) ?? .manual
        let effort = ReadioEffortLevel(
            rawValue: defaults.string(forKey: AppSettingsKeys.readingEffort) ?? "high"
        ) ?? .high
        let storedBaseRate = defaults.double(
            forKey: AppSettingsKeys.readingBaseCharactersPerSecond
        )
        if storedBaseRate > 0 {
            baseTextCharactersPerSecond = storedBaseRate
        }
        applyReadingPreferences(mode: mode, effort: effort)
    }

    /// Replaces the current chapter projection and installs its local cursor as
    /// one ordered navigation operation. Keeping the seek in this boundary
    /// prevents TOC fragments and cross-chapter search hits from briefly or
    /// permanently falling back to the chapter's first sentence.
    func updateSentences(_ sentences: [Sentence], startingAt sentenceIndex: Int) {
        engine?.updateSentences(sentences)
        engine?.seekToSentence(sentenceIndex)
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

    private func applyReadingPreferences(
        mode: ReadioReadingMode,
        effort: ReadioEffortLevel
    ) {
        guard let pace = try? ReadioSDK().resolveReadingPace(
            ReadioReadingPreferences(
                mode: mode,
                baseTextCharactersPerSecond: baseTextCharactersPerSecond,
                effort: effort
            )
        ) else { return }

        guard readingMode != pace.mode
                || readingEffort != pace.effort
                || defaultPlaybackRate != pace.audioPlaybackRate
                || defaultTextCharactersPerSecond != pace.textCharactersPerSecond else {
            return
        }

        readingMode = pace.mode
        readingEffort = pace.effort
        defaultPlaybackRate = pace.audioPlaybackRate
        defaultTextCharactersPerSecond = pace.textCharactersPerSecond
        currentSpeed = pace.audioPlaybackRate
        engine?.updateReadingPace(pace)
    }
}
