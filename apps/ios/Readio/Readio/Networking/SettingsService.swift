import Foundation

// MARK: - SettingsService
/// Service layer for reading and writing user settings on the Readio server.
///
/// The server stores settings as a flat key-value store (string → string).
/// This allows the web and iOS frontends to share user preferences like
/// accent color, font size, and default voice/speed.
///
/// **Settings flow:**
/// 1. On app launch, fetch remote settings via `fetchSettings()`
/// 2. Merge with local `@AppStorage` values (local takes precedence for
///    iOS-specific settings like serverURL and theme)
/// 3. When the user changes a synced setting, push it to the server via
///    `patchSettings()` so the web frontend stays in sync
///
/// **Key naming convention:**
/// Settings keys use snake_case to match the web frontend's convention
/// (e.g., "font_size", "accent_color", "default_voice", "default_speed").
/// The iOS app's `AppSettingsKeys` use a "readio." prefix for local-only
/// storage; synced settings use the bare key names.
///
/// **Usage example:**
/// ```swift
/// let settingsService = SettingsService()
///
/// // Fetch all settings from the server
/// let remote = try await settingsService.fetchSettings()
/// // remote == ["font_size": "18", "accent_color": "#4F46E5", ...]
///
/// // Update a specific setting
/// try await settingsService.patchSettings(data: ["default_speed": "1.5"])
/// ```
final class SettingsService: Sendable {
    /// The API client used for all HTTP requests.
    private let client: APIClient

    /// Creates a new settings service instance.
    /// - Parameter client: The API client to use. Defaults to the shared singleton.
    init(client: APIClient = .shared) {
        self.client = client
    }

    // MARK: - Fetch Settings

    /// Fetches all user settings from the server.
    ///
    /// Returns a dictionary where both keys and values are strings. The caller
    /// is responsible for parsing values into the appropriate types (e.g.,
    /// converting "18" to a Double for font size).
    ///
    /// The backend stores settings in a SQLite table and returns all rows
    /// as a single JSON object.
    ///
    /// - Returns: Dictionary of all stored settings keyed by setting name.
    /// - Throws: `APIError` on network or server failure.
    func fetchSettings() async throws -> [String: String] {
        return try await client.get(APIEndpoints.settings)
    }

    // MARK: - Patch Settings

    /// Updates one or more user settings on the server.
    ///
    /// This performs a merge/upsert: provided keys are created or updated,
    /// keys not included in the dictionary are left unchanged. To delete a
    /// setting, pass its key with a nil value (which serializes to JSON null).
    ///
    /// **Partial updates:** You can update a single setting without affecting
    /// others. For example, `patchSettings(data: ["default_speed": "2.0"])`
    /// only changes the speed, leaving all other settings untouched.
    ///
    /// - Parameter data: Dictionary of setting key-value pairs to upsert.
    ///   Keys and values are both strings (the backend stores everything as text).
    /// - Throws: `APIError` on network or server failure.
    func patchSettings(data: [String: String]) async throws {
        try await client.patch(APIEndpoints.settings, dictionary: data)
    }
}
