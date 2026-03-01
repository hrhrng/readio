import SwiftUI

// MARK: - BookCardView
/// A card-style view displaying a library item's cover, title, progress, and type badge.
///
/// **Design:**
/// ```
/// ┌─────────────────────────┐
/// │  ┌───────────────┐ [PDF]│  ← Type badge in top-right corner
/// │  │               │      │
/// │  │  Cover Image  │      │  ← CoverImageView or gradient fallback
/// │  │  or Gradient   │      │
/// │  │               │      │
/// │  └───────────────┘      │
/// │  Book Title Goes Here   │  ← 2-line max, truncated
/// │  ━━━━━━━━░░░░░░░░░  45% │  ← Progress bar (hidden if progress == 0)
/// └─────────────────────────┘
/// ```
///
/// **Dimensions:** 160pt wide x 220pt tall (golden ratio-ish proportion).
///
/// **Usage:** Used in `SectionCarouselView` horizontal scrollers and `LibraryView` grids.
/// Wrapped in a `NavigationLink` by the parent to navigate to `ReaderView` on tap.
struct BookCardView: View {

    // MARK: - Properties

    /// The library item summary to display. Does not include full content.
    let item: LibraryItemSummary

    /// Card dimensions — consistent across all usage contexts.
    private let cardWidth: CGFloat = 160
    private let cardHeight: CGFloat = 220

    // MARK: - Body

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            // MARK: Cover Image Area
            // The cover takes up most of the card height, with the title and
            // progress bar occupying the bottom portion.
            ZStack(alignment: .topTrailing) {
                CoverImageView(
                    coverImage: item.coverImage,
                    title: item.title,
                    size: CGSize(width: cardWidth, height: cardHeight * 0.65)
                )
                .clipShape(RoundedRectangle(cornerRadius: 10))

                // MARK: Type Badge
                // Small pill showing the document format (EPUB, PDF, TXT, WEB).
                // Positioned in the top-right corner with a slight offset.
                typeBadge
                    .padding(6)
            }

            // MARK: Title
            // Two-line maximum with tail truncation. Uses the `.headline` style
            // for readability at the card's width.
            Text(item.title)
                .font(.caption)
                .fontWeight(.medium)
                .lineLimit(2)
                .multilineTextAlignment(.leading)
                .foregroundStyle(.primary)

            // MARK: Progress Bar
            // Only shown when the item has been started (progress > 0).
            // A thin bar gives a quick visual indication of reading progress.
            if item.progress > 0 {
                progressBar
            }

            Spacer(minLength: 0)
        }
        .frame(width: cardWidth, height: cardHeight)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(.secondarySystemGroupedBackground))
                .shadow(color: .black.opacity(0.08), radius: 6, x: 0, y: 2)
        )
    }

    // MARK: - Type Badge

    /// Small rounded pill displaying the item's document type.
    /// Color-coded for quick visual distinction:
    /// - EPUB: indigo (rich content)
    /// - PDF: red (classic PDF color)
    /// - TXT: gray (plain text)
    /// - Web: blue (internet content)
    private var typeBadge: some View {
        Text(item.type.rawValue.uppercased())
            .font(.system(size: 9, weight: .bold, design: .rounded))
            .foregroundStyle(.white)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(
                Capsule()
                    .fill(badgeColor)
            )
    }

    /// Returns the badge background color corresponding to the item's document type.
    private var badgeColor: Color {
        switch item.type {
        case .epub:
            return .indigo
        case .pdf:
            return .red
        case .txt:
            return .gray
        case .web:
            return .blue
        }
    }

    // MARK: - Progress Bar

    /// A thin horizontal progress bar showing reading completion percentage.
    /// The filled portion uses the app's accent color for visual consistency.
    private var progressBar: some View {
        VStack(alignment: .leading, spacing: 2) {
            GeometryReader { geometry in
                ZStack(alignment: .leading) {
                    // Background track
                    RoundedRectangle(cornerRadius: 2)
                        .fill(Color(.systemGray5))
                        .frame(height: 3)

                    // Filled portion — width proportional to progress percentage
                    RoundedRectangle(cornerRadius: 2)
                        .fill(Color.accentColor)
                        .frame(
                            width: geometry.size.width * CGFloat(item.progress) / 100.0,
                            height: 3
                        )
                }
            }
            .frame(height: 3)

            // Progress percentage text
            Text("\(item.progress)%")
                .font(.system(size: 9, weight: .medium, design: .rounded))
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 2)
    }
}

// MARK: - Preview

#Preview("Book Card — EPUB with Progress") {
    BookCardView(
        item: LibraryItemSummary(
            id: "preview-1",
            title: "The Great Gatsby: A Novel That Changed Literature",
            type: .epub,
            progress: 45,
            date: "2024-01-15",
            category: .imported,
            folderId: "f1",
            source: "upload:gatsby.epub",
            filePath: nil,
            coverImage: nil,
            voice: nil,
            speed: nil,
            contentFormat: nil
        )
    )
    .padding()
}

#Preview("Book Card — PDF New") {
    BookCardView(
        item: LibraryItemSummary(
            id: "preview-2",
            title: "Machine Learning",
            type: .pdf,
            progress: 0,
            date: "2024-01-20",
            category: .imported,
            folderId: "f1",
            source: "upload:ml.pdf",
            filePath: nil,
            coverImage: nil,
            voice: nil,
            speed: nil,
            contentFormat: nil
        )
    )
    .padding()
}
