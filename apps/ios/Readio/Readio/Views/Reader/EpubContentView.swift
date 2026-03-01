import SwiftUI

// MARK: - EpubContentView
/// Renders EPUB chapter content with rich content blocks and sentence-level TTS highlighting.
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │                                      │
/// │  Chapter Title (heading block)       │  ← ContentBlockView(.heading)
/// │                                      │
/// │  First paragraph of the chapter     │  ← ContentBlockView(.paragraph)
/// │  with sentence highlighting...      │     with SentenceTextView inside
/// │                                      │
/// │  • List item one                    │  ← ContentBlockView(.list)
/// │  • List item two                    │
/// │                                      │
/// │  > A blockquote with attribution    │  ← ContentBlockView(.blockquote)
/// │                                      │
/// │  ──────────────────────             │  ← ContentBlockView(.separator)
/// │                                      │
/// │  More paragraph text...              │
/// │                                      │
/// │  ◀ Chapter 2 of 15 ▶               │  ← Chapter navigation
/// └──────────────────────────────────────┘
/// ```
///
/// **Data Flow:**
/// - `chapters`: Array of `EpubChapter` objects, each containing an array of `ContentBlock`.
/// - `currentChapterId`: Determines which chapter is currently displayed.
/// - Sentence highlighting works across all block types (heading, paragraph, list, blockquote).
///
/// **Navigation:**
/// - Bottom chapter navigation bar with previous/next buttons.
/// - Chapter changes notify the parent via `onChapterChange` callback.
///
/// **Auto-Scroll:**
/// Uses `ScrollViewReader` to follow the active sentence during TTS playback,
/// scrolling to keep the spoken sentence centered in the viewport.
struct EpubContentView: View {

    // MARK: - Properties

    /// All chapters in the EPUB book, each with its content blocks.
    let chapters: [EpubChapter]

    /// The ID of the chapter currently being displayed.
    let currentChapterId: String?

    /// The global sentence index currently being spoken by TTS.
    let currentSentenceIndex: Int

    /// Word-level progress within the active sentence (0.0 to 1.0).
    let currentWordProgress: Double

    /// Callback when the user taps a sentence to start playback there.
    let onSentenceClick: (Int) -> Void

    /// Callback when the user navigates to a different chapter.
    let onChapterChange: (String) -> Void

    // MARK: - Computed Properties

    /// The currently displayed chapter, found by matching `currentChapterId`.
    /// Falls back to the first chapter if no match is found.
    private var currentChapter: EpubChapter? {
        if let id = currentChapterId {
            return chapters.first { $0.id == id }
        }
        return chapters.first
    }

    /// The index of the current chapter in the `chapters` array.
    /// Used for previous/next navigation.
    private var currentChapterIndex: Int? {
        guard let chapter = currentChapter else { return nil }
        return chapters.firstIndex { $0.id == chapter.id }
    }

    /// Whether there is a previous chapter to navigate to.
    private var hasPreviousChapter: Bool {
        guard let index = currentChapterIndex else { return false }
        return index > 0
    }

    /// Whether there is a next chapter to navigate to.
    private var hasNextChapter: Bool {
        guard let index = currentChapterIndex else { return false }
        return index < chapters.count - 1
    }

    // MARK: - Body

    var body: some View {
        if let chapter = currentChapter {
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        // MARK: Chapter Content Blocks
                        // Each block is rendered by ContentBlockView which handles
                        // all block types (heading, paragraph, list, etc.).
                        ForEach(Array(chapter.blocks.enumerated()), id: \.offset) { index, block in
                            ContentBlockView(
                                block: block,
                                currentSentenceIndex: currentSentenceIndex,
                                currentWordProgress: currentWordProgress,
                                onSentenceClick: onSentenceClick
                            )
                            .id("block-\(index)") // For potential block-level scrolling
                        }

                        // MARK: Chapter Navigation Footer
                        chapterNavigationBar
                            .padding(.top, 24)
                    }
                    .padding(.horizontal)
                    .padding(.vertical, 16)
                }
                // MARK: Auto-Scroll on Sentence Change
                // Scroll to keep the active sentence visible during TTS playback.
                .onChange(of: currentSentenceIndex) { _, newIndex in
                    guard newIndex >= 0 else { return }
                    withAnimation(.easeInOut(duration: 0.3)) {
                        // Scroll to the sentence anchor within the content blocks.
                        // SentenceTextView uses `.id(sentence.index)` for this.
                        proxy.scrollTo(newIndex, anchor: .center)
                    }
                }
            }
            .background(Color(.systemBackground))
        } else {
            // No chapter available — show empty state
            EmptyStateView(
                icon: "book.closed",
                title: "No Content",
                message: "This chapter has no content to display."
            )
        }
    }

    // MARK: - Chapter Navigation Bar

    /// Bottom navigation bar for moving between chapters.
    ///
    /// Shows previous/next buttons and the current chapter position (e.g., "3 of 15").
    /// Buttons are disabled at the edges (first/last chapter).
    private var chapterNavigationBar: some View {
        HStack {
            // Previous chapter button
            Button {
                if let index = currentChapterIndex, index > 0 {
                    onChapterChange(chapters[index - 1].id)
                }
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: "chevron.left")
                    Text("Previous")
                }
                .font(.subheadline)
                .fontWeight(.medium)
            }
            .disabled(!hasPreviousChapter)
            .opacity(hasPreviousChapter ? 1.0 : 0.3)

            Spacer()

            // Chapter position indicator
            if let index = currentChapterIndex {
                Text("Chapter \(index + 1) of \(chapters.count)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            // Next chapter button
            Button {
                if let index = currentChapterIndex, index < chapters.count - 1 {
                    onChapterChange(chapters[index + 1].id)
                }
            } label: {
                HStack(spacing: 4) {
                    Text("Next")
                    Image(systemName: "chevron.right")
                }
                .font(.subheadline)
                .fontWeight(.medium)
            }
            .disabled(!hasNextChapter)
            .opacity(hasNextChapter ? 1.0 : 0.3)
        }
        .padding(.vertical, 12)
        .padding(.horizontal, 4)
    }
}

// MARK: - Preview

#Preview {
    EpubContentView(
        chapters: [
            EpubChapter(
                id: "ch-1",
                title: "Chapter 1: The Beginning",
                blocks: [
                    .heading(level: 1, sentences: [
                        RichSentence(index: 0, text: "Chapter 1: The Beginning", runs: [
                            StyledRun(text: "Chapter 1: The Beginning")
                        ])
                    ]),
                    .paragraph(sentences: [
                        RichSentence(index: 1, text: "It was a dark and stormy night.", runs: [
                            StyledRun(text: "It was a dark and stormy night.")
                        ]),
                        RichSentence(index: 2, text: "The wind howled through the trees.", runs: [
                            StyledRun(text: "The wind howled through the trees.")
                        ]),
                    ]),
                    .separator,
                    .paragraph(sentences: [
                        RichSentence(index: 3, text: "Morning came at last.", runs: [
                            StyledRun(text: "Morning came at last.")
                        ]),
                    ]),
                ]
            )
        ],
        currentChapterId: "ch-1",
        currentSentenceIndex: 1,
        currentWordProgress: 0.5,
        onSentenceClick: { _ in },
        onChapterChange: { _ in }
    )
}
