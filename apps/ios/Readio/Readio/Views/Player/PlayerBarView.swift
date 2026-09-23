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
    @Environment(\.locale) private var locale

    @Environment(\.readioTheme) private var theme

    // MARK: - Properties

    /// The TTS playback engine providing state (isPlaying, progress, errors, etc.)
    /// and control actions (play, pause, seek, etc.).
    let engine: TTSPlaybackEngine

    /// The current playback speed, provided by the PlayerViewModel since the engine's
    /// speed property is private. Used by the SpeedControlView.
    var currentSpeed: Double = 1.0

    var readingMode: ReadioReadingMode = .readAloud
    var readingEffort: ReadioEffortLevel = .high
    var onReadingModeChange: ((ReadioReadingMode) -> Void)? = nil
    var onReadingEffortChange: ((ReadioEffortLevel) -> Void)? = nil

    /// Callback to open the voice picker sheet. Handled by the parent `ReaderView`.
    let onVoiceTap: () -> Void

    // MARK: - Body

    var body: some View {
        #if os(macOS)
        playerContent
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .frame(maxWidth: 760)
            .background {
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .fill(theme.raised)
            }
            .shadow(color: .black.opacity(0.14), radius: 8, x: 0, y: 4)
        #else
        VStack(spacing: 0) {
            // Top divider separating the player bar from the content area
            Rectangle()
                .fill(theme.border)
                .frame(height: 1)

            playerContent
            .padding(.horizontal, 16)
            .padding(.top, 10)
            .padding(.bottom, 6)
        }
        .background(
            Rectangle()
                .fill(theme.raised)
                .ignoresSafeArea(.container, edges: .bottom)
        )
        #endif
    }

    private var playerContent: some View {
        VStack(spacing: 8) {
            progressSection
            transportControls

            if let ttsError = engine.ttsError {
                errorBanner(ttsError)
            }
        }
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
                        .fill(theme.selected)
                        .frame(height: 4)

                    // Filled progress indicator
                    RoundedRectangle(cornerRadius: 2)
                        .fill(theme.accent)
                        .frame(
                            width: geometry.size.width * progressFraction,
                            height: 4
                        )

                    // Seek thumb — small circle at the current position
                    Circle()
                        .fill(theme.accent)
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
                .foregroundStyle(theme.secondaryInk)
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
                    .foregroundStyle(theme.ink)
            }
            .frame(width: 44, height: 44)
            .buttonStyle(.plain)

            Spacer()

            // MARK: Play / Pause Button
            // This is the primary action button, larger than the others.
            // Shows a loading spinner when audio is being synthesized.
            ZStack {
                if engine.isLoading {
                    // Loading state — spinner replaces the play/pause icon
                    ProgressView()
                        .controlSize(.regular)
                        .tint(theme.accent)
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
                            .foregroundStyle(theme.accent)
                    }
                    .disabled(readingMode == .manual)
                    .buttonStyle(.plain)
                }
            }
            .frame(width: 50, height: 50)

            Spacer()

            // MARK: Next Sentence
            Button { engine.nextSentence() } label: {
                Image(systemName: "forward.fill")
                    .font(.title3)
                    .foregroundStyle(theme.ink)
            }
            .frame(width: 44, height: 44)
            .buttonStyle(.plain)

            Spacer()

            // One compact control selects the peer reading mode and the shared
            // six-level effort without adding another transport row on iPhone.
            readingPreferencesMenu

            Spacer()

            if readingMode == .readAloud {
                Menu {
                    Button("Off") { engine.clearSleepDeadline() }
                    Divider()
                    ForEach([15, 30, 60], id: \.self) { minutes in
                        Button("\(minutes) minutes") {
                            let deadline = Int64(
                                Date().timeIntervalSince1970 * 1_000
                            ) + Int64(minutes * 60_000)
                            engine.setSleepDeadline(unixMilliseconds: deadline)
                        }
                    }
                } label: {
                    TimelineView(.periodic(from: .now, by: 30)) { context in
                        Label(
                            sleepTimerLabel(at: context.date),
                            systemImage: "moon.zzz"
                        )
                        .font(.caption)
                    }
                }
                .frame(minWidth: 44, minHeight: 44)
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)

                Spacer()
            }

            // MARK: Voice Picker Button
            Button(action: onVoiceTap) {
                Image(systemName: "person.wave.2.fill")
                    .font(.body)
                    .foregroundStyle(.primary)
            }
            .frame(width: 44, height: 44)
            .buttonStyle(.plain)
            .disabled(readingMode != .readAloud)
            .opacity(readingMode == .readAloud ? 1 : 0)
        }
    }

    private var readingPreferencesMenu: some View {
        Menu {
            Section("Reading Mode") {
                ForEach(ReadioReadingMode.allCases, id: \.self) { mode in
                    Button {
                        onReadingModeChange?(mode)
                    } label: {
                        if mode == readingMode {
                            Label(readingModeLabel(mode), systemImage: "checkmark")
                        } else {
                            Text(readingModeLabel(mode))
                        }
                    }
                }
            }
            Section("Effort") {
                ForEach(ReadioEffortLevel.allCases, id: \.self) { effort in
                    Button {
                        onReadingEffortChange?(effort)
                    } label: {
                        if effort == readingEffort {
                            Label(readingEffortLabel(effort), systemImage: "checkmark")
                        } else {
                            Text(readingEffortLabel(effort))
                        }
                    }
                }
            }
        } label: {
            VStack(spacing: 1) {
                Image(systemName: readingModeSymbol)
                    .font(.body)
                Text("\(readingModeLabel(readingMode)) · \(currentSpeed.formatted(.number.precision(.fractionLength(1))))×")
                    .font(.system(size: 9, design: .rounded))
                    .lineLimit(1)
            }
            .frame(minWidth: 56, minHeight: 44)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .accessibilityLabel("Reading mode and effort")
        .accessibilityValue(
            "\(readingModeLabel(readingMode)), \(readingEffortLabel(readingEffort)) effort"
        )
    }

    private var readingModeSymbol: String {
        switch readingMode {
        case .manual: "hand.tap"
        case .auto: "text.line.first.and.arrowtriangle.forward"
        case .readAloud: "waveform"
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

    private func readingEffortLabel(_ effort: ReadioEffortLevel) -> String {
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
            AppleLocalization.string("Xhigh", defaultValue: "Extra High", locale: locale)
        case .max:
            AppleLocalization.string("Max", defaultValue: "Maximum", locale: locale)
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

            Button("Change Voice", action: onVoiceTap)
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

    private func sleepTimerLabel(at date: Date) -> String {
        guard let deadline = engine.sleepDeadlineUnixMilliseconds else {
            return "Sleep"
        }
        let remaining = max(
            0,
            Double(deadline) / 1_000 - date.timeIntervalSince1970
        )
        return "\(Int(ceil(remaining / 60)))m"
    }

    // MARK: - Computed Properties

    /// The current playback progress as a fraction (0.0 to 1.0).
    /// Based on the current sentence index relative to total sentences.
    private var progressFraction: CGFloat {
        CGFloat(engine.progressBasisPoints) / 10_000
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
