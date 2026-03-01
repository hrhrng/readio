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

    // MARK: - State

    /// The currently selected tab in the iPhone TabView layout.
    @State private var selectedTab: Tab = .home

    /// The currently selected sidebar item in the iPad NavigationSplitView layout.
    /// Optional because nothing may be selected when the sidebar first appears.
    @State private var selectedSidebarItem: SidebarItem? = .home

    // MARK: - Tab / Sidebar Enums

    /// Represents the four top-level navigation destinations in the TabView.
    enum Tab: Hashable {
        case home
        case library
        case importItem
        case settings
    }

    /// Represents sidebar navigation items for the iPad layout.
    /// Conforms to `CaseIterable` for easy sidebar list generation and `Identifiable`
    /// for SwiftUI's `List` selection binding.
    enum SidebarItem: String, CaseIterable, Identifiable {
        case home
        case library
        case importItem
        case settings

        var id: String { rawValue }

        /// Human-readable label displayed in the sidebar row.
        var title: String {
            switch self {
            case .home: return "Home"
            case .library: return "Library"
            case .importItem: return "Import"
            case .settings: return "Settings"
            }
        }

        /// SF Symbol name for the sidebar row icon.
        var icon: String {
            switch self {
            case .home: return "house.fill"
            case .library: return "books.vertical.fill"
            case .importItem: return "square.and.arrow.down"
            case .settings: return "gearshape.fill"
            }
        }
    }

    // MARK: - Body

    var body: some View {
        if sizeClass == .regular {
            // MARK: iPad Layout — NavigationSplitView
            iPadLayout
        } else {
            // MARK: iPhone Layout — TabView
            iPhoneLayout
        }
    }

    // MARK: - iPad Layout

    /// iPad layout using a two-column NavigationSplitView.
    ///
    /// The sidebar column is 220pt wide and lists all navigation destinations.
    /// The detail column renders the view corresponding to the selected sidebar item.
    /// If no item is selected, a placeholder is shown.
    private var iPadLayout: some View {
        NavigationSplitView {
            SidebarView(selection: $selectedSidebarItem)
        } detail: {
            // Wrap detail in NavigationStack so child views can push
            // (e.g., BookCardView → ReaderView) within the detail column.
            NavigationStack {
                switch selectedSidebarItem {
                case .home:
                    HomeView()
                case .library:
                    LibraryView()
                case .importItem:
                    ImportView()
                case .settings:
                    SettingsView()
                case nil:
                    // No selection — show a gentle welcome placeholder
                    ContentUnavailableView(
                        "Welcome to Readio",
                        systemImage: "book.fill",
                        description: Text("Select a section from the sidebar to get started.")
                    )
                }
            }
        }
    }

    // MARK: - iPhone Layout

    /// iPhone layout using a standard TabView with four tabs.
    ///
    /// Each tab wraps its content in an independent `NavigationStack` so that
    /// navigation pushes within one tab do not affect other tabs. For example,
    /// navigating from a BookCard to ReaderView in the Home tab leaves the
    /// Library tab's navigation stack untouched.
    private var iPhoneLayout: some View {
        TabView(selection: $selectedTab) {
            // MARK: Home Tab
            NavigationStack {
                HomeView()
            }
            .tabItem {
                Label("Home", systemImage: "house.fill")
            }
            .tag(Tab.home)

            // MARK: Library Tab
            NavigationStack {
                LibraryView()
            }
            .tabItem {
                Label("Library", systemImage: "books.vertical.fill")
            }
            .tag(Tab.library)

            // MARK: Import Tab
            NavigationStack {
                ImportView()
            }
            .tabItem {
                Label("Import", systemImage: "square.and.arrow.down")
            }
            .tag(Tab.importItem)

            // MARK: Settings Tab
            NavigationStack {
                SettingsView()
            }
            .tabItem {
                Label("Settings", systemImage: "gearshape.fill")
            }
            .tag(Tab.settings)
        }
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
