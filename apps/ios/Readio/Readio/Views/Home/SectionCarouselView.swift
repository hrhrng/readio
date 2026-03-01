import SwiftUI

// MARK: - SectionCarouselView
/// A horizontal scrolling section with a title header and optional "See All" navigation link.
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │  Section Title ────────── See All >  │
/// │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐  │
/// │  │Card1│ │Card2│ │Card3│ │Card4│  ← │  Horizontal scroll
/// │  └─────┘ └─────┘ └─────┘ └─────┘  │
/// └──────────────────────────────────────┘
/// ```
///
/// **Usage:**
/// Used on the `HomeView` dashboard to display "Continue Reading" and "Recently Added"
/// sections. Each card is a `BookCardView` wrapped in a `NavigationLink` that pushes
/// to the `ReaderView` for that item.
///
/// **Behavior:**
/// - Scrolls horizontally with snap-like behavior (using `.scrollTargetBehavior`).
/// - The "See All" button navigates to the Library tab/view with a pre-applied filter.
/// - If `items` is empty, shows a compact inline empty state rather than hiding entirely
///   (the parent `HomeView` handles section visibility).
struct SectionCarouselView: View {

    // MARK: - Properties

    /// The section header title (e.g., "Continue Reading", "Recently Added").
    let title: String

    /// The items to display as book cards in the horizontal scroller.
    let items: [LibraryItemSummary]

    // MARK: - Body

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            // MARK: Section Header
            // HStack with title on the left and "See All" on the right.
            sectionHeader

            if items.isEmpty {
                // MARK: Inline Empty State
                // Shown when the section has no items (e.g., no in-progress books).
                inlineEmptyState
            } else {
                // MARK: Horizontal Carousel
                // ScrollView with horizontal axis showing BookCardView items.
                horizontalCarousel
            }
        }
    }

    // MARK: - Section Header

    /// The title row with a "See All" navigation link on the trailing side.
    private var sectionHeader: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title)
                .font(.title3)
                .fontWeight(.bold)
                .foregroundStyle(.primary)

            Spacer()

            // "See All" navigates to the full library view.
            // In a real implementation, this would pass a filter parameter
            // so the LibraryView pre-filters to the relevant subset.
            NavigationLink {
                LibraryView()
            } label: {
                HStack(spacing: 2) {
                    Text("See All")
                        .font(.subheadline)
                        .fontWeight(.medium)
                    Image(systemName: "chevron.right")
                        .font(.caption)
                }
                .foregroundStyle(.accentColor)
            }
        }
        .padding(.horizontal)
    }

    // MARK: - Horizontal Carousel

    /// The horizontally scrolling list of book cards.
    ///
    /// Each card is wrapped in a `NavigationLink` that navigates to the `ReaderView`
    /// when tapped. The scroll view uses `.scrollTargetBehavior(.viewAligned)` on
    /// iOS 17+ for a snapping effect, and disables scroll indicators for a cleaner look.
    private var horizontalCarousel: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            LazyHStack(spacing: 14) {
                ForEach(items) { item in
                    NavigationLink(value: item.id) {
                        BookCardView(item: item)
                    }
                    .buttonStyle(.plain) // Prevents the default blue tint on the card
                }
            }
            .padding(.horizontal)
            .scrollTargetLayout() // Enables snapping with `.scrollTargetBehavior`
        }
        .scrollTargetBehavior(.viewAligned) // Snap cards into alignment when scrolling
        .navigationDestination(for: String.self) { itemId in
            // Navigate to the reader when a book card is tapped.
            // The `itemId` is passed as a navigation value.
            ReaderView(itemId: itemId)
        }
    }

    // MARK: - Inline Empty State

    /// A compact empty state shown inline when the section has no items.
    /// Less prominent than a full-screen empty state — just a gentle hint.
    private var inlineEmptyState: some View {
        HStack {
            Spacer()
            VStack(spacing: 6) {
                Image(systemName: "tray")
                    .font(.title2)
                    .foregroundStyle(.secondary)
                Text("No items yet")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            .padding(.vertical, 30)
            Spacer()
        }
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(.secondarySystemGroupedBackground))
        )
        .padding(.horizontal)
    }
}

// MARK: - Preview

#Preview {
    NavigationStack {
        VStack(spacing: 24) {
            SectionCarouselView(
                title: "Continue Reading",
                items: [
                    LibraryItemSummary(
                        id: "1", title: "Swift Programming", type: .epub, progress: 45,
                        date: "2024-01-15", category: .imported, folderId: "f1",
                        source: "upload:swift.epub", filePath: nil, coverImage: nil,
                        voice: nil, speed: nil, contentFormat: nil
                    ),
                    LibraryItemSummary(
                        id: "2", title: "Design Patterns", type: .pdf, progress: 72,
                        date: "2024-01-10", category: .imported, folderId: "f1",
                        source: "upload:patterns.pdf", filePath: nil, coverImage: nil,
                        voice: nil, speed: nil, contentFormat: nil
                    ),
                ]
            )

            SectionCarouselView(
                title: "Recently Added",
                items: []
            )
        }
    }
}
