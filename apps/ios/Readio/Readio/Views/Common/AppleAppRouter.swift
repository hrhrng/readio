import Foundation
import Observation

enum AppleAppDestination: String, CaseIterable, Identifiable, Hashable, Sendable {
    case home
    case library
    case importItem
    case settings

    var id: String { rawValue }

    var title: String {
        switch self {
        case .home: "Home"
        case .library: "Library"
        case .importItem: "Import"
        case .settings: "Settings"
        }
    }

    func localizedTitle(
        locale: Locale,
        bundle: Bundle = .main
    ) -> String {
        AppleLocalization.string(
            title,
            defaultValue: title,
            locale: locale,
            bundle: bundle
        )
    }

    var icon: String {
        switch self {
        case .home: "house"
        case .library: "books.vertical"
        case .importItem: "square.and.arrow.down"
        case .settings: "slider.horizontal.3"
        }
    }
}

/// Semantic action router shared by touch navigation, keyboard shortcuts, and
/// native menu commands. A monotonically increasing request lets an already
/// visible Import screen observe repeated File > Import actions reliably.
@MainActor
@Observable
final class AppleAppRouter {
    var destination: AppleAppDestination
    private(set) var readerItemID: String?
    private(set) var fileImportRequest: UInt64
    private var consumedFileImportRequest: UInt64

    init(
        destination: AppleAppDestination = .home,
        fileImportRequest: UInt64 = 0
    ) {
        self.destination = destination
        readerItemID = nil
        self.fileImportRequest = fileImportRequest
        consumedFileImportRequest = 0
    }

    func navigate(to destination: AppleAppDestination) {
        readerItemID = nil
        self.destination = destination
    }

    func openReader(itemID: String) {
        readerItemID = itemID
    }

    func closeReader() {
        readerItemID = nil
    }

    func restoreDestination(rawValue: String?) {
        destination = rawValue.flatMap(AppleAppDestination.init(rawValue:)) ?? .home
    }

    func requestFileImport() {
        readerItemID = nil
        destination = .importItem
        fileImportRequest &+= 1
    }

    func consumeFileImportRequest(_ request: UInt64) -> Bool {
        guard request != 0,
              request == fileImportRequest,
              request != consumedFileImportRequest else {
            return false
        }
        consumedFileImportRequest = request
        return true
    }
}
