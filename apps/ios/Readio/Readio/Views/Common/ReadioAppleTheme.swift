import SwiftUI

/// Stable, inspectable color values shared by every Apple surface. Hex lives in
/// one place so the SwiftUI projection cannot drift into a collection of system
/// defaults that happens to look different on each OS.
struct ReadioAppleThemeSpec: Equatable, Sendable {
    let accentHex: String
    let accentStrongHex: String
    let onAccentHex: String
    let highlightHex: String
    let canvasHex: String
    let sidebarHex: String
    let sidebarInkHex: String
    let sidebarMutedHex: String
    let sidebarSelectedHex: String
    let raisedHex: String
    let inkHex: String
    let secondaryInkHex: String
    let hoverHex: String
    let selectedHex: String
    let borderHex: String
    let dangerHex: String
    let warningHex: String
    let infoHex: String
}

struct ReadioApplePalette {
    let spec: ReadioAppleThemeSpec

    var accent: Color { Color(readioHex: spec.accentHex) }
    /// Links and quiet shelf actions must not fall back to the host system's
    /// blue accent when the command-line macOS bundle has no asset catalog.
    var shelfAction: Color { accent }
    var accentStrong: Color { Color(readioHex: spec.accentStrongHex) }
    var onAccent: Color { Color(readioHex: spec.onAccentHex) }
    var highlight: Color { Color(readioHex: spec.highlightHex) }
    var canvas: Color { Color(readioHex: spec.canvasHex) }
    var sidebar: Color { Color(readioHex: spec.sidebarHex) }
    var sidebarInk: Color { Color(readioHex: spec.sidebarInkHex) }
    var sidebarMuted: Color { Color(readioHex: spec.sidebarMutedHex) }
    var sidebarSelected: Color { Color(readioHex: spec.sidebarSelectedHex) }
    var raised: Color { Color(readioHex: spec.raisedHex) }
    var ink: Color { Color(readioHex: spec.inkHex) }
    var secondaryInk: Color { Color(readioHex: spec.secondaryInkHex) }
    var hover: Color { Color(readioHex: spec.hoverHex) }
    var selected: Color { Color(readioHex: spec.selectedHex) }
    var border: Color { Color(readioHex: spec.borderHex) }
    var danger: Color { Color(readioHex: spec.dangerHex) }
    var warning: Color { Color(readioHex: spec.warningHex) }
    var info: Color { Color(readioHex: spec.infoHex) }
}

enum ReadioAppleTheme {
    static func spec(for colorScheme: ColorScheme) -> ReadioAppleThemeSpec {
        switch colorScheme {
        case .dark:
            ReadioAppleThemeSpec(
                accentHex: "#8AD6A9",
                accentStrongHex: "#B0E9C4",
                onAccentHex: "#102416",
                highlightHex: "#315941",
                canvasHex: "#181C19",
                sidebarHex: "#0F1912",
                sidebarInkHex: "#F4F8F4",
                sidebarMutedHex: "#A9BBAE",
                sidebarSelectedHex: "#244D34",
                raisedHex: "#252925",
                inkHex: "#F4F7F4",
                secondaryInkHex: "#C0C8C1",
                hoverHex: "#303630",
                selectedHex: "#294232",
                borderHex: "#374039",
                dangerHex: "#FF9B8F",
                warningHex: "#F4C76B",
                infoHex: "#82B7FF"
            )
        default:
            ReadioAppleThemeSpec(
                accentHex: "#397548",
                accentStrongHex: "#2A5C36",
                onAccentHex: "#FFFFFF",
                highlightHex: "#BFD9C4",
                canvasHex: "#FCFCF8",
                sidebarHex: "#F7F7F2",
                sidebarInkHex: "#292929",
                sidebarMutedHex: "#727772",
                sidebarSelectedHex: "#E0EBE1",
                raisedHex: "#FFFFFF",
                inkHex: "#292929",
                secondaryInkHex: "#5F655F",
                hoverHex: "#E9EDE8",
                selectedHex: "#E0EBE1",
                borderHex: "#DDE3DD",
                dangerHex: "#B42318",
                warningHex: "#8A5A00",
                infoHex: "#245EA8"
            )
        }
    }

    static func palette(for colorScheme: ColorScheme) -> ReadioApplePalette {
        ReadioApplePalette(spec: spec(for: colorScheme))
    }

    /// Older builds stored arbitrary system color names or the original indigo
    /// hex. Preserve the preference key while converging every install on the
    /// committed Readio clover identity.
    static func normalizedAccentID(_ value: String?) -> String {
        "clover"
    }

    static func preferredColorScheme(_ rawValue: String) -> ColorScheme? {
        switch rawValue {
        case "light": .light
        case "dark": .dark
        default: nil
        }
    }
}

/// Shared desktop geometry keeps the native Apple surface on the same visual
/// rails as the committed reference instead of inheriting arbitrary defaults.
enum ReadioAppleLayout {
    static let sidebarIdealWidth: CGFloat = 256
    static let libraryContentMaxWidth: CGFloat = 1180
    static let bookCoverWidth: CGFloat = 200
    static let bookCoverHeight: CGFloat = 300
}

private struct ReadioApplePaletteKey: EnvironmentKey {
    static let defaultValue = ReadioAppleTheme.palette(for: .light)
}

extension EnvironmentValues {
    var readioTheme: ReadioApplePalette {
        get { self[ReadioApplePaletteKey.self] }
        set { self[ReadioApplePaletteKey.self] = newValue }
    }
}

private extension Color {
    init(readioHex hex: String) {
        let value = hex.trimmingCharacters(in: CharacterSet(charactersIn: "#"))
        var integer: UInt64 = 0
        Scanner(string: value).scanHexInt64(&integer)
        let red = Double((integer >> 16) & 0xFF) / 255
        let green = Double((integer >> 8) & 0xFF) / 255
        let blue = Double(integer & 0xFF) / 255
        self.init(red: red, green: green, blue: blue)
    }
}
