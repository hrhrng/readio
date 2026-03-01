import SwiftUI

// MARK: - ThemePicker
/// A segmented control for selecting the app's color scheme (System / Light / Dark).
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │  Theme                               │
/// │  ┌──────────┬──────────┬──────────┐ │
/// │  │ ☀️ System │ ☀️ Light │ 🌙 Dark  │ │
/// │  └──────────┴──────────┴──────────┘ │
/// └──────────────────────────────────────┘
/// ```
///
/// **Binding:**
/// The `theme` binding accepts string values: "system", "light", "dark".
/// These match the backend's settings schema so the preference can be
/// synced across devices.
///
/// **Behavior:**
/// - "system" follows the device's appearance setting (Settings > Display & Brightness).
/// - "light" forces light mode regardless of system setting.
/// - "dark" forces dark mode regardless of system setting.
///
/// The actual `.preferredColorScheme` modifier should be applied higher up in the
/// view hierarchy (typically in `ContentView` or the `App` struct) based on this value.
struct ThemePicker: View {

    // MARK: - Properties

    /// Binding to the current theme string. Valid values: "system", "light", "dark".
    @Binding var theme: String

    /// The available theme options with their display metadata.
    private let options: [(id: String, label: String, icon: String)] = [
        ("system", "System", "circle.lefthalf.filled"),
        ("light", "Light", "sun.max.fill"),
        ("dark", "Dark", "moon.fill"),
    ]

    // MARK: - Body

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Theme")
                .font(.subheadline)
                .foregroundStyle(.secondary)

            // MARK: Segmented Picker
            // Uses a `Picker` with `.segmented` style for the native iOS segmented
            // control appearance. Each segment shows an icon and label.
            Picker("Theme", selection: $theme) {
                ForEach(options, id: \.id) { option in
                    Label(option.label, systemImage: option.icon)
                        .tag(option.id)
                }
            }
            .pickerStyle(.segmented)
        }
    }
}

// MARK: - Preview

#Preview {
    Form {
        ThemePicker(theme: .constant("system"))
    }
}

#Preview("Dark Selected") {
    Form {
        ThemePicker(theme: .constant("dark"))
    }
}
