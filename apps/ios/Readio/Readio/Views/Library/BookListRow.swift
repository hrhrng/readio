import SwiftUI

// MARK: - BookListRow
/// A compact list-row view for displaying a library item in the list layout mode.
///
/// **Layout:**
/// ```
/// ┌─────────────────────────────────────────────────┐
/// │  ┌──────┐                                       │
/// │  │Cover │  Title of the Book         [EPUB]     │
/// │  │ 60x80│  ━━━━━━━━░░░░░░  45%                  │
/// │  └──────┘                                       │
/// └─────────────────────────────────────────────────┘
/// ```
///
/// **Usage:**
/// Shown in `LibraryView` when the user toggles to list mode.
/// More compact than `BookCardView`, fitting more items on screen
/// and providing a scannable list experience.
///
/// **Design Choices:**
/// - Fixed cover thumbnail size (60x80) for consistent row heights.
/// - Title limited to 2 lines with tail truncation.
/// - Type badge and progress share the second line.
/// - System dividers between rows (handled by the parent).
struct BookListRow: View {

    // MARK: - Properties

    /// The library item summary to display in this row.
    let item: LibraryItemSummary

    /// Fixed dimensions for the cover thumbnail in list mode.
    private let thumbnailWidth: CGFloat = 60
    private let thumbnailHeight: CGFloat = 80

    // MARK: - Body

    var body: some View {
        HStack(spacing: 12) {
            // MARK: Cover Thumbnail
            // Small cover image on the leading edge for visual identification.
            CoverImageView(
                coverImage: item.coverImage,
                title: item.title,
                size: CGSize(width: thumbnailWidth, height: thumbnailHeight)
            )
            .clipShape(RoundedRectangle(cornerRadius: 6))

            // MARK: Text Content
            // Title, type badge, and progress stacked vertically.
            VStack(alignment: .leading, spacing: 6) {
                // Title — two lines max, truncated with ellipsis
                Text(item.title)
                    .font(.subheadline)
                    .fontWeight(.medium)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                    .foregroundStyle(.primary)

                // Metadata row: type badge + date
                HStack(spacing: 8) {
                    // Type badge — small colored pill matching BookCardView's badge
                    Text(item.type.rawValue.uppercased())
                        .font(.system(size: 10, weight: .bold, design: .rounded))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(
                            Capsule()
                                .fill(badgeColor)
                        )

                    // Relative date — formatted from the ISO string
                    Text(formattedDate)
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    Spacer()
                }

                // MARK: Progress Bar
                // Only shown when reading has started (progress > 0).
                if item.progress > 0 {
                    HStack(spacing: 6) {
                        // Progress track
                        GeometryReader { geometry in
                            ZStack(alignment: .leading) {
                                RoundedRectangle(cornerRadius: 2)
                                    .fill(Color(.systemGray5))
                                    .frame(height: 3)

                                RoundedRectangle(cornerRadius: 2)
                                    .fill(Color.accentColor)
                                    .frame(
                                        width: geometry.size.width * CGFloat(item.progress) / 100.0,
                                        height: 3
                                    )
                            }
                        }
                        .frame(height: 3)

                        // Progress percentage
                        Text("\(item.progress)%")
                            .font(.system(size: 10, weight: .medium, design: .rounded))
                            .foregroundStyle(.secondary)
                            .frame(width: 32, alignment: .trailing)
                    }
                }
            }

            // MARK: Chevron
            // Right-facing chevron indicating the row is tappable/navigable.
            Image(systemName: "chevron.right")
                .font(.caption)
                .foregroundStyle(.quaternary)
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
        .contentShape(Rectangle()) // Makes the entire row tappable, not just the text
    }

    // MARK: - Computed Properties

    /// Returns a color corresponding to the item's document type.
    /// Matches the color scheme used in `BookCardView.typeBadge`.
    private var badgeColor: Color {
        switch item.type {
        case .epub: return .indigo
        case .pdf: return .red
        case .txt: return .gray
        case .web: return .blue
        }
    }

    /// Formats the ISO 8601 date string into a concise relative or short date.
    ///
    /// Examples:
    /// - "Today", "Yesterday", "2 days ago" for recent items
    /// - "Jan 15" for items from the current year
    /// - "Jan 15, 2023" for items from previous years
    private var formattedDate: String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        // Try with fractional seconds first, then without
        guard let date = formatter.date(from: item.date) ?? {
            formatter.formatOptions = [.withInternetDateTime]
            return formatter.date(from: item.date)
        }() else {
            return item.date
        }

        let calendar = Calendar.current
        if calendar.isDateInToday(date) {
            return "Today"
        } else if calendar.isDateInYesterday(date) {
            return "Yesterday"
        } else {
            let displayFormatter = DateFormatter()
            if calendar.isDate(date, equalTo: Date(), toGranularity: .year) {
                displayFormatter.dateFormat = "MMM d"
            } else {
                displayFormatter.dateFormat = "MMM d, yyyy"
            }
            return displayFormatter.string(from: date)
        }
    }
}

// MARK: - Preview

#Preview {
    List {
        BookListRow(
            item: LibraryItemSummary(
                id: "1", title: "The Great Gatsby", type: .epub, progress: 45,
                date: "2024-01-15T12:00:00Z", category: .imported, folderId: "f1",
                source: "upload:gatsby.epub", filePath: nil, coverImage: nil,
                voice: nil, speed: nil, contentFormat: nil
            )
        )

        BookListRow(
            item: LibraryItemSummary(
                id: "2", title: "Machine Learning: A Very Long Title That Should Truncate Nicely",
                type: .pdf, progress: 0,
                date: "2024-01-20T08:30:00Z", category: .imported, folderId: "f1",
                source: "upload:ml.pdf", filePath: nil, coverImage: nil,
                voice: nil, speed: nil, contentFormat: nil
            )
        )
    }
    .listStyle(.plain)
}
