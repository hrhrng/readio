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

    @Environment(\.readioTheme) private var theme

    // MARK: - Properties

    /// Binding to the current accent color identifier string.
    @Binding var accentColor: String

    // MARK: - Body

    var body: some View {
        HStack(spacing: 14) {
            ZStack {
                RoundedRectangle(cornerRadius: 10)
                    .fill(theme.sidebar)
                    .frame(width: 52, height: 52)
                Circle()
                    .fill(theme.highlight)
                    .frame(width: 30, height: 30)
                Circle()
                    .fill(theme.accent)
                    .frame(width: 16, height: 16)
            }

            VStack(alignment: .leading, spacing: 3) {
                Text("Readio Clover")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(theme.ink)
                Text("Evergreen navigation · mint reading focus")
                    .font(.caption)
                    .foregroundStyle(theme.secondaryInk)
            }

            Spacer()
            Image(systemName: "checkmark.circle.fill")
                .font(.title3)
                .foregroundStyle(theme.accent)
        }
        .padding(12)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(theme.selected)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(theme.border, lineWidth: 1)
        )
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Readio Clover theme selected")
        .onAppear {
            accentColor = "clover"
        }
    }
}

// MARK: - Preview

#Preview("Readio Clover") {
    Form {
        AccentColorPicker(accentColor: .constant("clover"))
    }
}
