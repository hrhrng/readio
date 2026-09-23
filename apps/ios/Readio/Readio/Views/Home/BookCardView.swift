import SwiftUI

/// A book is presented as an object, not as a card container: full 2:3 cover,
/// restrained metadata, and one thin reading-progress rail.
struct BookCardView: View {
    @Environment(\.readioTheme) private var theme
    @Environment(\.locale) private var locale

    let item: LibraryItemSummary
    var cardWidth: CGFloat = ReadioAppleLayout.bookCoverWidth

    private var coverHeight: CGFloat { cardWidth * 1.5 }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            CoverImageView(
                coverImage: item.coverImage,
                title: item.title,
                size: CGSize(width: cardWidth, height: coverHeight),
                loadEpubAsset: { assetID in
                    try await LibraryService().loadEpubAsset(itemID: item.id, assetID: assetID)
                }
            )
            .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
            .shadow(color: .black.opacity(0.14), radius: 5, x: 0, y: 4)

            Text(item.title)
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(theme.ink)
                .lineLimit(1)
                .truncationMode(.tail)
                .padding(.top, 14)

            Text(sourceLabel)
                .font(.system(size: 13, weight: .regular))
                .foregroundStyle(theme.secondaryInk)
                .lineLimit(1)
                .padding(.top, 6)

            GeometryReader { geometry in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(theme.highlight.opacity(0.55))
                    Capsule()
                        .fill(theme.accent)
                        .frame(
                            width: geometry.size.width
                                * CGFloat(min(max(item.progress, 0), 100)) / 100
                        )
                }
            }
            .frame(height: 3)
            .padding(.top, 12)
            .accessibilityLabel(
                AppleLocalization.formatted(
                    "reader.percentRead.format",
                    defaultValue: "%1$lld percent read",
                    locale: locale,
                    arguments: [item.progress]
                )
            )
        }
        .frame(width: cardWidth, alignment: .leading)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    private var sourceLabel: String {
        if item.source.hasPrefix("upload:") {
            let filename = String(item.source.dropFirst("upload:".count))
            let stem = URL(fileURLWithPath: filename)
                .deletingPathExtension()
                .lastPathComponent
            return stem.isEmpty ? item.type.rawValue.uppercased() : stem
        }
        if let url = URL(string: item.source), let host = url.host {
            return host.replacingOccurrences(of: "www.", with: "")
        }
        if item.source.isEmpty || item.source == "Manual Input" {
            return item.type.rawValue.uppercased()
        }
        return item.source
    }
}

#Preview("Book shelf") {
    BookCardView(
        item: LibraryItemSummary(
            id: "preview-1",
            title: "Ways of Seeing",
            type: .epub,
            progress: 45,
            date: "2024-01-15",
            category: .imported,
            folderId: "f1",
            source: "upload:John Berger.epub",
            filePath: nil,
            coverImage: nil,
            voice: nil,
            speed: nil,
            contentFormat: nil
        )
    )
    .padding()
}
