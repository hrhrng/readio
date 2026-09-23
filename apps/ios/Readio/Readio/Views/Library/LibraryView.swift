import SwiftUI

/// Cover-led collection surface shared by iPad and macOS. Search and filtering
/// remain available, but stay quiet until requested so the library reads like a
/// shelf rather than a dashboard.
struct LibraryView: View {
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.readioTheme) private var theme
    @Environment(\.locale) private var locale

    @State private var viewModel = LibraryViewModel()
    @State private var isShowingBuiltinSample = false
    @State private var isSearchVisible = false
    @State private var pendingDeletion: LibraryItemSummary?
    @AppStorage("libraryViewStyle") private var isListView = false

    private let onImport: (() -> Void)?
    private let onOpenReader: ((String) -> Void)?

    init(
        onImport: (() -> Void)? = nil,
        onOpenReader: ((String) -> Void)? = nil
    ) {
        self.onImport = onImport
        self.onOpenReader = onOpenReader
    }

    private var isCompact: Bool { sizeClass == .compact }
    private var pageGutter: CGFloat { isCompact ? 20 : 48 }
    private var cardWidth: CGFloat { isCompact ? 150 : ReadioAppleLayout.bookCoverWidth }
    private var gridColumns: [GridItem] {
        [
            GridItem(
                .adaptive(minimum: cardWidth, maximum: isCompact ? 180 : 220),
                spacing: isCompact ? 18 : 44,
                alignment: .top
            )
        ]
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                pageHeader

                if isSearchVisible {
                    searchAndFilters
                        .padding(.top, 24)
                }

                if viewModel.isLoading && viewModel.items.isEmpty {
                    loadingView
                } else if viewModel.items.isEmpty {
                    emptyStateView
                } else {
                    if let item = viewModel.continueReadingItem {
                        continueReadingStrip(item)
                            .padding(.top, 36)
                    }

                    shelfHeader
                        .padding(.top, 42)
                        .padding(.bottom, 24)

                    collection
                }
            }
            .frame(maxWidth: ReadioAppleLayout.libraryContentMaxWidth, alignment: .leading)
            .padding(.horizontal, pageGutter)
            .padding(.top, isCompact ? 24 : 52)
            .padding(.bottom, 48)
            .frame(maxWidth: .infinity, alignment: .top)
        }
        .scrollContentBackground(.hidden)
        .background(theme.canvas.ignoresSafeArea())
        .tint(theme.accent)
        .navigationTitle("")
        .navigationDestination(for: String.self) { itemID in
            ReaderView(itemId: itemID)
        }
        .navigationDestination(isPresented: $isShowingBuiltinSample) {
            if let itemID = AppleBuiltinSampleEntry.destinationItemID(
                libraryIsEmpty: true,
                searchOrFilterIsActive: false
            ) {
                ReaderView(itemId: itemID)
            }
        }
        .task {
            await viewModel.loadItems()
        }
        .onReceive(NotificationCenter.default.publisher(for: .readioReplicaDidSync)) { _ in
            Task { await viewModel.refresh() }
        }
        .alert(item: $pendingDeletion) { item in
            let presentation = LibraryForgetPresentation.resolve(
                itemTitle: item.title,
                ownership: item.importRecord?.ownership,
                locale: locale
            )
            return Alert(
                title: Text(presentation.title),
                message: Text(presentation.message),
                primaryButton: .destructive(Text(presentation.actionLabel)) {
                    Task { try? await viewModel.deleteItem(id: item.id) }
                },
                secondaryButton: .cancel()
            )
        }
    }

    // MARK: - Header

    @ViewBuilder
    private var pageHeader: some View {
        if isCompact {
            VStack(alignment: .leading, spacing: 20) {
                titleBlock
                headerActions
            }
        } else {
            HStack(alignment: .top, spacing: 24) {
                titleBlock
                Spacer(minLength: 32)
                headerActions
            }
        }
    }

    private var titleBlock: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Library")
                .font(.system(size: isCompact ? 34 : 42, weight: .bold, design: .rounded))
                .tracking(-1.2)
                .foregroundStyle(theme.ink)

            Text(librarySummary)
                .font(.system(size: 16, weight: .regular))
                .foregroundStyle(theme.secondaryInk)
        }
    }

    private var librarySummary: String {
        let bookLabel = AppleLocalization.string(
            viewModel.totalItems == 1 ? "library.book.singular" : "library.book.plural",
            defaultValue: viewModel.totalItems == 1 ? "book" : "books",
            locale: locale
        )
        let progressLabel = AppleLocalization.string(
            "library.inProgress",
            defaultValue: "in progress",
            locale: locale
        )
        return "\(viewModel.totalItems) \(bookLabel) · \(viewModel.inProgressTotal) \(progressLabel)"
    }

    private var headerActions: some View {
        HStack(spacing: 10) {
            Button {
                withAnimation(.easeOut(duration: 0.18)) {
                    isSearchVisible.toggle()
                }
            } label: {
                Image(systemName: "magnifyingglass")
                    .frame(width: 40, height: 40)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(isSearchVisible ? theme.accent : theme.secondaryInk)
            .accessibilityLabel(
                AppleLocalization.string(
                    isSearchVisible ? "Hide search" : "Show search",
                    defaultValue: isSearchVisible ? "Hide search" : "Show search",
                    locale: locale
                )
            )

            viewToggleButton

            if let onImport {
                Button(action: onImport) {
                    Label("Import", systemImage: "plus")
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(theme.onAccent)
                        .padding(.horizontal, 18)
                        .frame(height: 44)
                        .background {
                            RoundedRectangle(cornerRadius: 10, style: .continuous)
                                .fill(theme.accent)
                        }
                }
                .buttonStyle(.plain)
                .keyboardShortcut("o", modifiers: .command)
            }
        }
    }

    // MARK: - Search and filters

    private var searchAndFilters: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 10) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(theme.secondaryInk)

                TextField("Search books", text: $viewModel.searchQuery)
                    .textFieldStyle(.plain)
                    .autocorrectionDisabled()
                    .readioSearchInputTraits()
                    .submitLabel(.search)
                    .onSubmit {
                        Task { await viewModel.search(viewModel.searchQuery) }
                    }

                if !viewModel.searchQuery.isEmpty {
                    Button {
                        viewModel.searchQuery = ""
                        Task { await viewModel.search("") }
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(theme.secondaryInk)
                    .accessibilityLabel("Clear search")
                }
            }
            .padding(.horizontal, 12)
            .frame(maxWidth: 420)
            .frame(height: 40)
            .background(theme.raised)
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(theme.border, lineWidth: 1)
            }

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    FilterChip(
                        title: String(localized: "All", locale: locale),
                        isSelected: viewModel.selectedType == nil
                    ) {
                        viewModel.selectedType = nil
                        Task { await viewModel.loadItems() }
                    }

                    ForEach(LibraryItemType.allCases, id: \.self) { type in
                        FilterChip(
                            title: type.rawValue.uppercased(),
                            isSelected: viewModel.selectedType == type.rawValue
                        ) {
                            viewModel.selectedType = type.rawValue
                            Task { await viewModel.loadItems() }
                        }
                    }
                }
            }
        }
    }

    // MARK: - Continue reading

    private func continueReadingStrip(_ item: LibraryItemSummary) -> some View {
        HStack(spacing: isCompact ? 14 : 22) {
            CoverImageView(
                coverImage: item.coverImage,
                title: item.title,
                size: CGSize(width: isCompact ? 48 : 64, height: isCompact ? 72 : 96),
                loadEpubAsset: { assetID in
                    try await LibraryService().loadEpubAsset(itemID: item.id, assetID: assetID)
                }
            )
            .clipShape(RoundedRectangle(cornerRadius: 5, style: .continuous))

            VStack(alignment: .leading, spacing: 8) {
                Text("Continue reading")
                    .font(.system(size: 14))
                    .foregroundStyle(theme.secondaryInk)

                Text(item.title)
                    .font(.system(size: isCompact ? 17 : 20, weight: .semibold))
                    .foregroundStyle(theme.ink)
                    .lineLimit(1)

                progressBar(item.progress, height: 4)
                    .frame(maxWidth: isCompact ? 180 : 360)
            }

            Spacer(minLength: 8)

            if !isCompact {
                readerLink(itemID: item.id) {
                    HStack(spacing: 10) {
                        Text("Resume")
                            .font(.system(size: 15, weight: .semibold))
                        Image(systemName: "arrow.right")
                    }
                    .foregroundStyle(theme.accent)
                    .padding(.horizontal, 20)
                    .frame(height: 48)
                    .background(theme.raised)
                    .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, isCompact ? 16 : 24)
        .padding(.vertical, isCompact ? 16 : 18)
        .background(theme.selected)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    // MARK: - Shelf

    private var shelfHeader: some View {
        HStack {
            Text("Recently added")
                .font(.system(size: 22, weight: .semibold))
                .foregroundStyle(theme.ink)

            Spacer()
            sortMenu
        }
    }

    @ViewBuilder
    private var collection: some View {
        if isListView {
            LazyVStack(spacing: 0) {
                ForEach(viewModel.items) { item in
                    readerLink(itemID: item.id) {
                        BookListRow(item: item)
                    }
                    .buttonStyle(.plain)
                    .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                        deleteButton(for: item)
                    }

                    Divider()
                        .overlay(theme.border)
                        .padding(.leading, 76)
                }
                paginationTrigger
            }
        } else {
            LazyVGrid(columns: gridColumns, alignment: .leading, spacing: isCompact ? 32 : 48) {
                ForEach(viewModel.items) { item in
                    readerLink(itemID: item.id) {
                        BookCardView(item: item, cardWidth: cardWidth)
                            .contextMenu {
                                deleteButton(for: item)
                            }
                    }
                    .buttonStyle(.plain)
                }
                paginationTrigger
            }
        }
    }

    private var paginationTrigger: some View {
        Group {
            if viewModel.isLoading {
                ProgressView()
                    .tint(theme.accent)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 20)
            } else if viewModel.hasMorePages {
                Color.clear
                    .frame(height: 1)
                    .onAppear {
                        Task { await viewModel.loadNextPage() }
                    }
            }
        }
    }

    private var sortMenu: some View {
        Menu {
            Section("Sort") {
                sortButton("Newest first", by: "created_at", order: "desc")
                sortButton("Oldest first", by: "created_at", order: "asc")
                sortButton("Title A–Z", by: "title", order: "asc")
                sortButton("Most progress", by: "progress", order: "desc")
            }
        } label: {
            Image(systemName: "ellipsis")
                .font(.system(size: 18, weight: .medium))
                .foregroundStyle(theme.secondaryInk)
                .frame(width: 40, height: 40)
                .contentShape(Rectangle())
        }
        .readioQuietMenuStyle()
        .accessibilityLabel("Sort library")
    }

    private func sortButton(
        _ title: LocalizedStringKey,
        by field: String,
        order: String
    ) -> some View {
        Button {
            viewModel.sortBy = field
            viewModel.sortOrder = order
            Task { await viewModel.loadItems() }
        } label: {
            Text(title)
            if viewModel.sortBy == field && viewModel.sortOrder == order {
                Image(systemName: "checkmark")
            }
        }
    }

    private var viewToggleButton: some View {
        Button {
            withAnimation(.easeOut(duration: 0.18)) {
                isListView.toggle()
            }
        } label: {
            Image(systemName: isListView ? "list.bullet" : "square.grid.2x2")
                .frame(width: 40, height: 40)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(theme.secondaryInk)
        .accessibilityLabel(
            AppleLocalization.string(
                isListView ? "Show grid" : "Show list",
                defaultValue: isListView ? "Show grid" : "Show list",
                locale: locale
            )
        )
    }

    private func deleteButton(for item: LibraryItemSummary) -> some View {
        Button(role: .destructive) {
            pendingDeletion = item
        } label: {
            Label(
                AppleLocalization.string(
                    item.importRecord?.ownership == .link ? "Remove" : "Delete",
                    defaultValue: item.importRecord?.ownership == .link ? "Remove" : "Delete",
                    locale: locale
                ),
                systemImage: "trash"
            )
        }
    }

    // MARK: - States

    private var loadingView: some View {
        HStack(spacing: isCompact ? 18 : 44) {
            ForEach(0..<(isCompact ? 2 : 3), id: \.self) { _ in
                VStack(alignment: .leading, spacing: 10) {
                    RoundedRectangle(cornerRadius: 6)
                        .fill(theme.hover)
                        .aspectRatio(2.0 / 3.0, contentMode: .fit)
                    RoundedRectangle(cornerRadius: 3)
                        .fill(theme.hover)
                        .frame(height: 16)
                    RoundedRectangle(cornerRadius: 2)
                        .fill(theme.hover)
                        .frame(width: 90, height: 12)
                }
                .frame(width: cardWidth)
            }
        }
        .redacted(reason: .placeholder)
        .padding(.top, 48)
    }

    private var emptyStateView: some View {
        Group {
            if !viewModel.searchQuery.isEmpty {
                EmptyStateView(
                    icon: "magnifyingglass",
                    title: AppleLocalization.string(
                        "library.noResults",
                        defaultValue: "No results",
                        locale: locale
                    ),
                    message: AppleLocalization.formatted(
                        "library.noBooksMatch.format",
                        defaultValue: "No books match “%1$@”. Try another search.",
                        locale: locale,
                        arguments: [viewModel.searchQuery]
                    )
                )
            } else if let type = viewModel.selectedType {
                EmptyStateView(
                    icon: "doc",
                    title: AppleLocalization.formatted(
                        "library.noTypeBooks.format",
                        defaultValue: "No %1$@ books",
                        locale: locale,
                        arguments: [type.uppercased()]
                    ),
                    message: AppleLocalization.formatted(
                        "library.importType.format",
                        defaultValue: "Import a %1$@ file to add it to this shelf.",
                        locale: locale,
                        arguments: [type.uppercased()]
                    )
                )
            } else {
                EmptyStateView(
                    icon: "book.closed",
                    title: String(localized: "Your library is empty", locale: locale),
                    message: String(
                        localized: "Import a book or open the built-in sample to start reading.",
                        locale: locale
                    ),
                    actionLabel: String(localized: "Open Sample", locale: locale)
                ) {
                    guard let itemID = AppleBuiltinSampleEntry.destinationItemID(
                        libraryIsEmpty: true,
                        searchOrFilterIsActive: false
                    ) else { return }
                    if let onOpenReader {
                        onOpenReader(itemID)
                    } else {
                        isShowingBuiltinSample = true
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, minHeight: 460)
    }

    private func progressBar(_ progress: Int, height: CGFloat) -> some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(theme.highlight.opacity(0.72))
                Capsule()
                    .fill(theme.accent)
                    .frame(width: geometry.size.width * CGFloat(min(max(progress, 0), 100)) / 100)
            }
        }
        .frame(height: height)
        .accessibilityLabel(
            AppleLocalization.formatted(
                "reader.percentRead.format",
                defaultValue: "%1$lld percent read",
                locale: locale,
                arguments: [progress]
            )
        )
    }

    @ViewBuilder
    private func readerLink<Label: View>(
        itemID: String,
        @ViewBuilder label: () -> Label
    ) -> some View {
        if let onOpenReader {
            Button {
                onOpenReader(itemID)
            } label: {
                label()
            }
        } else {
            NavigationLink(value: itemID) {
                label()
            }
        }
    }
}

private extension View {
    @ViewBuilder
    func readioSearchInputTraits() -> some View {
        #if os(iOS)
        textInputAutocapitalization(.never)
        #else
        self
        #endif
    }

    @ViewBuilder
    func readioQuietMenuStyle() -> some View {
        #if os(macOS)
        menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
        #else
        self
        #endif
    }
}

private struct FilterChip: View {
    @Environment(\.readioTheme) private var theme
    let title: String
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(isSelected ? theme.ink : theme.secondaryInk)
                .padding(.horizontal, 12)
                .frame(height: 32)
                .background(isSelected ? theme.selected : theme.hover)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(.plain)
    }
}

#Preview {
    NavigationStack {
        LibraryView(onImport: {})
    }
}
