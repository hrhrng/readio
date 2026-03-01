import SwiftUI

// MARK: - ContentBlockView
/// Renders a single EPUB content block based on its type.
///
/// **Supported Block Types:**
///
/// | Type        | Rendering                                         |
/// |-------------|---------------------------------------------------|
/// | heading     | Large bold text (size varies by level 1-6)        |
/// | paragraph   | Body text with sentence-level TTS highlighting    |
/// | image       | AsyncImage with alt text caption                  |
/// | list        | Numbered or bulleted list items                   |
/// | blockquote  | Indented italic text with leading border           |
/// | code        | Monospace text in a pre-formatted block            |
/// | separator   | A horizontal divider line                          |
///
/// **Sentence Highlighting:**
/// For text-containing blocks (heading, paragraph, list, blockquote), each sentence
/// is rendered using `SentenceTextView` which provides:
/// - Background highlight for the active sentence
/// - Word-level progress highlighting within the active sentence
/// - Tap-to-seek: tapping any sentence starts TTS from that point
///
/// **Design Philosophy:**
/// This mirrors the web frontend's `BlockRenderer` component from `content-blocks.tsx`,
/// adapted for SwiftUI's declarative layout system. The block types and their properties
/// match the server's EPUB parser output format.
struct ContentBlockView: View {

    // MARK: - Properties

    /// The content block to render. Determines which visual treatment to use.
    let block: ContentBlock

    /// The global sentence index currently being spoken by TTS.
    let currentSentenceIndex: Int

    /// Word-level progress within the active sentence (0.0 to 1.0).
    let currentWordProgress: Double

    /// Callback when the user taps a sentence to start playback there.
    let onSentenceClick: (Int) -> Void

    // MARK: - Body

    var body: some View {
        switch block {
        case .heading(let level, let sentences, _):
            // MARK: Heading Block
            // Large text with size proportional to the heading level.
            // Level 1 is the largest (title), level 6 is the smallest.
            headingView(level: level, sentences: sentences)

        case .paragraph(let sentences, _):
            // MARK: Paragraph Block
            // Standard body text with sentence-level highlighting.
            paragraphView(sentences: sentences)

        case .image(let src, let alt):
            // MARK: Image Block
            // Async-loaded image with an optional alt text caption below.
            imageView(src: src, alt: alt)

        case .list(let ordered, let items, _):
            // MARK: List Block
            // Numbered (ordered) or bulleted (unordered) list.
            listView(ordered: ordered, items: items)

        case .blockquote(let sentences, _):
            // MARK: Blockquote Block
            // Indented text with a leading accent border and italic style.
            blockquoteView(sentences: sentences)

        case .code(let text, let language):
            // MARK: Code Block
            // Monospace pre-formatted text in a rounded container.
            codeView(text: text, language: language)

        case .separator:
            // MARK: Separator Block
            // A simple horizontal divider.
            Divider()
                .padding(.vertical, 12)
        }
    }

    // MARK: - Heading View

    /// Renders a heading with size based on the heading level (1-6).
    ///
    /// - Level 1: `.title` size, extra bold
    /// - Level 2: `.title2` size, bold
    /// - Level 3: `.title3` size, semibold
    /// - Level 4-6: `.headline` size, medium weight
    private func headingView(level: Int, sentences: [RichSentence]) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(sentences, id: \.index) { sentence in
                SentenceTextView(
                    sentence: Sentence(index: sentence.index, text: sentence.text),
                    isActive: sentence.index == currentSentenceIndex,
                    wordProgress: currentWordProgress,
                    onTap: { onSentenceClick(sentence.index) }
                )
            }
        }
        .font(headingFont(for: level))
        .padding(.vertical, level <= 2 ? 8 : 4)
    }

    /// Returns the appropriate font for a heading level.
    private func headingFont(for level: Int) -> Font {
        switch level {
        case 1: return .title.bold()
        case 2: return .title2.bold()
        case 3: return .title3.weight(.semibold)
        default: return .headline
        }
    }

    // MARK: - Paragraph View

    /// Renders a paragraph as a sequence of sentence views with body text styling.
    private func paragraphView(sentences: [RichSentence]) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(sentences, id: \.index) { sentence in
                SentenceTextView(
                    sentence: Sentence(index: sentence.index, text: sentence.text),
                    isActive: sentence.index == currentSentenceIndex,
                    wordProgress: currentWordProgress,
                    onTap: { onSentenceClick(sentence.index) }
                )
            }
        }
        .padding(.bottom, 8)
    }

    // MARK: - Image View

    /// Renders an image block with async loading and an optional alt text caption.
    ///
    /// The image scales to fill the available width while maintaining its aspect ratio.
    /// A rounded clip shape and subtle shadow give it a polished, card-like appearance.
    private func imageView(src: String, alt: String?) -> some View {
        VStack(alignment: .center, spacing: 6) {
            if let url = URL(string: src) {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image):
                        image
                            .resizable()
                            .aspectRatio(contentMode: .fit)
                            .clipShape(RoundedRectangle(cornerRadius: 8))

                    case .failure:
                        // Image load failed — show placeholder with icon
                        RoundedRectangle(cornerRadius: 8)
                            .fill(Color(.systemGray5))
                            .frame(height: 200)
                            .overlay {
                                Image(systemName: "photo")
                                    .font(.largeTitle)
                                    .foregroundStyle(.secondary)
                            }

                    case .empty:
                        // Loading state
                        RoundedRectangle(cornerRadius: 8)
                            .fill(Color(.systemGray6))
                            .frame(height: 200)
                            .overlay { ProgressView() }

                    @unknown default:
                        EmptyView()
                    }
                }
            }

            // Alt text caption
            if let alt, !alt.isEmpty {
                Text(alt)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .italic()
                    .multilineTextAlignment(.center)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 8)
    }

    // MARK: - List View

    /// Renders an ordered or unordered list with sentence highlighting per item.
    ///
    /// - Ordered lists use numeric indices (1., 2., 3., ...).
    /// - Unordered lists use bullet characters.
    /// Each list item contains its own sentences for TTS highlighting.
    private func listView(ordered: Bool, items: [ListItem]) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                HStack(alignment: .top, spacing: 8) {
                    // Bullet or number marker
                    Text(ordered ? "\(index + 1)." : "\u{2022}")
                        .font(.body)
                        .foregroundStyle(.secondary)
                        .frame(width: 24, alignment: .trailing)

                    // List item sentences
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(item.sentences, id: \.index) { sentence in
                            SentenceTextView(
                                sentence: Sentence(index: sentence.index, text: sentence.text),
                                isActive: sentence.index == currentSentenceIndex,
                                wordProgress: currentWordProgress,
                                onTap: { onSentenceClick(sentence.index) }
                            )
                        }
                    }
                }
            }
        }
        .padding(.leading, 8)
        .padding(.bottom, 8)
    }

    // MARK: - Blockquote View

    /// Renders a blockquote with a leading accent-color border and italic text.
    ///
    /// Mirrors the web frontend's blockquote styling: `border-l-4 border-accent pl-4 italic`.
    private func blockquoteView(sentences: [RichSentence]) -> some View {
        HStack(alignment: .top, spacing: 0) {
            // Leading accent border
            Rectangle()
                .fill(Color.accentColor.opacity(0.5))
                .frame(width: 4)
                .clipShape(RoundedRectangle(cornerRadius: 2))

            // Blockquote text content
            VStack(alignment: .leading, spacing: 2) {
                ForEach(sentences, id: \.index) { sentence in
                    SentenceTextView(
                        sentence: Sentence(index: sentence.index, text: sentence.text),
                        isActive: sentence.index == currentSentenceIndex,
                        wordProgress: currentWordProgress,
                        onTap: { onSentenceClick(sentence.index) }
                    )
                }
            }
            .italic()
            .padding(.leading, 12)
        }
        .padding(.vertical, 4)
        .padding(.bottom, 8)
    }

    // MARK: - Code View

    /// Renders a code block with monospace font in a rounded container.
    ///
    /// Code blocks are not sentence-splittable (they contain raw text, not sentences),
    /// so they don't participate in TTS highlighting. The optional `language` parameter
    /// is displayed as a label but syntax highlighting is not implemented.
    private func codeView(text: String, language: String?) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            // Optional language label
            if let language, !language.isEmpty {
                Text(language)
                    .font(.caption2)
                    .fontWeight(.medium)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 2)
                    .background(
                        Capsule()
                            .fill(Color(.systemGray5))
                    )
            }

            // Code content
            ScrollView(.horizontal, showsIndicators: false) {
                Text(text)
                    .font(.system(.callout, design: .monospaced))
                    .foregroundStyle(.primary)
                    .textSelection(.enabled) // Allow copying code text
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(Color(.secondarySystemGroupedBackground))
            )
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Note on Types
// The types used by this view (ContentBlock, EpubChapter, RichSentence, StyledRun,
// ListItem) are defined in the Models layer:
//   - Models/ContentBlock.swift: ContentBlock, StyledRun, RichSentence, ListItem
//   - Models/EpubChapter.swift: EpubChapter, TOCEntry

// MARK: - Preview

#Preview {
    ScrollView {
        VStack(alignment: .leading, spacing: 0) {
            ContentBlockView(
                block: .heading(level: 1, sentences: [
                    RichSentence(index: 0, text: "Chapter One", runs: [StyledRun(text: "Chapter One")])
                ]),
                currentSentenceIndex: -1,
                currentWordProgress: 0,
                onSentenceClick: { _ in }
            )

            ContentBlockView(
                block: .paragraph(sentences: [
                    RichSentence(index: 1, text: "It was the best of times.", runs: [StyledRun(text: "It was the best of times.")]),
                    RichSentence(index: 2, text: "It was the worst of times.", runs: [StyledRun(text: "It was the worst of times.")]),
                ]),
                currentSentenceIndex: 1,
                currentWordProgress: 0.5,
                onSentenceClick: { _ in }
            )

            ContentBlockView(
                block: .separator,
                currentSentenceIndex: -1,
                currentWordProgress: 0,
                onSentenceClick: { _ in }
            )

            ContentBlockView(
                block: .code(text: "let x = 42\nprint(x)", language: "swift"),
                currentSentenceIndex: -1,
                currentWordProgress: 0,
                onSentenceClick: { _ in }
            )
        }
        .padding()
    }
}
