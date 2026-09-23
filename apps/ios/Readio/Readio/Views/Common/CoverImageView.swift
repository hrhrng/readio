import SwiftUI
import ImageIO

// MARK: - CoverImageView
/// Displays a book cover image with intelligent fallback handling.
///
/// **Image Source Priority:**
/// 1. **Data URL** (starts with `"data:image"`): Base64-encoded image embedded
///    directly in the JSON response. Common for EPUB covers extracted server-side.
///    Decoded synchronously since the data is already local.
///
/// 2. **Remote URL** (starts with `"http"` or `"https"`): Standard web image URL.
///    Loaded asynchronously via `AsyncImage` with a loading placeholder.
///
/// 3. **Gradient Fallback**: When no cover image is available (nil or unparseable),
///    displays a deterministic gradient background with the first character of the
///    book title. The gradient colors are derived from the title's hash to ensure
///    consistent colors for the same book across app launches.
///
/// **Usage:**
/// ```swift
/// CoverImageView(
///     coverImage: item.coverImage,
///     title: item.title,
///     size: CGSize(width: 160, height: 140)
/// )
/// ```
struct CoverImageView: View {

    @Environment(\.colorScheme) private var colorScheme

    // MARK: - Properties

    /// The cover image string from the API. Can be:
    /// - A data URL: `"data:image/jpeg;base64,/9j/4AAQ..."``
    /// - A remote URL: `"https://example.com/cover.jpg"``
    /// - `nil` when no cover is available
    let coverImage: String?

    /// The book title, used for the gradient fallback's initial letter display
    /// and for generating deterministic gradient colors.
    let title: String

    /// The desired render size of the cover image view.
    let size: CGSize

    /// Resolves local EPUB asset IDs from the original archive. Keeping this
    /// dependency injected lets previews and remote-only covers remain pure.
    var loadEpubAsset: ((String) async throws -> Data)? = nil

    // MARK: - Body

    var body: some View {
        Group {
            if let coverImage, !coverImage.isEmpty {
                if let assetID = Self.epubAssetID(from: coverImage),
                   let loadEpubAsset {
                    ZStack {
                        gradientFallback
                        LocalEpubCoverImage(
                            assetID: assetID,
                            load: loadEpubAsset
                        )
                    }
                } else if coverImage.hasPrefix("data:image") {
                    // MARK: Data URL — Base64 Decoded Image
                    dataURLImage(coverImage)
                } else if coverImage.hasPrefix("http") {
                    // MARK: Remote URL — AsyncImage
                    remoteImage(coverImage)
                } else {
                    // MARK: Unrecognized Format — Fallback
                    gradientFallback
                }
            } else {
                // MARK: No Cover — Gradient Fallback
                gradientFallback
            }
        }
        .frame(width: size.width, height: size.height)
    }

    static func epubAssetID(from reference: String) -> String? {
        let prefix = "readio-epub-asset:"
        guard reference.hasPrefix(prefix) else { return nil }
        let assetID = String(reference.dropFirst(prefix.count))
        return assetID.isEmpty ? nil : assetID
    }

    // MARK: - Data URL Image

    /// Extracts and renders a base64-encoded image from a data URL.
    ///
    /// Data URL format: `data:image/<format>;base64,<encoded-data>`
    /// We split on the first comma to get the base64 payload, then decode it
    /// into a platform-neutral `CGImage`. If decoding fails, we fall back to the gradient.
    @ViewBuilder
    private func dataURLImage(_ dataURL: String) -> some View {
        if let image = decodeDataURL(dataURL) {
            Image(decorative: image, scale: 1)
                .resizable()
                .aspectRatio(contentMode: .fill)
                .frame(width: size.width, height: size.height)
                .clipped()
        } else {
            // Base64 decoding failed — show gradient fallback
            gradientFallback
        }
    }

    /// Decodes a data URL string into a platform-neutral CGImage.
    ///
    /// **Steps:**
    /// 1. Find the first comma separator between the MIME header and base64 data.
    /// 2. Extract the substring after the comma.
    /// 3. Decode from base64 into raw `Data`.
    /// 4. Create a `CGImage` from the decoded data.
    ///
    /// Returns `nil` if any step fails (malformed URL, invalid base64, corrupt image data).
    private func decodeDataURL(_ dataURL: String) -> CGImage? {
        // Find the comma that separates "data:image/jpeg;base64," from the actual data
        guard let commaIndex = dataURL.firstIndex(of: ",") else { return nil }
        let base64String = String(dataURL[dataURL.index(after: commaIndex)...])
        guard let data = Data(base64Encoded: base64String, options: .ignoreUnknownCharacters) else { return nil }
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }

    // MARK: - Remote URL Image

    /// Loads and displays a remote image using SwiftUI's `AsyncImage`.
    ///
    /// Shows a shimmer placeholder during loading and the gradient fallback
    /// if the download fails.
    @ViewBuilder
    private func remoteImage(_ urlString: String) -> some View {
        if let url = URL(string: urlString) {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image
                        .resizable()
                        .aspectRatio(contentMode: .fill)
                        .frame(width: size.width, height: size.height)
                        .clipped()

                case .failure:
                    // Network error — show gradient fallback
                    gradientFallback

                case .empty:
                    // Loading — show subtle placeholder
                    loadingPlaceholder

                @unknown default:
                    gradientFallback
                }
            }
        } else {
            // Invalid URL string — show gradient fallback
            gradientFallback
        }
    }

    // MARK: - Gradient Fallback

    /// Editorial solid-color fallback. Imported cover art is allowed to carry its
    /// own palette; generated fallbacks stay muted and avoid rainbow format colors.
    private var gradientFallback: some View {
        ZStack(alignment: .topLeading) {
            fallbackColor

            if size.width < 100 {
                Text(title.first.map(String.init)?.uppercased() ?? "?")
                    .font(.system(size: size.width * 0.48, weight: .medium, design: .serif))
                    .foregroundStyle(fallbackInk)
                    .frame(width: size.width, height: size.height)
            } else {
                Text(title)
                    .font(
                        .system(
                            size: max(16, min(size.width * 0.15, 28)),
                            weight: .semibold,
                            design: .serif
                        )
                    )
                    .foregroundStyle(fallbackInk)
                    .multilineTextAlignment(.leading)
                    .lineLimit(4)
                    .padding(max(14, size.width * 0.09))
            }
        }
        .frame(width: size.width, height: size.height)
    }

    // MARK: - Loading Placeholder

    /// Subtle shimmer-like placeholder shown while a remote image loads.
    private var loadingPlaceholder: some View {
        Rectangle()
            .fill(.quaternary)
            .overlay {
                ProgressView()
                    .tint(.secondary)
            }
    }

    // MARK: - Computed Properties

    private var fallbackIndex: Int {
        title.unicodeScalars.reduce(0) { ($0 + Int($1.value)) % 4 }
    }

    private var fallbackColor: Color {
        let light: [Color] = [
            Color(red: 0.47, green: 0.33, blue: 0.30),
            Color(red: 0.29, green: 0.42, blue: 0.49),
            Color(red: 0.72, green: 0.58, blue: 0.31),
            Color(red: 0.34, green: 0.43, blue: 0.34),
        ]
        let dark: [Color] = [
            Color(red: 0.36, green: 0.25, blue: 0.23),
            Color(red: 0.21, green: 0.32, blue: 0.37),
            Color(red: 0.48, green: 0.38, blue: 0.20),
            Color(red: 0.23, green: 0.31, blue: 0.24),
        ]
        return (colorScheme == .dark ? dark : light)[fallbackIndex]
    }

    private var fallbackInk: Color {
        fallbackIndex == 2 ? Color(red: 0.13, green: 0.13, blue: 0.11) : .white.opacity(0.94)
    }
}

private struct LocalEpubCoverImage: View {
    let assetID: String
    let load: (String) async throws -> Data

    @State private var image: CGImage?

    var body: some View {
        Group {
            if let image {
                Image(decorative: image, scale: 1)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .clipped()
            } else {
                Color.clear
            }
        }
        .task(id: assetID) {
            image = nil
            guard let data = try? await load(assetID),
                  !Task.isCancelled,
                  let source = CGImageSourceCreateWithData(data as CFData, nil),
                  let decoded = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
                return
            }
            image = decoded
        }
    }
}

// MARK: - Preview

#Preview("Data URL Cover") {
    CoverImageView(
        coverImage: nil,
        title: "The Great Gatsby",
        size: CGSize(width: 160, height: 140)
    )
    .clipShape(RoundedRectangle(cornerRadius: 10))
    .padding()
}

#Preview("Gradient Fallbacks") {
    HStack(spacing: 12) {
        CoverImageView(coverImage: nil, title: "Alice in Wonderland", size: CGSize(width: 100, height: 130))
            .clipShape(RoundedRectangle(cornerRadius: 8))
        CoverImageView(coverImage: nil, title: "Machine Learning", size: CGSize(width: 100, height: 130))
            .clipShape(RoundedRectangle(cornerRadius: 8))
        CoverImageView(coverImage: nil, title: "Swift Programming", size: CGSize(width: 100, height: 130))
            .clipShape(RoundedRectangle(cornerRadius: 8))
    }
    .padding()
}
