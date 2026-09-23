import SwiftUI

// MARK: - ContentView
/// Adaptive root layout that responds to the device's horizontal size class.
///
/// **Layout Strategy:**
/// - **iPad (regular width):** Uses `NavigationSplitView` with a persistent sidebar
///   on the leading edge, providing a desktop-like experience. The sidebar shows
///   navigation items (Home, Library, Import, Settings) and the detail area renders
///   the selected section.
/// - **iPhone (compact width):** Uses a standard `TabView` with four tabs, each
///   wrapping its content in its own `NavigationStack` for independent navigation
///   hierarchies (e.g., tapping a book in Library pushes a ReaderView without
///   affecting the Home tab's navigation state).
///
/// This pattern follows Apple's Human Interface Guidelines for adaptive navigation
/// and works seamlessly across iPhone SE through iPad Pro 13".
struct ContentView: View {

    // MARK: - Environment

    /// The current horizontal size class, injected by SwiftUI.
    /// - `.compact`: iPhone portrait, iPhone landscape (non-Plus), iPad slide-over
    /// - `.regular`: iPad full-screen, iPad split-view (2/3+), iPhone Plus landscape
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.readioTheme) private var theme

    // MARK: - Navigation

    /// Each window owns its own router. Native commands receive the focused
    /// window's instance through `FocusedValues` rather than a process singleton.
    @State private var router = AppleAppRouter()

    /// Scene-scoped persistence keeps independent macOS windows on their own
    /// destination when the system restores them.
    @SceneStorage("readio.rootDestination") private var storedDestination =
        AppleAppDestination.home.rawValue

    // MARK: - Body

    var body: some View {
        Group {
            if let readerItemID = router.readerItemID {
                ReaderView(itemId: readerItemID) {
                    router.closeReader()
                }
            } else {
                switch AppleRootLayoutPolicy.presentation(horizontalSizeClass: sizeClass) {
                case .split:
                    // MARK: iPad/macOS Layout — NavigationSplitView
                    iPadLayout
                case .compactTabs:
                    // MARK: iPhone Layout — TabView
                    iPhoneLayout
                }
            }
        }
        .tint(theme.accent)
        .foregroundStyle(theme.ink)
        .background(theme.canvas.ignoresSafeArea())
        .readioDesktopMinimumSize()
        .task {
            _ = try? await AppleCloudService.shared.syncIfConnected()
        }
        .onAppear {
            router.restoreDestination(rawValue: storedDestination)
        }
        .onChange(of: router.destination) { _, destination in
            storedDestination = destination.rawValue
        }
        .focusedSceneValue(\.readioAppRouter, router)
    }

    // MARK: - iPad Layout

    /// iPad layout using a two-column NavigationSplitView.
    ///
    /// The sidebar column is 220pt wide and lists all navigation destinations.
    /// The detail column renders the view corresponding to the selected sidebar item.
    /// If no item is selected, a placeholder is shown.
    private var iPadLayout: some View {
        NavigationSplitView {
            SidebarView(selection: sidebarSelection)
        } detail: {
            // Wrap detail in NavigationStack so child views can push
            // (e.g., BookCardView → ReaderView) within the detail column.
            NavigationStack {
                switch router.destination {
                case .home:
                    HomeView(onOpenReader: router.openReader)
                case .library:
                    LibraryView(
                        onImport: { router.navigate(to: .importItem) },
                        onOpenReader: router.openReader
                    )
                case .importItem:
                    ImportView(router: router)
                case .settings:
                    SettingsView()
                }
            }
        }
    }

    private var sidebarSelection: Binding<AppleAppDestination?> {
        Binding(
            get: { router.destination },
            set: { destination in
                if let destination {
                    router.navigate(to: destination)
                }
            }
        )
    }

    // MARK: - iPhone Layout

    /// iPhone layout using a standard TabView with four tabs.
    ///
    /// Each tab wraps its content in an independent `NavigationStack` so that
    /// navigation pushes within one tab do not affect other tabs. For example,
    /// navigating from a BookCard to ReaderView in the Home tab leaves the
    /// Library tab's navigation stack untouched.
    private var iPhoneLayout: some View {
        TabView(selection: $router.destination) {
            // MARK: Home Tab
            NavigationStack {
                HomeView()
            }
            .tabItem {
                Label("Home", systemImage: "house.fill")
            }
            .tag(AppleAppDestination.home)

            // MARK: Library Tab
            NavigationStack {
                LibraryView()
            }
            .tabItem {
                Label("Library", systemImage: "books.vertical.fill")
            }
            .tag(AppleAppDestination.library)

            // MARK: Import Tab
            NavigationStack {
                ImportView(router: router)
            }
            .tabItem {
                Label("Import", systemImage: "square.and.arrow.down")
            }
            .tag(AppleAppDestination.importItem)

            // MARK: Settings Tab
            NavigationStack {
                SettingsView()
            }
            .tabItem {
                Label("Settings", systemImage: "gearshape.fill")
            }
            .tag(AppleAppDestination.settings)
        }
    }
}

private extension View {
    @ViewBuilder
    func readioDesktopMinimumSize() -> some View {
        #if os(macOS)
        frame(minWidth: 1_080, minHeight: 720)
        #else
        self
        #endif
    }
}

// MARK: - Preview

#Preview("iPhone") {
    ContentView()
}

#Preview("iPad") {
    ContentView()
        .previewDevice("iPad Pro 11-inch (M4)")
}
