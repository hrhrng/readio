import SwiftUI

// MARK: - LibraryView
/// The main library screen showing all imported items in a filterable, sortable grid.
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │  Library                 (nav title) │
/// ├──────────────────────────────────────┤
/// │  🔍 Search books...                  │
/// │                                      │
/// │  [All] [EPUB] [PDF] [TXT] [Web]      │  ← Filter tabs
/// │                                      │
/// │  ┌─────┐ ┌─────┐ ┌─────┐           │
/// │  │Card │ │Card │ │Card │           │  ← Adaptive grid
/// │  └─────┘ └─────┘ └─────┘           │
/// │  ┌─────┐ ┌─────┐ ┌─────┐           │
/// │  │Card │ │Card │ │Card │           │
/// │  └─────┘ └─────┘ └─────┘           │
/// │                                      │
/// │  ⏳ Loading more...                  │  ← Pagination indicator
/// └──────────────────────────────────────┘
/// ```
///
/// **Features:**
/// - Search bar with debounced server-side search
/// - Filter tabs to narrow by document type (EPUB, PDF, TXT, Web)
/// - Sort menu accessible from the toolbar (date, title, progress)
/// - Adaptive grid layout (minimum 160pt columns) for all screen sizes
/// - Infinite scroll pagination — loads the next page when the last item appears
/// - Pull-to-refresh for manual reload
/// - Swipe-to-delete on grid items
/// - Full-screen empty state when no items match the current filter/search
struct LibraryView: View {

    // MARK: - State

    /// The view model managing library data, pagination, search, and filtering.
    @State private var viewModel = LibraryViewModel()

    /// Controls whether the grid displays in list mode vs. grid mode.
    /// Persisted across app launches via `@AppStorage`.
    @AppStorage("libraryViewStyle") private var isListView = false

    /// The columns configuration for the adaptive grid.
    /// Minimum 160pt per column ensures cards look good on all screen sizes.
    private let gridColumns = [
        GridItem(.adaptive(minimum: 160, maximum: 200), spacing: 16)
    ]

    // MARK: - Body

    var body: some View {
        VStack(spacing: 0) {
            // MARK: Search Bar
            searchBar

            // MARK: Filter Tabs
            filterTabs

            // MARK: Content Area
            if viewModel.isLoading && viewModel.items.isEmpty {
                // First load — show loading placeholder
                loadingView
            } else if viewModel.items.isEmpty {
                // No items match current filter/search
                emptyStateView
            } else {
                // Normal content — grid or list of items
                contentView
            }
        }
        .navigationTitle("Library")
        .toolbar {
            // MARK: Toolbar — Sort Menu & View Toggle
            ToolbarItemGroup(placement: .topBarTrailing) {
                sortMenu
                viewToggleButton
            }
        }
        .task {
            // Initial data load when the view appears.
            await viewModel.loadItems()
        }
    }

    // MARK: - Search Bar

    /// A search field that triggers server-side search with debouncing.
    ///
    /// The `searchQuery` binding on `viewModel` is debounced internally (typically
    /// 300ms) so we don't flood the server with requests on every keystroke.
    private var searchBar: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)

            TextField("Search books...", text: $viewModel.searchQuery)
                .textFieldStyle(.plain)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .submitLabel(.search)
                .onSubmit {
                    // The ViewModel's search(_:) method takes a query parameter.
                    // We pass the current searchQuery value.
                    Task { await viewModel.search(viewModel.searchQuery) }
                }

            // Clear button — only shown when there is text to clear
            if !viewModel.searchQuery.isEmpty {
                Button {
                    viewModel.searchQuery = ""
                    // Clearing the query and reloading shows all items again.
                    Task { await viewModel.search("") }
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Color(.secondarySystemGroupedBackground))
        )
        .padding(.horizontal)
        .padding(.top, 8)
    }

    // MARK: - Filter Tabs

    /// Horizontally scrolling filter pills for document type filtering.
    ///
    /// "All" shows everything; tapping a specific type (EPUB, PDF, etc.) sets
    /// `viewModel.selectedType` which triggers a filtered reload from the server.
    private var filterTabs: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                // "All" filter — clears the type filter
                FilterChip(
                    title: "All",
                    isSelected: viewModel.selectedType == nil,
                    action: {
                        viewModel.selectedType = nil
                        Task { await viewModel.loadItems() }
                    }
                )

                // One chip per document type.
                // Note: viewModel.selectedType is String? (not LibraryItemType?),
                // so we compare and assign the raw string value.
                ForEach(LibraryItemType.allCases, id: \.self) { type in
                    FilterChip(
                        title: type.rawValue.uppercased(),
                        isSelected: viewModel.selectedType == type.rawValue,
                        action: {
                            viewModel.selectedType = type.rawValue
                            Task { await viewModel.loadItems() }
                        }
                    )
                }
            }
            .padding(.horizontal)
            .padding(.vertical, 10)
        }
    }

    // MARK: - Content View (Grid / List)

    /// The main scrollable content area containing the library items.
    /// Supports both grid and list layouts, toggled by the toolbar button.
    private var contentView: some View {
        ScrollView {
            if isListView {
                // MARK: List Layout
                LazyVStack(spacing: 0) {
                    ForEach(viewModel.items) { item in
                        NavigationLink(value: item.id) {
                            BookListRow(item: item)
                        }
                        .buttonStyle(.plain)
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            deleteSwipeButton(for: item)
                        }

                        Divider()
                            .padding(.leading, 80)
                    }

                    // Pagination trigger — load more when the last item appears
                    paginationTrigger
                }
            } else {
                // MARK: Grid Layout
                LazyVGrid(columns: gridColumns, spacing: 20) {
                    ForEach(viewModel.items) { item in
                        NavigationLink(value: item.id) {
                            BookCardView(item: item)
                                .contextMenu {
                                    deleteContextMenuButton(for: item)
                                }
                        }
                        .buttonStyle(.plain)
                    }

                    // Pagination trigger — load more when the last item appears
                    paginationTrigger
                }
                .padding(.horizontal)
                .padding(.top, 8)
            }
        }
        .refreshable {
            await viewModel.refresh()
        }
        .navigationDestination(for: String.self) { itemId in
            ReaderView(itemId: itemId)
        }
    }

    // MARK: - Pagination Trigger

    /// An invisible view placed at the bottom of the list/grid.
    /// When it appears on screen, triggers loading the next page of results.
    private var paginationTrigger: some View {
        Group {
            if viewModel.isLoading {
                // Show a loading spinner at the bottom while fetching the next page
                HStack {
                    Spacer()
                    ProgressView()
                        .padding()
                    Spacer()
                }
            } else {
                // Invisible trigger — `.onAppear` fires when this view scrolls into the viewport
                Color.clear
                    .frame(height: 1)
                    .onAppear {
                        Task { await viewModel.loadNextPage() }
                    }
            }
        }
    }

    // MARK: - Sort Menu

    /// Toolbar menu for selecting sort criteria and direction.
    /// Options: Date (newest/oldest), Title (A-Z/Z-A), Progress (most/least).
    private var sortMenu: some View {
        Menu {
            Section("Sort By") {
                Button {
                    viewModel.sortBy = "date"
                    viewModel.sortOrder = "desc"
                    Task { await viewModel.loadItems() }
                } label: {
                    Label("Newest First", systemImage: "calendar.badge.clock")
                    if viewModel.sortBy == "date" && viewModel.sortOrder == "desc" {
                        Image(systemName: "checkmark")
                    }
                }

                Button {
                    viewModel.sortBy = "date"
                    viewModel.sortOrder = "asc"
                    Task { await viewModel.loadItems() }
                } label: {
                    Label("Oldest First", systemImage: "calendar")
                    if viewModel.sortBy == "date" && viewModel.sortOrder == "asc" {
                        Image(systemName: "checkmark")
                    }
                }

                Divider()

                Button {
                    viewModel.sortBy = "title"
                    viewModel.sortOrder = "asc"
                    Task { await viewModel.loadItems() }
                } label: {
                    Label("Title A-Z", systemImage: "textformat.abc")
                    if viewModel.sortBy == "title" && viewModel.sortOrder == "asc" {
                        Image(systemName: "checkmark")
                    }
                }

                Button {
                    viewModel.sortBy = "title"
                    viewModel.sortOrder = "desc"
                    Task { await viewModel.loadItems() }
                } label: {
                    Label("Title Z-A", systemImage: "textformat.abc")
                    if viewModel.sortBy == "title" && viewModel.sortOrder == "desc" {
                        Image(systemName: "checkmark")
                    }
                }

                Divider()

                Button {
                    viewModel.sortBy = "progress"
                    viewModel.sortOrder = "desc"
                    Task { await viewModel.loadItems() }
                } label: {
                    Label("Most Progress", systemImage: "chart.bar.fill")
                    if viewModel.sortBy == "progress" && viewModel.sortOrder == "desc" {
                        Image(systemName: "checkmark")
                    }
                }
            }
        } label: {
            Image(systemName: "arrow.up.arrow.down")
        }
    }

    // MARK: - View Toggle Button

    /// Toggles between grid and list display modes.
    private var viewToggleButton: some View {
        Button {
            withAnimation(.easeInOut(duration: 0.2)) {
                isListView.toggle()
            }
        } label: {
            Image(systemName: isListView ? "square.grid.2x2" : "list.bullet")
        }
    }

    // MARK: - Delete Actions

    /// Swipe-to-delete button for list rows.
    /// The ViewModel's `deleteItem(id:)` throws on server error, but we handle
    /// errors silently here (the ViewModel shows an error state internally).
    private func deleteSwipeButton(for item: LibraryItemSummary) -> some View {
        Button(role: .destructive) {
            Task { try? await viewModel.deleteItem(id: item.id) }
        } label: {
            Label("Delete", systemImage: "trash")
        }
    }

    /// Context menu delete button for grid cards.
    private func deleteContextMenuButton(for item: LibraryItemSummary) -> some View {
        Button(role: .destructive) {
            Task { try? await viewModel.deleteItem(id: item.id) }
        } label: {
            Label("Delete", systemImage: "trash")
        }
    }

    // MARK: - Loading View

    /// Full-screen loading placeholder shown during the initial data fetch.
    private var loadingView: some View {
        VStack {
            Spacer()
            ProgressView("Loading library...")
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Spacer()
        }
    }

    // MARK: - Empty State

    /// Shown when no items match the current filter or search query.
    /// Message varies based on whether a search/filter is active.
    private var emptyStateView: some View {
        VStack {
            Spacer()
            if !viewModel.searchQuery.isEmpty {
                EmptyStateView(
                    icon: "magnifyingglass",
                    title: "No Results",
                    message: "No items match \"\(viewModel.searchQuery)\". Try a different search term."
                )
            } else if let typeFilter = viewModel.selectedType {
                EmptyStateView(
                    icon: "doc",
                    title: "No \(typeFilter.uppercased()) Items",
                    message: "You haven't imported any \(typeFilter.uppercased()) files yet."
                )
            } else {
                EmptyStateView(
                    icon: "book.closed.fill",
                    title: "Your Library is Empty",
                    message: "Import files or paste a URL to start building your library."
                )
            }
            Spacer()
        }
    }
}

// MARK: - FilterChip
/// A small pill-shaped button used as a filter tab in the library toolbar.
///
/// Selected state uses the accent color with white text; unselected uses
/// a subtle background with primary text. Animates the transition.
struct FilterChip: View {

    /// The label text displayed inside the chip.
    let title: String

    /// Whether this chip is currently the active filter.
    let isSelected: Bool

    /// Action to perform when the chip is tapped.
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.subheadline)
                .fontWeight(isSelected ? .semibold : .medium)
                .foregroundStyle(isSelected ? .white : .primary)
                .padding(.horizontal, 14)
                .padding(.vertical, 6)
                .background(
                    Capsule()
                        .fill(isSelected ? Color.accentColor : Color(.secondarySystemGroupedBackground))
                )
        }
        .buttonStyle(.plain)
        .animation(.easeInOut(duration: 0.15), value: isSelected)
    }
}

// MARK: - Preview

#Preview {
    NavigationStack {
        LibraryView()
    }
}
