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
            ContentView()
        }
        .onChange(of: scenePhase) { oldPhase, newPhase in
            handleScenePhaseChange(from: oldPhase, to: newPhase)
        }
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
            // App returned to foreground — could refresh data here if needed.
            break
        @unknown default:
            break
        }
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
