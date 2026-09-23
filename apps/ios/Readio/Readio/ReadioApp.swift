import SwiftUI
import AVFoundation

// MARK: - ReadioApp
/// The main entry point for the Readio iOS application.
///
/// **Responsibilities:**
/// - Configures `AVAudioSession` for background audio playback so TTS continues
///   when the app is backgrounded or the device is locked.
/// - Creates the root `ContentView` and injects shared environment objects.
/// - Monitors `scenePhase` transitions to trigger progress persistence when the
///   user leaves the app (moving to `.inactive` or `.background`).
///
/// **Audio Session Setup:**
/// The `.playback` category with `.duckOthers` policy allows Readio's TTS audio
/// to play alongside (but quieter than) other audio sources like navigation prompts.
/// If `.duckOthers` is not desired, switch to `.playback` with default options.
@main
struct ReadioApp: App {

    // MARK: - Environment

    /// Tracks the current scene lifecycle phase (active, inactive, background).
    /// Used to save reading progress automatically when the user leaves the app.
    @Environment(\.scenePhase) private var scenePhase

    // MARK: - Initialization

    init() {
        configureAudioSession()
    }

    // MARK: - Scene

    var body: some Scene {
        WindowGroup {
            ReadioThemedScene {
                ContentView()
            }
        }
        .commands {
            ReadioCommands()
        }
        .onChange(of: scenePhase) { oldPhase, newPhase in
            handleScenePhaseChange(from: oldPhase, to: newPhase)
        }

        #if os(macOS)
        Settings {
            ReadioThemedScene {
                NavigationStack {
                    SettingsView()
                }
            }
        }
        #endif
    }

    // MARK: - Private Helpers

    /// Configures the shared audio session for background TTS playback.
    ///
    /// This must be called early in the app lifecycle — before any `AVAudioPlayer`
    /// or `AVPlayer` instances are created — so the system knows our app needs
    /// background audio capabilities.
    ///
    /// **Category:** `.playback` — audio continues when the screen is locked or
    /// the app is in the background.
    ///
    /// **Mode:** `.spokenAudio` — optimized for spoken word content, which enables
    /// system-level behaviors like automatic ducking and "listen after speaking"
    /// routing for CarPlay / Bluetooth accessories.
    private func configureAudioSession() {
        #if os(iOS)
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(
                .playback,
                mode: .spokenAudio,
                options: [.duckOthers]
            )
            try session.setActive(true)
        } catch {
            // Audio session configuration failure is non-fatal — TTS playback
            // will still work in the foreground, just not in the background.
            print("[ReadioApp] Failed to configure audio session: \(error.localizedDescription)")
        }
        #endif
    }

    /// Responds to scene lifecycle transitions.
    ///
    /// When the app moves from `.active` to `.inactive` (e.g., user swipes home,
    /// Control Center appears), we post a notification that view models can observe
    /// to persist any unsaved reading progress. This avoids coupling the app delegate
    /// directly to specific view model instances.
    private func handleScenePhaseChange(from oldPhase: ScenePhase, to newPhase: ScenePhase) {
        switch newPhase {
        case .inactive:
            // App is about to lose focus — a good time to flush progress.
            NotificationCenter.default.post(name: .readioSaveProgress, object: nil)
        case .background:
            // App has been fully backgrounded. Audio continues if session is active.
            NotificationCenter.default.post(name: .readioSaveProgress, object: nil)
        case .active:
            Task {
                _ = try? await AppleCloudService.shared.syncIfConnected()
            }
        @unknown default:
            break
        }
    }
}

private struct ReadioThemedScene<Content: View>: View {
    @Environment(\.colorScheme) private var colorScheme
    @AppStorage(AppSettingsKeys.theme) private var storedTheme = "system"
    @AppStorage(AppSettingsKeys.accentColor) private var storedAccent = "clover"
    @AppStorage(AppleLocalization.preferenceKey) private var storedLanguage = "system"

    let content: Content

    init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    var body: some View {
        let palette = ReadioAppleTheme.palette(for: colorScheme)
        content
            .environment(\.readioTheme, palette)
            .tint(palette.accent)
            .foregroundStyle(palette.ink)
            .background(palette.canvas.ignoresSafeArea())
            .preferredColorScheme(ReadioAppleTheme.preferredColorScheme(storedTheme))
            .environment(\.locale, AppleLocalization.locale(preference: storedLanguage))
            .onAppear {
                storedAccent = ReadioAppleTheme.normalizedAccentID(storedAccent)
                storedLanguage = AppleLocalization.normalizedPreference(storedLanguage)
            }
    }
}

private struct ReadioCommands: Commands {
    @FocusedValue(\.readioAppRouter) private var router

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Button("Import File…") {
                router?.requestFileImport()
            }
            .disabled(router == nil)
            .keyboardShortcut("o", modifiers: .command)
        }

        CommandMenu("Navigate") {
            Button("Home") {
                router?.navigate(to: .home)
            }
            .keyboardShortcut("1", modifiers: .command)

            Button("Library") {
                router?.navigate(to: .library)
            }
            .keyboardShortcut("2", modifiers: .command)

            Button("Import") {
                router?.navigate(to: .importItem)
            }
            .keyboardShortcut("3", modifiers: .command)

            Button("Settings") {
                router?.navigate(to: .settings)
            }
            .keyboardShortcut("4", modifiers: .command)
        }
    }
}

private struct ReadioAppRouterFocusedValueKey: FocusedValueKey {
    typealias Value = AppleAppRouter
}

extension FocusedValues {
    var readioAppRouter: AppleAppRouter? {
        get { self[ReadioAppRouterFocusedValueKey.self] }
        set { self[ReadioAppRouterFocusedValueKey.self] = newValue }
    }
}

// MARK: - Notification Names
/// Custom notification names used for cross-component communication.
/// Preferred over direct coupling between the App struct and view models.
extension Notification.Name {
    /// Posted when the app is about to lose focus, signaling view models
    /// to persist any unsaved reading/playback progress to the server.
    static let readioSaveProgress = Notification.Name("readioSaveProgress")
}
