import SwiftUI

// MARK: - SpeedControlView
/// A compact speed control button that cycles through TTS playback speed presets.
///
/// **Layout:**
/// ```
/// ┌─────────┐
/// │  1.0x   │  ← Tap to cycle to next speed
/// └─────────┘
/// ```
///
/// **Speed Presets:**
/// The button cycles through these speeds in order on each tap:
/// ```
/// 0.75x → 1.0x → 1.25x → 1.5x → 2.0x → (wraps to 0.75x)
/// ```
///
/// **Interaction:**
/// - **Tap:** Advances to the next speed in the cycle.
/// - **Long press (context menu):** Shows all available speeds for direct selection,
///   useful when the user wants to jump to a specific speed without cycling.
///
/// **Design:**
/// - Displays the current speed as a compact label (e.g., "1.5x").
/// - Uses monospaced digits for stable layout as the number changes.
/// - Provides haptic feedback on speed change.
struct SpeedControlView: View {

    // MARK: - Properties

    /// The current playback speed (e.g., 1.0, 1.5, 2.0).
    let currentSpeed: Double

    /// Callback invoked when the user selects a new speed.
    let onSpeedChange: (Double) -> Void

    // MARK: - Constants

    /// The ordered list of speed presets to cycle through.
    /// Covers the common range from slow (0.75x) to fast (2.0x).
    private static let speedPresets: [Double] = [0.75, 1.0, 1.25, 1.5, 2.0]

    // MARK: - Body

    var body: some View {
        // MARK: Speed Button with Context Menu
        // Tap cycles to the next speed; long-press shows all options.
        Menu {
            // Context menu showing all speed options for direct selection.
            // Each option shows a checkmark if it's the current speed.
            ForEach(Self.speedPresets, id: \.self) { speed in
                Button {
                    selectSpeed(speed)
                } label: {
                    HStack {
                        Text(speedLabel(for: speed))
                        if isCurrentSpeed(speed) {
                            Image(systemName: "checkmark")
                        }
                    }
                }
            }
        } label: {
            // The visible button label showing the current speed.
            Text(speedLabel(for: currentSpeed))
                .font(.system(size: 14, weight: .semibold, design: .rounded))
                .monospacedDigit() // Prevents layout jumps when digits change
                .foregroundStyle(.primary)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(Color(.secondarySystemGroupedBackground))
                )
        } primaryAction: {
            // Primary tap action — cycle to the next speed preset.
            cycleToNextSpeed()
        }
        .frame(width: 52, height: 44)
    }

    // MARK: - Actions

    /// Cycles to the next speed preset in the ordered list.
    ///
    /// Finds the current speed's index in `speedPresets`, then advances by one.
    /// Wraps around to the beginning when the end is reached.
    /// Provides haptic feedback on change.
    private func cycleToNextSpeed() {
        let presets = Self.speedPresets
        // Find the index of the current speed (or closest match)
        let currentIndex = presets.firstIndex(where: { isCurrentSpeed($0) }) ?? 0
        let nextIndex = (currentIndex + 1) % presets.count
        selectSpeed(presets[nextIndex])
    }

    /// Selects a specific speed and triggers haptic feedback.
    private func selectSpeed(_ speed: Double) {
        let generator = UIImpactFeedbackGenerator(style: .light)
        generator.impactOccurred()
        onSpeedChange(speed)
    }

    // MARK: - Helpers

    /// Formats a speed value into a display label (e.g., "1.0x", "1.25x").
    ///
    /// - Speeds that are whole numbers or have one decimal: "1.0x", "1.5x"
    /// - Speeds with two decimal places: "1.25x", "0.75x"
    private func speedLabel(for speed: Double) -> String {
        if speed.truncatingRemainder(dividingBy: 1.0) == 0 {
            // Whole number (1.0, 2.0) — show one decimal place
            return String(format: "%.1fx", speed)
        } else if (speed * 10).truncatingRemainder(dividingBy: 1.0) == 0 {
            // One decimal place (0.5, 1.5) — show one decimal place
            return String(format: "%.1fx", speed)
        } else {
            // Two decimal places (0.75, 1.25) — show two decimal places
            return String(format: "%.2fx", speed)
        }
    }

    /// Checks if a given speed matches the current speed within floating-point tolerance.
    ///
    /// Uses a small epsilon comparison because floating-point arithmetic can produce
    /// tiny rounding errors (e.g., 0.75 might be stored as 0.7499999999...).
    private func isCurrentSpeed(_ speed: Double) -> Bool {
        abs(currentSpeed - speed) < 0.01
    }
}

// MARK: - Preview

#Preview("1.0x Speed") {
    HStack {
        SpeedControlView(
            currentSpeed: 1.0,
            onSpeedChange: { speed in print("Speed changed to \(speed)") }
        )
    }
    .padding()
}

#Preview("All Speeds") {
    VStack(spacing: 12) {
        ForEach([0.75, 1.0, 1.25, 1.5, 2.0], id: \.self) { speed in
            HStack {
                Text("Current: \(speed, specifier: "%.2f")")
                    .font(.caption)
                Spacer()
                SpeedControlView(
                    currentSpeed: speed,
                    onSpeedChange: { _ in }
                )
            }
        }
    }
    .padding()
}
