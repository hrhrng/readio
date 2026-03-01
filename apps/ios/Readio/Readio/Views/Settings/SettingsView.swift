import SwiftUI

// MARK: - SettingsView
/// The app settings screen organized into logical sections.
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │  Settings                (nav title) │
/// ├──────────────────────────────────────┤
/// │  Server                              │
/// │  ┌────────────────────────────────┐  │
/// │  │ URL  [ http://localhost:8000 ] │  │
/// │  │ [ Test Connection ]           │  │
/// │  └────────────────────────────────┘  │
/// │                                      │
/// │  Appearance                          │
/// │  ┌────────────────────────────────┐  │
/// │  │ Theme  [System|Light|Dark]    │  │
/// │  │ Accent ● ● ● ● ● ●          │  │
/// │  │ Font Size  ──●────────  16pt  │  │
/// │  └────────────────────────────────┘  │
/// │                                      │
/// │  About                               │
/// │  ┌────────────────────────────────┐  │
/// │  │ Readio v1.0.0                 │  │
/// │  │ Open Source on GitHub         │  │
/// │  └────────────────────────────────┘  │
/// └──────────────────────────────────────┘
/// ```
///
/// **Persistence:**
/// - Server URL stored in `@AppStorage` for cross-launch persistence.
/// - Theme and accent color managed by `SettingsViewModel` and synced to
///   the server for cross-device consistency.
/// - Font size stored locally and applied to reader views.
struct SettingsView: View {

    // MARK: - State

    /// The view model that handles server settings, theme persistence, etc.
    @State private var viewModel = SettingsViewModel()

    /// The server URL, persisted locally via UserDefaults.
    /// This is the primary source of truth for the API base URL.
    @AppStorage("serverURL") private var serverURL = "http://localhost:8000"

    /// Whether a connection test is currently in progress.
    @State private var isTestingConnection = false

    /// The result of the last connection test (nil = not tested yet).
    @State private var connectionTestResult: ConnectionTestResult? = nil

    /// Font size for the reader, persisted across launches.
    @AppStorage("readerFontSize") private var readerFontSize: Double = 16.0

    // MARK: - Body

    var body: some View {
        Form {
            // MARK: Server Section
            serverSection

            // MARK: Appearance Section
            appearanceSection

            // MARK: About Section
            aboutSection
        }
        .navigationTitle("Settings")
        .task {
            await viewModel.loadSettings()
        }
    }

    // MARK: - Server Section

    /// Server configuration: URL input and connection test button.
    ///
    /// The URL is validated on change and the connection test sends a health
    /// check request to the server's `/api/health` endpoint.
    private var serverSection: some View {
        Section {
            // MARK: Server URL Field
            VStack(alignment: .leading, spacing: 6) {
                Text("Server URL")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)

                TextField("http://localhost:8000", text: $serverURL)
                    .textFieldStyle(.roundedBorder)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .onChange(of: serverURL) { _, newValue in
                        // Reset test result when URL changes
                        connectionTestResult = nil
                        viewModel.updateServerURL(newValue)
                    }
            }

            // MARK: Connection Test Button
            HStack {
                Button {
                    Task { await testConnection() }
                } label: {
                    HStack(spacing: 8) {
                        if isTestingConnection {
                            ProgressView()
                                .controlSize(.small)
                        }
                        Text("Test Connection")
                    }
                }
                .disabled(serverURL.isEmpty || isTestingConnection)

                Spacer()

                // Connection test result indicator
                if let result = connectionTestResult {
                    connectionResultBadge(result)
                }
            }
        } header: {
            Label("Server", systemImage: "server.rack")
        } footer: {
            Text("The URL of your Readio backend server. Required for all features.")
                .font(.caption)
        }
    }

    // MARK: - Appearance Section

    /// Visual customization: theme, accent color, and reader font size.
    private var appearanceSection: some View {
        Section {
            // MARK: Theme Picker
            ThemePicker(theme: $viewModel.theme)

            // MARK: Accent Color Picker
            VStack(alignment: .leading, spacing: 8) {
                Text("Accent Color")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)

                AccentColorPicker(accentColor: $viewModel.accentColor)
            }
            .padding(.vertical, 4)

            // MARK: Font Size Slider
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("Reader Font Size")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)

                    Spacer()

                    Text("\(Int(readerFontSize))pt")
                        .font(.subheadline)
                        .fontWeight(.medium)
                        .foregroundStyle(.primary)
                        .monospacedDigit()
                }

                // Font size slider with min/max labels
                HStack(spacing: 8) {
                    // Small "A" icon representing minimum size
                    Text("A")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)

                    Slider(
                        value: $readerFontSize,
                        in: 12...28,
                        step: 1
                    )

                    // Large "A" icon representing maximum size
                    Text("A")
                        .font(.system(size: 20))
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.vertical, 4)
        } header: {
            Label("Appearance", systemImage: "paintbrush")
        }
    }

    // MARK: - About Section

    /// App information: name, version, and open-source link.
    private var aboutSection: some View {
        Section {
            // App name and version
            HStack {
                Text("Version")
                    .foregroundStyle(.primary)
                Spacer()
                Text(appVersion)
                    .foregroundStyle(.secondary)
            }

            // Open source link
            Link(destination: URL(string: "https://github.com/anthropics/readio")!) {
                HStack {
                    Label("Source Code", systemImage: "chevron.left.forwardslash.chevron.right")
                        .foregroundStyle(.primary)
                    Spacer()
                    Image(systemName: "arrow.up.right.square")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        } header: {
            Label("About", systemImage: "info.circle")
        } footer: {
            VStack(spacing: 4) {
                Text("Readio")
                    .fontWeight(.medium)
                Text("Open Source Audiobook & TTS Reading Platform")
            }
            .font(.caption)
            .foregroundStyle(.tertiary)
            .frame(maxWidth: .infinity, alignment: .center)
            .padding(.top, 16)
        }
    }

    // MARK: - Helpers

    /// The app version string from the main bundle (e.g., "1.0.0 (1)").
    private var appVersion: String {
        let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0"
        let build = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "1"
        return "\(version) (\(build))"
    }

    /// Tests the connection to the Readio backend server.
    ///
    /// Sends a GET request to `/api/health` and checks for a 200 response.
    /// Updates `connectionTestResult` with the outcome.
    private func testConnection() async {
        isTestingConnection = true
        connectionTestResult = nil

        defer { isTestingConnection = false }

        guard let url = URL(string: "\(serverURL)/api/health") else {
            connectionTestResult = .failure("Invalid URL")
            return
        }

        do {
            let (_, response) = try await URLSession.shared.data(from: url)
            if let httpResponse = response as? HTTPURLResponse, httpResponse.statusCode == 200 {
                connectionTestResult = .success
            } else {
                connectionTestResult = .failure("Server returned an error")
            }
        } catch {
            connectionTestResult = .failure(error.localizedDescription)
        }
    }

    /// Renders a small badge showing the connection test result.
    @ViewBuilder
    private func connectionResultBadge(_ result: ConnectionTestResult) -> some View {
        switch result {
        case .success:
            Label("Connected", systemImage: "checkmark.circle.fill")
                .font(.caption)
                .foregroundStyle(.green)

        case .failure(let message):
            Label(message, systemImage: "xmark.circle.fill")
                .font(.caption)
                .foregroundStyle(.red)
                .lineLimit(1)
        }
    }
}

// MARK: - ConnectionTestResult
/// The outcome of a server connection test.
private enum ConnectionTestResult {
    case success
    case failure(String)
}

// MARK: - Preview

#Preview {
    NavigationStack {
        SettingsView()
    }
}
