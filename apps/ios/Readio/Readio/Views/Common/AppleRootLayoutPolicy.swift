import SwiftUI

enum AppleRootPresentation: Equatable {
    case compactTabs
    case split
}

/// Keeps platform facts out of the root view's presentation tree.
///
/// macOS does not promise an iOS-style horizontal size class, but a desktop
/// window still needs the sidebar/split hierarchy even when that value is nil.
enum AppleRootLayoutPolicy {
    static func presentation(
        horizontalSizeClass: UserInterfaceSizeClass?
    ) -> AppleRootPresentation {
        #if os(macOS)
        .split
        #else
        horizontalSizeClass == .regular ? .split : .compactTabs
        #endif
    }
}

enum AppleReaderImagePhase: Equatable {
    case loading
    case loaded
    case failed
    case unresolved
}

enum AppleReaderImagePresentation: Equatable {
    case placeholder
    case content
    case fallback
}

enum AppleReaderToolbarVisibility: Equatable {
    case hidden
    case visible
}

struct AppleReaderHoverTargets: Equatable {
    let toolbar: Bool
    let playerDock: Bool
}

/// Small, executable layout rules for the reader and the desktop home shelves.
/// Keeping failure and sizing decisions here prevents transient SwiftUI state
/// from silently turning into permanent, page-sized visual artifacts.
enum AppleReaderSurfacePolicy {
    static let homeShelfHeaderGap: CGFloat = 12
    static let showsHomeScrollIndicator = false

    /// The topmost macOS pixels remain part of the draggable titlebar even
    /// when its native toolbar is hidden. Keep the reveal target deep enough
    /// to reach the reader content instead of leaving a visually present but
    /// pointer-inert strip at the window edge.
    static let toolbarRevealHeight: CGFloat = 44

    /// The dock stays quiet until the pointer reaches the bottom edge. Once it
    /// is visible, its full control surface remains interactive until the
    /// pointer returns to the reading canvas.
    static let playerDockRevealHeight: CGFloat = 22
    static let playerDockInteractionHeight: CGFloat = 112

    static func imagePresentation(
        for phase: AppleReaderImagePhase
    ) -> AppleReaderImagePresentation {
        switch phase {
        case .loading:
            .placeholder
        case .loaded:
            .content
        case .failed, .unresolved:
            .fallback
        }
    }

    static func remoteImageURL(_ source: String?) -> URL? {
        guard let source,
              let url = URL(string: source),
              let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https" else {
            return nil
        }
        return url
    }

    static var hidesNativeWindowToolbar: Bool {
        #if os(macOS)
        true
        #else
        false
        #endif
    }

    static func toolbarVisibility(
        pointerInside: Bool
    ) -> AppleReaderToolbarVisibility {
        pointerInside ? .visible : .hidden
    }

    static func playerDockVisibility(
        pointerInside: Bool
    ) -> AppleReaderToolbarVisibility {
        pointerInside ? .visible : .hidden
    }

    static func hoverTargets(
        locationY: CGFloat,
        viewportHeight: CGFloat,
        playerDockVisible: Bool = false
    ) -> AppleReaderHoverTargets {
        guard viewportHeight > 0,
              locationY >= 0,
              locationY <= viewportHeight else {
            return AppleReaderHoverTargets(toolbar: false, playerDock: false)
        }

        let playerDockHeight = playerDockVisible
            ? playerDockInteractionHeight
            : playerDockRevealHeight
        return AppleReaderHoverTargets(
            toolbar: locationY <= toolbarRevealHeight,
            playerDock: locationY >= max(viewportHeight - playerDockHeight, 0)
        )
    }

    static func homeShelfWidth(itemCount: Int, cardWidth: Double) -> Double {
        let visibleItems = max(itemCount, 1)
        let contentWidth = Double(visibleItems) * cardWidth
            + Double(max(visibleItems - 1, 0)) * 14
            + 32
        return min(max(contentWidth, 320), 860)
    }
}
