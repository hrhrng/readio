import Foundation

/// Keeps language selection deterministic across SwiftUI, Xcode builds, and the
/// standalone macOS packager. Unsupported system languages deliberately fall
/// back to the English development catalog.
enum AppleLocalization {
    static let preferenceKey = "readio.language"

    static func normalizedPreference(_ value: String) -> String {
        switch value {
        case "en", "zh-Hans": value
        default: "system"
        }
    }

    static func resolvedLocaleIdentifier(
        preference: String,
        systemLocaleIdentifier: String = Locale.autoupdatingCurrent.identifier
    ) -> String {
        let normalized = normalizedPreference(preference)
        guard normalized == "system" else { return normalized }

        let system = systemLocaleIdentifier
            .replacingOccurrences(of: "_", with: "-")
            .lowercased()
        if system == "zh"
            || system.hasPrefix("zh-cn")
            || system.hasPrefix("zh-sg")
            || system.hasPrefix("zh-hans") {
            return "zh-Hans"
        }
        return "en"
    }

    static func locale(preference: String) -> Locale {
        Locale(identifier: resolvedLocaleIdentifier(preference: preference))
    }

    static func string(
        _ key: String,
        defaultValue: String,
        locale: Locale,
        bundle: Bundle = .main
    ) -> String {
        let identifier = resolvedLocaleIdentifier(
            preference: locale.identifier,
            systemLocaleIdentifier: locale.identifier
        )
        guard let localizationPath = bundle.path(
            forResource: identifier,
            ofType: "lproj"
        ), let localizedBundle = Bundle(path: localizationPath) else {
            return defaultValue
        }
        return localizedBundle.localizedString(
            forKey: key,
            value: defaultValue,
            table: nil
        )
    }

    static func formatted(
        _ key: String,
        defaultValue: String,
        locale: Locale,
        arguments: [CVarArg],
        bundle: Bundle = .main
    ) -> String {
        String(
            format: string(
                key,
                defaultValue: defaultValue,
                locale: locale,
                bundle: bundle
            ),
            locale: locale,
            arguments: arguments
        )
    }
}
