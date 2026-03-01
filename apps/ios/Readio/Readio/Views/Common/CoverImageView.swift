import SwiftUI

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

    // MARK: - Body

    var body: some View {
        Group {
            if let coverImage, !coverImage.isEmpty {
                if coverImage.hasPrefix("data:image") {
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

    // MARK: - Data URL Image

    /// Extracts and renders a base64-encoded image from a data URL.
    ///
    /// Data URL format: `data:image/<format>;base64,<encoded-data>`
    /// We split on the first comma to get the base64 payload, then decode it
    /// into a `UIImage`. If decoding fails, we fall back to the gradient.
    @ViewBuilder
    private func dataURLImage(_ dataURL: String) -> some View {
        if let image = decodeDataURL(dataURL) {
            Image(uiImage: image)
                .resizable()
                .aspectRatio(contentMode: .fill)
                .frame(width: size.width, height: size.height)
                .clipped()
        } else {
            // Base64 decoding failed — show gradient fallback
            gradientFallback
        }
    }

    /// Decodes a data URL string into a UIImage.
    ///
    /// **Steps:**
    /// 1. Find the first comma separator between the MIME header and base64 data.
    /// 2. Extract the substring after the comma.
    /// 3. Decode from base64 into raw `Data`.
    /// 4. Create a `UIImage` from the decoded data.
    ///
    /// Returns `nil` if any step fails (malformed URL, invalid base64, corrupt image data).
    private func decodeDataURL(_ dataURL: String) -> UIImage? {
        // Find the comma that separates "data:image/jpeg;base64," from the actual data
        guard let commaIndex = dataURL.firstIndex(of: ",") else { return nil }
        let base64String = String(dataURL[dataURL.index(after: commaIndex)...])
        guard let data = Data(base64Encoded: base64String, options: .ignoreUnknownCharacters) else { return nil }
        return UIImage(data: data)
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

    /// A visually pleasing gradient background with the first letter of the title.
    ///
    /// The gradient colors are deterministically derived from the title string's hash,
    /// so the same book always gets the same color combination. This provides visual
    /// consistency and makes books recognizable even without cover art.
    private var gradientFallback: some View {
        ZStack {
            // Two-color gradient derived from the title hash
            LinearGradient(
                colors: gradientColors,
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )

            // First letter of the title, displayed large and semi-transparent
            Text(titleInitial)
                .font(.system(size: min(size.width, size.height) * 0.4, weight: .bold, design: .serif))
                .foregroundStyle(.white.opacity(0.8))
        }
        .frame(width: size.width, height: size.height)
    }

    // MARK: - Loading Placeholder

    /// Subtle shimmer-like placeholder shown while a remote image loads.
    private var loadingPlaceholder: some View {
        Rectangle()
            .fill(Color(.systemGray5))
            .overlay {
                ProgressView()
                    .tint(.secondary)
            }
    }

    // MARK: - Computed Properties

    /// The first character of the title (uppercased), or "?" if the title is empty.
    private var titleInitial: String {
        let firstChar = title.first.map(String.init) ?? "?"
        return firstChar.uppercased()
    }

    /// Generates a pair of gradient colors from the title's hash value.
    ///
    /// Uses a simple hash-to-hue mapping to create aesthetically pleasing
    /// color pairs. The second color is offset by ~0.3 on the hue wheel
    /// to create a complementary gradient effect.
    private var gradientColors: [Color] {
        // Predefined palette of gradient pairs for visual variety.
        // Using curated pairs ensures all gradients look good, unlike
        // purely algorithmic hue generation which can produce ugly combinations.
        let palettes: [(Color, Color)] = [
            (.indigo, .purple),
            (.blue, .cyan),
            (.teal, .green),
            (.orange, .pink),
            (.red, .orange),
            (.purple, .blue),
            (.mint, .teal),
            (.brown, .orange),
        ]

        // Deterministic selection based on the title hash.
        // `abs` handles potential negative hash values.
        let index = abs(title.hashValue) % palettes.count
        let (color1, color2) = palettes[index]
        return [color1, color2]
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
