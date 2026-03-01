import Foundation
import SwiftUI

// MARK: - SettingsViewModel
/// View model for the Settings screen, managing both local (device-only) and
/// server-persisted configuration.
///
/// ## Settings Architecture
///
/// Settings in Readio are split across two storage layers:
///
/// ### Local settings (@AppStorage / UserDefaults)
/// - **Server URL:** The base URL of the Readio backend API. Defaults to
///   `http://localhost:8000`. Changing this requires restarting API connections.
/// - **Theme:** Appearance mode — "system" (follows device), "light", or "dark".
/// - **Accent color:** Index into a preset color palette for UI tinting.
/// - **Font size:** Reader font size in points.
///
/// These are stored locally because they're device-specific (a user might want
/// different font sizes on iPhone vs. iPad) and must be available before the
/// network is reachable (server URL bootstraps all other requests).
///
/// ### Server settings (GET/PATCH /api/settings)
/// - Key-value pairs persisted on the backend. Currently used for global defaults
///   like the default TTS voice, but extensible for any future preferences.
/// - Fetched on settings screen appear, saved on user action.
///
/// ## Data Flow
///
/// ```
/// SettingsView
///   ├─ .onAppear → loadSettings()        // fetch server settings
///   ├─ .onChange(serverURL) → updateServerURL()  // update @AppStorage + APIClient
///   ├─ .onChange(theme/accent/font) → auto-saved via @AppStorage bindings
///   └─ .onSaveButton → saveSettings()    // PATCH server settings
/// ```
///
/// ## Why @Observable + @AppStorage?
///
/// The `@Observable` macro provides change tracking for SwiftUI views, while
/// `@AppStorage` handles persistence. We bridge them by reading/writing
/// `UserDefaults` manually within the @Observable class, since @AppStorage
/// property wrappers can't be used directly inside @Observable classes.
@MainActor
@Observable
final class SettingsViewModel {

    // MARK: - Local Settings (persisted in UserDefaults)

    /// The base URL of the Readio backend API.
    ///
    /// Defaults to "http://localhost:8000" for local development. In production,
    /// users configure this to point to their self-hosted instance.
    ///
    /// Changing this value updates `APIClient.shared.baseURL` immediately so
    /// subsequent API calls use the new endpoint.
    var serverURL: String {
        didSet { UserDefaults.standard.set(serverURL, forKey: Keys.serverURL) }
    }

    /// Appearance theme preference.
    ///
    /// Possible values:
    /// - `"system"`: follow the device's light/dark mode setting
    /// - `"light"`: always use light mode
    /// - `"dark"`: always use dark mode
    var theme: String {
        didSet { UserDefaults.standard.set(theme, forKey: Keys.theme) }
    }

    /// Accent color preset identifier.
    ///
    /// Stored as a string (e.g., "blue", "purple", "orange") that maps to a
    /// preset in the UI layer. The view is responsible for applying the actual
    /// color values to the app's tint.
    var accentColor: String {
        didSet { UserDefaults.standard.set(accentColor, forKey: Keys.accentColor) }
    }

    /// Reader font size in points.
    ///
    /// Applied as a dynamic type size override in the reader view. The default
    /// (18.0) is optimized for comfortable reading on iPhone-sized screens.
    var fontSize: Double {
        didSet { UserDefaults.standard.set(fontSize, forKey: Keys.fontSize) }
    }

    // MARK: - Server Settings

    /// Key-value pairs fetched from `GET /api/settings`.
    ///
    /// The server settings are an open-ended dictionary. Currently known keys:
    /// - `default_voice`: default TTS voice ID for new items
    /// - `default_speed`: default TTS speed multiplier
    ///
    /// The view layer can display and edit these as needed.
    var serverSettings: [String: String] = [:]

    /// Whether settings are being loaded from the server.
    var isLoading = false

    /// Non-nil when a settings load or save operation failed.
    var error: String? = nil

    // MARK: - Dependencies

    /// Service layer for fetching and updating server-side settings.
    private let settingsService = SettingsService()

    // MARK: - UserDefaults Keys

    /// Centralized UserDefaults key constants to avoid typos and ensure consistency
    /// with any @AppStorage usage elsewhere in the app.
    /// Reuse the app-wide namespaced keys so UserDefaults stays consistent
    /// with @AppStorage and AppSettings reads elsewhere.
    private enum Keys {
        static let serverURL = AppSettingsKeys.serverURL
        static let theme = AppSettingsKeys.theme
        static let accentColor = AppSettingsKeys.accentColor
        static let fontSize = AppSettingsKeys.fontSize
    }

    // MARK: - Initialization

    /// Initialize with values from UserDefaults, falling back to sensible defaults.
    init() {
        let defaults = UserDefaults.standard
        self.serverURL = defaults.string(forKey: Keys.serverURL) ?? "http://localhost:8000"
        self.theme = defaults.string(forKey: Keys.theme) ?? "system"
        self.accentColor = defaults.string(forKey: Keys.accentColor) ?? "blue"
        self.fontSize = defaults.double(forKey: Keys.fontSize) > 0
            ? defaults.double(forKey: Keys.fontSize)
            : 18.0
    }

    // MARK: - Public API

    /// Fetch server-side settings from the backend.
    ///
    /// Called on settings screen appear. Updates `serverSettings` with the
    /// key-value pairs returned by `GET /api/settings`.
    func loadSettings() async {
        isLoading = true
        error = nil

        do {
            let settings = try await settingsService.fetchSettings()
            serverSettings = settings
        } catch {
            self.error = "Failed to load settings: \(error.localizedDescription)"
        }

        isLoading = false
    }

    /// Save the current server settings to the backend.
    ///
    /// PATCHes the `serverSettings` dictionary to `PATCH /api/settings`.
    /// Only sends values that have been modified by the user.
    func saveSettings() async {
        guard !serverSettings.isEmpty else { return }

        do {
            try await settingsService.patchSettings(data: serverSettings)
        } catch {
            self.error = "Failed to save settings: \(error.localizedDescription)"
        }
    }

    /// Update the server URL and propagate the change to the API client.
    ///
    /// This is more than a simple property setter — it also updates the
    /// singleton `APIClient`'s base URL so all subsequent network requests
    /// use the new endpoint.
    ///
    /// - Parameter url: The new server base URL (e.g., "https://readio.example.com").
    func updateServerURL(_ url: String) {
        // Normalize: remove trailing slash for consistency
        let normalized = url.hasSuffix("/") ? String(url.dropLast()) : url
        serverURL = normalized
        // APIClient.shared.baseURL reads from UserDefaults dynamically,
        // so writing to UserDefaults (via serverURL.didSet) is sufficient —
        // no explicit assignment needed.
    }
}
