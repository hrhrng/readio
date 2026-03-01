import Foundation
import SwiftUI

// MARK: - AppTheme
/// User's preferred color scheme for the app.
///
/// Maps to SwiftUI's `ColorScheme` for `.preferredColorScheme()` modifier.
/// The "system" option follows the device's current appearance setting.
enum AppTheme: String, CaseIterable, Sendable {
    case light
    case dark
    case system

    /// Converts to an optional `ColorScheme` for SwiftUI's `.preferredColorScheme()`.
    /// Returns nil for `.system` to inherit the device's current appearance.
    var colorScheme: ColorScheme? {
        switch self {
        case .light: return .light
        case .dark: return .dark
        case .system: return nil
        }
    }
}

// MARK: - AppSettingsKeys
/// Centralized storage keys for `@AppStorage`-backed user preferences.
///
/// All keys are namespaced with "readio." to avoid collisions with other
/// UserDefaults consumers. These keys are used consistently across the app
/// wherever `@AppStorage` is referenced.
///
/// **Sync with server:**
/// Some settings (like voice and speed) are also persisted on the server via
/// `PATCH /api/settings` so the web frontend stays in sync. The iOS app should
/// push local changes to the server and pull server settings on launch.
enum AppSettingsKeys {
    /// Base URL of the Readio API server.
    /// Default: "http://localhost:8000" for local development.
    /// Users configure this to point to their self-hosted instance.
    static let serverURL = "readio.serverURL"

    /// Color theme preference: "light", "dark", or "system".
    static let theme = "readio.theme"

    /// Accent color name used for buttons, links, and active states.
    /// Stored as a hex string (e.g., "#4F46E5") or a named color.
    static let accentColor = "readio.accentColor"

    /// Base font size for reader content in points.
    /// Typical range: 14–28. Default: 18.
    static let fontSize = "readio.fontSize"

    /// Default TTS voice ID used for new items that don't have a per-item override.
    /// Should match a `voice_id` from the `/api/tts/voices` response.
    static let defaultVoice = "readio.defaultVoice"

    /// Default TTS playback speed multiplier.
    /// 1.0 = normal speed. Typical range: 0.5–3.0.
    static let defaultSpeed = "readio.defaultSpeed"
}

// MARK: - AppSettings
/// Observable settings object that bridges `@AppStorage` values into a
/// single `@Observable` model for use throughout the SwiftUI view hierarchy.
///
/// This provides a centralized, reactive settings store that:
/// 1. Persists values to UserDefaults via `@AppStorage`
/// 2. Triggers SwiftUI view updates when any setting changes
/// 3. Provides type-safe access with sensible defaults
///
/// **Usage:**
/// ```swift
/// // In your App struct or root view:
/// @State private var settings = AppSettings()
///
/// // In any child view:
/// @Environment(AppSettings.self) var settings
/// Text("Font size: \(settings.fontSize)")
/// ```
@Observable
final class AppSettings: Sendable {
    // MARK: - Stored Properties
    // Each property reads from / writes to UserDefaults via the keys above.

    /// The base URL for all API requests. Must include the scheme and port
    /// (e.g., "http://192.168.1.100:8000"). No trailing slash.
    var serverURL: String {
        didSet { UserDefaults.standard.set(serverURL, forKey: AppSettingsKeys.serverURL) }
    }

    /// Current theme preference. Changing this immediately updates the app's appearance.
    var theme: AppTheme {
        didSet { UserDefaults.standard.set(theme.rawValue, forKey: AppSettingsKeys.theme) }
    }

    /// Accent color as a hex string or named color identifier.
    var accentColor: String {
        didSet { UserDefaults.standard.set(accentColor, forKey: AppSettingsKeys.accentColor) }
    }

    /// Reader font size in points. Affects all content text in the reader view.
    var fontSize: Double {
        didSet { UserDefaults.standard.set(fontSize, forKey: AppSettingsKeys.fontSize) }
    }

    /// Default TTS voice ID. Used when a library item doesn't have a per-item voice override.
    var defaultVoice: String {
        didSet { UserDefaults.standard.set(defaultVoice, forKey: AppSettingsKeys.defaultVoice) }
    }

    /// Default TTS playback speed. Used when a library item doesn't have a per-item speed override.
    var defaultSpeed: Double {
        didSet { UserDefaults.standard.set(defaultSpeed, forKey: AppSettingsKeys.defaultSpeed) }
    }

    // MARK: - Initialization
    /// Loads all settings from UserDefaults, falling back to sensible defaults.
    init() {
        let defaults = UserDefaults.standard
        self.serverURL = defaults.string(forKey: AppSettingsKeys.serverURL) ?? "http://localhost:8000"
        self.theme = AppTheme(rawValue: defaults.string(forKey: AppSettingsKeys.theme) ?? "system") ?? .system
        self.accentColor = defaults.string(forKey: AppSettingsKeys.accentColor) ?? "#4F46E5"
        self.fontSize = defaults.double(forKey: AppSettingsKeys.fontSize) != 0
            ? defaults.double(forKey: AppSettingsKeys.fontSize)
            : 18.0
        self.defaultVoice = defaults.string(forKey: AppSettingsKeys.defaultVoice) ?? ""
        self.defaultSpeed = defaults.double(forKey: AppSettingsKeys.defaultSpeed) != 0
            ? defaults.double(forKey: AppSettingsKeys.defaultSpeed)
            : 1.0
    }
}
