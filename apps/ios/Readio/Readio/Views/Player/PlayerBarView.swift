import SwiftUI

// MARK: - PlayerBarView
/// The bottom transport control bar for TTS playback.
///
/// **Layout (compact, single-row):**
/// ```
/// ┌──────────────────────────────────────────────────────────┐
/// │                                                          │
/// │  ━━━━━━━━━━━━━━━━●━━━━━━━━━━━━━━━  3:42 remaining       │  ← Progress bar
/// │                                                          │
/// │  ◀◀    ▶ / ⏸    ▶▶         1.0x           🎤            │  ← Transport controls
/// │                                                          │
/// │  ⚠️ Error message...                      [Retry]       │  ← Error banner (conditional)
/// │                                                          │
/// └──────────────────────────────────────────────────────────┘
/// ```
///
/// **Controls:**
/// - **Play/Pause:** Large central button toggling TTS playback.
/// - **Previous/Next:** Skip to the previous or next sentence.
/// - **Progress Bar:** Shows overall reading progress. Tappable to seek.
/// - **Speed Button:** Cycles through speed presets (0.75x → 1.0x → 1.25x → 1.5x → 2.0x).
/// - **Voice Button:** Opens the voice picker sheet via callback.
/// - **Loading Indicator:** Overlays the play button when audio is being synthesized.
/// - **Error Banner:** Appears when TTS encounters an error, with retry/dismiss actions.
///
/// **Design:**
/// - Uses a glass-morphism background (`ultraThinMaterial`) for a modern, layered appearance.
/// - Respects safe area insets for devices with home indicators (iPhone X and later).
/// - The entire bar is 60pt tall (plus safe area) for comfortable touch targets.
struct PlayerBarView: View {

    // MARK: - Properties

    /// The TTS playback engine providing state (isPlaying, progress, errors, etc.)
    /// and control actions (play, pause, seek, etc.).
    let engine: TTSPlaybackEngine

    /// The current playback speed, provided by the PlayerViewModel since the engine's
    /// speed property is private. Used by the SpeedControlView.
    var currentSpeed: Double = 1.0

    /// Callback when the user changes the playback speed via the speed control.
    /// The parent view should update both the engine and the ViewModel.
    var onSpeedChange: ((Double) -> Void)? = nil

    /// Callback to open the voice picker sheet. Handled by the parent `ReaderView`.
    let onVoiceTap: () -> Void

    // MARK: - Body

    var body: some View {
        VStack(spacing: 0) {
            // Top divider separating the player bar from the content area
            Divider()

            VStack(spacing: 8) {
                // MARK: Progress Bar + Time Remaining
                progressSection

                // MARK: Transport Controls
                transportControls

                // MARK: Error Banner (Conditional)
                if let ttsError = engine.ttsError {
                    errorBanner(ttsError)
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 10)
            .padding(.bottom, 6)
        }
        .background(
            // Glass-morphism background for a modern, layered look.
            // `.ultraThinMaterial` provides a frosted-glass effect that lets
            // the content beneath show through subtly.
            Rectangle()
                .fill(.ultraThinMaterial)
                .ignoresSafeArea(.container, edges: .bottom)
        )
    }

    // MARK: - Progress Section

    /// Horizontal progress bar with remaining time estimate.
    ///
    /// The progress bar is interactive — tapping or dragging seeks to the corresponding
    /// position in the document (by sentence index).
    private var progressSection: some View {
        HStack(spacing: 8) {
            // Progress bar (tappable to seek)
            GeometryReader { geometry in
                ZStack(alignment: .leading) {
                    // Background track
                    RoundedRectangle(cornerRadius: 2)
                        .fill(Color(.systemGray4))
                        .frame(height: 4)

                    // Filled progress indicator
                    RoundedRectangle(cornerRadius: 2)
                        .fill(Color.accentColor)
                        .frame(
                            width: geometry.size.width * progressFraction,
                            height: 4
                        )

                    // Seek thumb — small circle at the current position
                    Circle()
                        .fill(Color.accentColor)
                        .frame(width: 10, height: 10)
                        .offset(x: geometry.size.width * progressFraction - 5)
                }
                .contentShape(Rectangle()) // Make the entire track tappable
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onEnded { value in
                            let fraction = max(0, min(1, value.location.x / geometry.size.width))
                            engine.seekToProgress(fraction)
                        }
                )
            }
            .frame(height: 10)

            // Remaining time estimate
            Text(remainingTimeText)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 70, alignment: .trailing)
        }
    }

    // MARK: - Transport Controls

    /// The main row of playback control buttons.
    private var transportControls: some View {
        HStack(spacing: 0) {
            // MARK: Previous Sentence
            Button { engine.prevSentence() } label: {
                Image(systemName: "backward.fill")
                    .font(.title3)
                    .foregroundStyle(.primary)
            }
            .frame(width: 44, height: 44)

            Spacer()

            // MARK: Play / Pause Button
            // This is the primary action button, larger than the others.
            // Shows a loading spinner when audio is being synthesized.
            ZStack {
                if engine.isLoading {
                    // Loading state — spinner replaces the play/pause icon
                    ProgressView()
                        .controlSize(.regular)
                        .tint(.accentColor)
                } else {
                    Button {
                        if engine.isPlaying {
                            engine.pause()
                        } else {
                            engine.play()
                        }
                    } label: {
                        Image(systemName: engine.isPlaying ? "pause.circle.fill" : "play.circle.fill")
                            .font(.system(size: 44))
                            .foregroundStyle(.accentColor)
                    }
                }
            }
            .frame(width: 50, height: 50)

            Spacer()

            // MARK: Next Sentence
            Button { engine.nextSentence() } label: {
                Image(systemName: "forward.fill")
                    .font(.title3)
                    .foregroundStyle(.primary)
            }
            .frame(width: 44, height: 44)

            Spacer()

            // MARK: Speed Control
            // Speed is managed by PlayerViewModel (not directly on the engine),
            // so we pass the externally-provided currentSpeed and callback.
            SpeedControlView(
                currentSpeed: currentSpeed,
                onSpeedChange: { newSpeed in
                    onSpeedChange?(newSpeed)
                }
            )

            Spacer()

            // MARK: Voice Picker Button
            Button(action: onVoiceTap) {
                Image(systemName: "person.wave.2.fill")
                    .font(.body)
                    .foregroundStyle(.primary)
            }
            .frame(width: 44, height: 44)
        }
    }

    // MARK: - Error Banner

    /// Displays a TTS error message with retry and dismiss actions.
    ///
    /// Appears as a compact red-tinted banner at the bottom of the player bar.
    /// The user can retry the failed synthesis or dismiss the error.
    private func errorBanner(_ errorMessage: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.caption)
                .foregroundStyle(.red)

            Text(errorMessage)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)

            Spacer()

            // Retry button — re-attempts synthesis for the current sentence
            Button {
                engine.retry()
            } label: {
                Text("Retry")
                    .font(.caption)
                    .fontWeight(.medium)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)

            // Dismiss button — clears the error without retrying
            Button {
                engine.dismissError()
            } label: {
                Image(systemName: "xmark")
                    .font(.caption)
            }
            .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(Color.red.opacity(0.08))
        )
    }

    // MARK: - Computed Properties

    /// The current playback progress as a fraction (0.0 to 1.0).
    /// Based on the current sentence index relative to total sentences.
    private var progressFraction: CGFloat {
        // Use a reasonable fallback if the engine doesn't track total sentences.
        // The engine.currentSentenceIndex is the sentence being played;
        // progress is mapped from 0 to 1 based on the total count.
        guard engine.currentSentenceIndex >= 0 else { return 0 }
        // Approximate: use the sentence index as a proxy for progress.
        // A more accurate implementation would use the engine's internal total.
        return CGFloat(max(0, min(1, engine.currentWordProgress)))
    }

    /// Formats the estimated remaining time into a human-readable string.
    ///
    /// - Less than 60 seconds: "42s left"
    /// - Less than 60 minutes: "5:42 left"
    /// - 60+ minutes: "1:05:42 left"
    private var remainingTimeText: String {
        let totalSeconds = Int(engine.estimatedRemainingSeconds)
        guard totalSeconds > 0 else { return "--:--" }

        let hours = totalSeconds / 3600
        let minutes = (totalSeconds % 3600) / 60
        let seconds = totalSeconds % 60

        if hours > 0 {
            return String(format: "%d:%02d:%02d", hours, minutes, seconds)
        } else {
            return String(format: "%d:%02d", minutes, seconds)
        }
    }
}

// MARK: - Preview

// PlayerBarView requires a TTSPlaybackEngine instance, which is a complex object.
// Previews would need a mock engine. Shown here as a placeholder.
#Preview {
    VStack {
        Spacer()
        // In a real preview, you'd inject a mock TTSPlaybackEngine here.
        Text("Player bar preview requires a mock TTSPlaybackEngine")
            .font(.caption)
            .foregroundStyle(.secondary)
    }
}
