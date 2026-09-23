import SwiftUI
import UniformTypeIdentifiers

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

    @Environment(\.openURL) private var openURL
    @Environment(\.readioTheme) private var theme
    @Environment(\.locale) private var locale

    // MARK: - State

    /// The view model that handles server settings, theme persistence, etc.
    @State private var viewModel = SettingsViewModel()

    /// The server URL, persisted locally via UserDefaults.
    /// This is the primary source of truth for the API base URL.
    @AppStorage("serverURL") private var serverURL = "http://localhost:8000"

    @AppStorage("readio.cloudURL") private var cloudURL = "https://cloud.read-io.com"

    @State private var cloudSession: AppleCloudSession? = nil

    @State private var deviceAuthorization: AppleDeviceAuthorization? = nil

    @State private var isCloudWorking = false

    @State private var cloudMessage: String? = nil

    /// Whether a connection test is currently in progress.
    @State private var isTestingConnection = false

    /// The result of the last connection test (nil = not tested yet).
    @State private var connectionTestResult: ConnectionTestResult? = nil

    /// Font size for the reader, persisted across launches.
    @AppStorage("readerFontSize") private var readerFontSize: Double = 16.0

    @AppStorage(AppleLocalization.preferenceKey) private var languagePreference = "system"

    @State private var supportBundleDocument: ReadioSupportBundleDocument? = nil

    @State private var isBuildingSupportBundle = false

    @State private var isExportingSupportBundle = false

    @State private var supportBundleMessage: String? = nil

    // MARK: - Body

    var body: some View {
        Form {
            cloudSection

            ttsSection

            readingSection

            // MARK: Server Section
            serverSection

            // MARK: Appearance Section
            appearanceSection

            diagnosticsSection

            // MARK: About Section
            aboutSection
        }
        .scrollContentBackground(.hidden)
        .background(theme.canvas.ignoresSafeArea())
        .tint(theme.accent)
        .navigationTitle("Settings")
        .task {
            await viewModel.loadSettings()
            cloudSession = try? await AppleCloudService.shared.session()
        }
        .onReceive(NotificationCenter.default.publisher(for: .readioReplicaDidSync)) { _ in
            Task { await viewModel.loadSettings() }
        }
        .fileExporter(
            isPresented: $isExportingSupportBundle,
            document: supportBundleDocument,
            contentType: .json,
            defaultFilename: supportBundleDocument?.file.filename ?? "readio-support.json"
        ) { result in
            switch result {
            case .success:
                supportBundleMessage = "Support bundle saved."
            case .failure(let error):
                supportBundleMessage = error.localizedDescription
            }
        }
    }

    private var ttsSection: some View {
        Section {
            Picker("Speech Engine", selection: $viewModel.ttsMode) {
                Label("Native", systemImage: "iphone.radiowaves.left.and.right")
                    .tag(AppleTTSMode.native)
                Label("Cloud", systemImage: "cloud")
                    .tag(AppleTTSMode.cloud)
            }
            .pickerStyle(.segmented)

            if viewModel.ttsMode == .cloud {
                TextField("Cloud voice ID", text: $viewModel.cloudVoiceID)
                    .readioSettingsUncapitalizedInput()
                    .autocorrectionDisabled()

                if cloudSession == nil {
                    Label(
                        "Sign in above before using Cloud TTS.",
                        systemImage: "person.crop.circle.badge.exclamationmark"
                    )
                    .font(.caption)
                    .foregroundStyle(.orange)
                }
            }
        } header: {
            Label("Text to Speech", systemImage: "waveform")
        } footer: {
            if viewModel.ttsMode == .native {
                Text("Native voices stay on this device and work offline without an account.")
            } else {
                Text("Cloud TTS sends sentence text to your authenticated cloud provider. Reopen the reader after changing engines.")
            }
        }
    }

    private var readingSection: some View {
        Section {
            Picker("Mode", selection: $viewModel.readingMode) {
                ForEach(ReadioReadingMode.allCases, id: \.self) { mode in
                    Text(readingModeLabel(mode)).tag(mode)
                }
            }
            .pickerStyle(.segmented)

            Picker("Effort", selection: $viewModel.readingEffort) {
                ForEach(ReadioEffortLevel.allCases, id: \.self) { effort in
                    Text(effortLabel(effort)).tag(effort)
                }
            }

            Stepper(
                value: $viewModel.baseTextCharactersPerSecond,
                in: 4...4_000,
                step: 1
            ) {
                LabeledContent(
                    "Base Text Pace",
                    value: "\(formatPace(viewModel.baseTextCharactersPerSecond)) \(paceUnit)"
                )
            }

            if let pace = try? viewModel.readingPace() {
                LabeledContent(
                    "Effective Pace",
                    value: "\(formatPace(pace.textCharactersPerSecond)) \(paceUnit) · \(formatPace(pace.audioPlaybackRate))× \(audioLabel)"
                )
                .monospacedDigit()
            }
        } header: {
            Label("Reading", systemImage: "book.pages")
        } footer: {
            Text("Manual waits for you, Auto follows text pace, and Read Aloud follows the real audio clock. One Rust effort level drives both text and audio.")
        }
    }

    // MARK: - Server Section

    private var cloudSection: some View {
        Section {
            if let cloudSession {
                LabeledContent("Account", value: "Connected")
                LabeledContent("Cloud", value: cloudSession.serverURL.host ?? cloudSession.serverURL.absoluteString)

                Button {
                    Task { await syncCloud() }
                } label: {
                    Label(isCloudWorking ? "Syncing…" : "Sync Now", systemImage: "arrow.triangle.2.circlepath")
                }
                .disabled(isCloudWorking)

                Button("Sign Out", role: .destructive) {
                    Task { await signOutCloud() }
                }
                .disabled(isCloudWorking)
            } else if let authorization = deviceAuthorization {
                LabeledContent("Code", value: authorization.userCode)
                    .textSelection(.enabled)

                Button("Open Approval Page") {
                    openURL(authorization.verificationURLComplete ?? authorization.verificationURL)
                }

                Button {
                    Task { await checkCloudApproval(authorization) }
                } label: {
                    Label("Check Approval", systemImage: "checkmark.shield")
                }
                .disabled(isCloudWorking)

                Button("Cancel", role: .cancel) {
                    deviceAuthorization = nil
                    cloudMessage = nil
                }
            } else {
                TextField("https://cloud.read-io.com", text: $cloudURL)
                    .readioSettingsURLInput()
                    .autocorrectionDisabled()

                Button {
                    Task { await beginCloudLogin() }
                } label: {
                    Label("Connect Cloud", systemImage: "icloud")
                }
                .disabled(isCloudWorking || cloudURL.isEmpty)
            }

            if let cloudMessage {
                Text(cloudMessage)
                    .font(.caption)
                    .foregroundStyle(cloudSession == nil ? Color.secondary : Color.green)
            }
        } header: {
            Label("Cloud (Optional)", systemImage: "icloud")
        } footer: {
            Text("Sign in only for cross-device sync and cloud TTS. Your local library and native voices work without an account.")
        }
    }

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
                    .readioSettingsURLInput()
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
            Text("Optional URL acquisition/provider endpoint. Local files, reading, bookmarks, settings, and native TTS remain offline-first.")
                .font(.caption)
        }
    }

    // MARK: - Appearance Section

    /// Visual customization: theme, accent color, and reader font size.
    private var appearanceSection: some View {
        Section {
            Picker("Language", selection: $languagePreference) {
                Text("Follow System").tag("system")
                Text("English").tag("en")
                Text("简体中文").tag("zh-Hans")
            }

            // MARK: Theme Picker
            ThemePicker(theme: $viewModel.theme)

            // MARK: Accent Color Picker
            VStack(alignment: .leading, spacing: 8) {
                Text("Brand Theme")
                    .font(.subheadline)
                    .foregroundStyle(theme.secondaryInk)

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

    private var diagnosticsSection: some View {
        Section {
            Button {
                Task { await prepareSupportBundle() }
            } label: {
                Label(
                    isBuildingSupportBundle ? "Preparing Support Bundle…" : "Export Support Bundle",
                    systemImage: "square.and.arrow.up"
                )
            }
            .disabled(isBuildingSupportBundle)

            if let supportBundleMessage {
                Text(supportBundleMessage)
                    .font(.caption)
                    .foregroundStyle(theme.secondaryInk)
            }
        } header: {
            Label("Diagnostics", systemImage: "stethoscope")
        } footer: {
            Text("Exports recent operation IDs, provider, sync, and playback timing. It never includes book text, URLs, credentials, or API keys.")
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

    private func prepareSupportBundle() async {
        isBuildingSupportBundle = true
        supportBundleMessage = nil
        defer { isBuildingSupportBundle = false }

        do {
            let file = try await AppleDiagnosticLog.shared.buildSupportBundleFile(
                appVersion: appVersion
            )
            supportBundleDocument = ReadioSupportBundleDocument(file: file)
            isExportingSupportBundle = true
        } catch {
            supportBundleMessage = error.localizedDescription
        }
    }

    private func readingModeLabel(_ mode: ReadioReadingMode) -> String {
        switch mode {
        case .manual:
            AppleLocalization.string("Manual", defaultValue: "Manual", locale: locale)
        case .auto:
            AppleLocalization.string("Auto", defaultValue: "Auto", locale: locale)
        case .readAloud:
            AppleLocalization.string("Read Aloud", defaultValue: "Read Aloud", locale: locale)
        }
    }

    private func effortLabel(_ effort: ReadioEffortLevel) -> String {
        switch effort {
        case .minimal:
            AppleLocalization.string("Minimal", defaultValue: "Minimal", locale: locale)
        case .low:
            AppleLocalization.string("Low", defaultValue: "Low", locale: locale)
        case .medium:
            AppleLocalization.string("Medium", defaultValue: "Medium", locale: locale)
        case .high:
            AppleLocalization.string("High", defaultValue: "High", locale: locale)
        case .xhigh:
            AppleLocalization.string("Xhigh", defaultValue: "Xhigh", locale: locale)
        case .max:
            AppleLocalization.string("Max", defaultValue: "Max", locale: locale)
        }
    }

    private func formatPace(_ value: Double) -> String {
        value.formatted(.number.precision(.fractionLength(0...2)))
    }

    private var paceUnit: String {
        AppleLocalization.string(
            "settings.charactersPerSecond",
            defaultValue: "chars/s",
            locale: locale
        )
    }

    private var audioLabel: String {
        AppleLocalization.string(
            "settings.audio",
            defaultValue: "audio",
            locale: locale
        )
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

    private func beginCloudLogin() async {
        guard let url = URL(string: cloudURL) else {
            cloudMessage = "Enter a valid cloud URL."
            return
        }
        isCloudWorking = true
        defer { isCloudWorking = false }
        do {
            let authorization = try await AppleCloudService.shared.beginLogin(serverURL: url)
            deviceAuthorization = authorization
            cloudMessage = "Approve this device in your browser, then check approval."
            openURL(authorization.verificationURLComplete ?? authorization.verificationURL)
        } catch {
            cloudMessage = error.localizedDescription
        }
    }

    private func checkCloudApproval(_ authorization: AppleDeviceAuthorization) async {
        guard let url = URL(string: cloudURL) else { return }
        isCloudWorking = true
        defer { isCloudWorking = false }
        do {
            switch try await AppleCloudService.shared.pollLogin(
                serverURL: url,
                deviceCode: authorization.deviceCode
            ) {
            case .pending:
                cloudMessage = "Approval is still pending."
            case .connected(let session):
                cloudSession = session
                deviceAuthorization = nil
                cloudMessage = "Connected. Local data is unchanged."
            case .denied:
                deviceAuthorization = nil
                cloudMessage = "Device authorization was denied."
            case .expired:
                deviceAuthorization = nil
                cloudMessage = "The device code expired. Start again."
            }
        } catch {
            cloudMessage = error.localizedDescription
        }
    }

    private func syncCloud() async {
        isCloudWorking = true
        defer { isCloudWorking = false }
        do {
            _ = try await AppleCloudService.shared.sync()
            cloudMessage = "Sync complete."
        } catch {
            cloudMessage = error.localizedDescription
        }
    }

    private func signOutCloud() async {
        isCloudWorking = true
        defer { isCloudWorking = false }
        do {
            try await AppleCloudService.shared.logout()
            cloudSession = nil
            deviceAuthorization = nil
            cloudMessage = "Signed out. Local data was kept."
        } catch {
            cloudMessage = error.localizedDescription
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

private extension View {
    @ViewBuilder
    func readioSettingsUncapitalizedInput() -> some View {
        #if os(iOS)
        textInputAutocapitalization(.never)
        #else
        self
        #endif
    }

    @ViewBuilder
    func readioSettingsURLInput() -> some View {
        #if os(iOS)
        keyboardType(.URL)
            .textInputAutocapitalization(.never)
        #else
        self
        #endif
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
