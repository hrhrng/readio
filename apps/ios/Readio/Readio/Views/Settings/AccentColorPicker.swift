import SwiftUI

// MARK: - AccentColorPicker
/// A horizontal row of color circles for selecting the app's accent color.
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │  (●) (●) (●) (●) (●) (●)           │
/// │  blue purp orng grn  red  pink      │
/// │                                      │
/// │  Selected one has ✓ checkmark        │
/// └──────────────────────────────────────┘
/// ```
///
/// **Binding:**
/// The `accentColor` binding holds a string identifier (e.g., "blue", "purple")
/// that maps to a SwiftUI `Color`. This string is synced to the backend settings
/// for cross-device consistency.
///
/// **Design:**
/// - Each circle is 36pt diameter with a slight shadow for depth.
/// - The selected circle shows a white checkmark overlay.
/// - All circles have a subtle border to remain visible on both light/dark backgrounds.
/// - Tapping a circle provides haptic feedback via `UIImpactFeedbackGenerator`.
struct AccentColorPicker: View {

    // MARK: - Properties

    /// Binding to the current accent color identifier string.
    @Binding var accentColor: String

    /// The available accent color options.
    /// Each entry maps a string identifier to a SwiftUI Color.
    private let colorOptions: [(id: String, color: Color, label: String)] = [
        ("blue", .blue, "Blue"),
        ("purple", .purple, "Purple"),
        ("orange", .orange, "Orange"),
        ("green", .green, "Green"),
        ("red", .red, "Red"),
        ("pink", .pink, "Pink"),
        ("indigo", .indigo, "Indigo"),
        ("teal", .teal, "Teal"),
    ]

    /// Size of each color circle.
    private let circleSize: CGFloat = 36

    // MARK: - Body

    var body: some View {
        HStack(spacing: 12) {
            ForEach(colorOptions, id: \.id) { option in
                colorCircle(for: option)
            }

            Spacer()
        }
    }

    // MARK: - Color Circle

    /// Renders a single color circle with optional checkmark overlay.
    ///
    /// **States:**
    /// - **Selected:** Shows a white checkmark and a slightly larger scale.
    /// - **Unselected:** Plain circle with a subtle border.
    ///
    /// Tapping triggers a light haptic feedback and updates the binding.
    private func colorCircle(for option: (id: String, color: Color, label: String)) -> some View {
        let isSelected = accentColor == option.id

        return Button {
            // Provide haptic feedback on selection
            let generator = UIImpactFeedbackGenerator(style: .light)
            generator.impactOccurred()

            withAnimation(.easeInOut(duration: 0.15)) {
                accentColor = option.id
            }
        } label: {
            ZStack {
                // Color fill circle
                Circle()
                    .fill(option.color)
                    .frame(width: circleSize, height: circleSize)
                    .shadow(color: option.color.opacity(0.3), radius: isSelected ? 4 : 2, y: 1)

                // Subtle border for visibility on matching backgrounds
                Circle()
                    .strokeBorder(Color.primary.opacity(0.1), lineWidth: 1)
                    .frame(width: circleSize, height: circleSize)

                // Checkmark overlay for the selected color
                if isSelected {
                    Image(systemName: "checkmark")
                        .font(.system(size: 14, weight: .bold))
                        .foregroundStyle(.white)
                        .shadow(color: .black.opacity(0.3), radius: 1, y: 1)
                        .transition(.scale.combined(with: .opacity))
                }
            }
            // Scale up slightly when selected for extra emphasis
            .scaleEffect(isSelected ? 1.1 : 1.0)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(option.label)
        .accessibilityAddTraits(isSelected ? [.isSelected] : [])
    }
}

// MARK: - Preview

#Preview("Blue Selected") {
    Form {
        AccentColorPicker(accentColor: .constant("blue"))
    }
}

#Preview("Purple Selected") {
    Form {
        AccentColorPicker(accentColor: .constant("purple"))
    }
}
