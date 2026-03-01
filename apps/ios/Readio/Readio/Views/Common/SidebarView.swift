import SwiftUI

// MARK: - SidebarView
/// The navigation sidebar used in the iPad layout (regular horizontal size class).
///
/// **Layout:**
/// ```
/// ┌───────────────────────┐
/// │  📖 Readio            │  ← App branding header
/// ├───────────────────────┤
/// │  🏠 Home              │  ← Navigation items
/// │  📚 Library           │     with SF Symbol icons
/// │  ⬇️ Import            │     and selection highlighting
/// │  ⚙️ Settings          │
/// ├───────────────────────┤
/// │                       │
/// │  v1.0.0               │  ← Version info at bottom
/// └───────────────────────┘
/// ```
///
/// **Usage:**
/// Used exclusively in `ContentView`'s iPad layout as the sidebar column of
/// a `NavigationSplitView`. The `selection` binding drives the detail column
/// content — when the user taps a sidebar item, the corresponding view is
/// rendered in the detail area.
///
/// **Design Notes:**
/// - Uses `List` with `selection` binding for native sidebar appearance.
/// - The "Readio" header provides brand identity in the sidebar.
/// - Section separators and styling follow Apple's sidebar conventions.
/// - Supports keyboard navigation (arrow keys + Return) for iPad with keyboard.
struct SidebarView: View {

    // MARK: - Properties

    /// Binding to the currently selected sidebar item.
    /// Drives the detail column content in the parent `NavigationSplitView`.
    @Binding var selection: ContentView.SidebarItem?

    // MARK: - Body

    var body: some View {
        List(selection: $selection) {
            // MARK: Navigation Section
            // The main navigation items, each with an icon and label.
            Section {
                ForEach(ContentView.SidebarItem.allCases) { item in
                    Label(item.title, systemImage: item.icon)
                        .tag(item)
                }
            } header: {
                // MARK: App Branding Header
                // Shows the app name with a book icon for visual identity.
                // Uses a large font to make it feel like a proper app header
                // rather than just another list section.
                HStack(spacing: 8) {
                    Image(systemName: "book.fill")
                        .font(.title3)
                        .foregroundStyle(.accentColor)

                    Text("Readio")
                        .font(.title3)
                        .fontWeight(.bold)
                        .foregroundStyle(.primary)
                }
                .padding(.vertical, 8)
                .textCase(nil) // Prevent the default uppercase section header behavior
            }

            // MARK: Footer Section
            // Version info at the bottom of the sidebar.
            Section {
                // Empty section — just shows the footer
            } footer: {
                VStack(spacing: 4) {
                    Text("Readio")
                        .font(.caption)
                        .fontWeight(.medium)
                    Text("Open Source Audiobook Platform")
                        .font(.caption2)
                }
                .foregroundStyle(.tertiary)
                .frame(maxWidth: .infinity, alignment: .center)
                .padding(.top, 12)
            }
        }
        .listStyle(.sidebar)
        .navigationTitle("") // Clear the navigation title — the header section handles branding
    }
}

// MARK: - Preview

#Preview {
    NavigationSplitView {
        SidebarView(selection: .constant(.home))
    } detail: {
        Text("Detail View")
    }
}
