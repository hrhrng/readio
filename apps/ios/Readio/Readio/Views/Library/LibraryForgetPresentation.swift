import Foundation

/// User-facing consequence of forgetting an item. The copy follows the Rust
/// ownership record instead of treating a linked source as Readio-owned data.
struct LibraryForgetPresentation: Equatable, Sendable {
    let title: String
    let message: String
    let actionLabel: String

    static func resolve(
        itemTitle: String,
        ownership: ReadioImportOwnership?,
        locale: Locale = Locale(identifier: "en")
    ) -> LibraryForgetPresentation {
        switch ownership {
        case .link:
            return LibraryForgetPresentation(
                title: AppleLocalization.string(
                    "library.removeLinked.title",
                    defaultValue: "Remove from Library?",
                    locale: locale
                ),
                message: AppleLocalization.formatted(
                    "library.removeLinked.message",
                    defaultValue: "“%1$@” will leave your Readio library. The linked source file stays on your device.",
                    locale: locale,
                    arguments: [itemTitle]
                ),
                actionLabel: AppleLocalization.string(
                    "Remove",
                    defaultValue: "Remove",
                    locale: locale
                )
            )
        case .copy, .move:
            return LibraryForgetPresentation(
                title: AppleLocalization.string(
                    "library.deleteCopy.title",
                    defaultValue: "Delete Readio Copy?",
                    locale: locale
                ),
                message: AppleLocalization.formatted(
                    "library.deleteCopy.message",
                    defaultValue: "“%1$@” and its Readio-managed copy will be deleted. Your original source is unaffected.",
                    locale: locale,
                    arguments: [itemTitle]
                ),
                actionLabel: AppleLocalization.string(
                    "Delete",
                    defaultValue: "Delete",
                    locale: locale
                )
            )
        case nil:
            return LibraryForgetPresentation(
                title: AppleLocalization.string(
                    "library.removeLinked.title",
                    defaultValue: "Remove from Library?",
                    locale: locale
                ),
                message: AppleLocalization.formatted(
                    "library.removeLegacy.message",
                    defaultValue: "“%1$@” will leave your Readio library. Its stored Readio copy, if present, will also be deleted.",
                    locale: locale,
                    arguments: [itemTitle]
                ),
                actionLabel: AppleLocalization.string(
                    "Remove",
                    defaultValue: "Remove",
                    locale: locale
                )
            )
        }
    }
}
