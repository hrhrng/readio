import SwiftUI

// MARK: - PlainTextContentView
/// Renders plain text content with sentence-level TTS highlighting and auto-scroll.
///
/// **Usage:**
/// Used for TXT, Web, and PDF content types where the document is represented
/// as a flat array of `Sentence` objects (no chapter/block structure).
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │                                      │
/// │  Sentence 1 text goes here and      │  ← Inactive (normal style)
/// │  continues onto the next line.      │
/// │                                      │
/// │  [Sentence 2 is currently active    │  ← Active (highlighted BG)
/// │   and being spoken by TTS.]         │     Word-level highlight
/// │                                      │
/// │  Sentence 3 text follows here.      │  ← Inactive
/// │                                      │
/// └──────────────────────────────────────┘
/// ```
///
/// **Behavior:**
/// - **Auto-scroll:** During TTS playback, the view automatically scrolls to keep
///   the currently spoken sentence visible. Uses `ScrollViewReader` with smooth
///   animation to track the active sentence.
/// - **Tap-to-seek:** Tapping any sentence starts TTS playback from that sentence.
/// - **Word highlight:** The active sentence shows per-word progress highlighting,
///   matching the web frontend's behavior.
///
/// **Performance:**
/// Uses `LazyVStack` to only render visible sentences. This is important for
/// large documents (novels, research papers) that may contain thousands of sentences.
struct PlainTextContentView: View {

    // MARK: - Properties

    /// The array of sentences extracted from the document content.
    /// Each sentence has an `index` (position) and `text` (raw content).
    let sentences: [Sentence]

    /// The index of the currently active sentence (being spoken by TTS).
    /// -1 or out-of-range means no sentence is active.
    let currentSentenceIndex: Int

    /// The word-level progress within the active sentence (0.0 to 1.0).
    /// Used by `SentenceTextView` to highlight individual words as they are spoken.
    let currentWordProgress: Double

    /// Callback invoked when the user taps a sentence to start playback there.
    let onSentenceClick: (Int) -> Void

    // MARK: - Body

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 4) {
                    ForEach(sentences) { sentence in
                        SentenceTextView(
                            sentence: sentence,
                            isActive: sentence.index == currentSentenceIndex,
                            wordProgress: currentWordProgress,
                            onTap: {
                                onSentenceClick(sentence.index)
                            }
                        )
                        .id(sentence.index) // Used by ScrollViewReader for programmatic scrolling
                    }
                }
                .padding(.horizontal)
                .padding(.vertical, 12)
            }
            // MARK: Auto-Scroll
            // When the active sentence changes, smoothly scroll to keep it visible.
            // The `.center` anchor positions the active sentence in the middle of
            // the viewport for optimal reading context (text above and below).
            .onChange(of: currentSentenceIndex) { _, newIndex in
                guard newIndex >= 0 else { return }
                withAnimation(.easeInOut(duration: 0.3)) {
                    proxy.scrollTo(newIndex, anchor: .center)
                }
            }
        }
        .background(Color(.systemBackground))
    }
}

// MARK: - Preview

#Preview {
    PlainTextContentView(
        sentences: [
            Sentence(index: 0, text: "In the beginning, there was nothing but darkness and silence."),
            Sentence(index: 1, text: "Then, slowly, a faint light appeared on the horizon."),
            Sentence(index: 2, text: "It grew brighter and brighter until it filled the entire sky."),
            Sentence(index: 3, text: "The world was born in that moment of pure, radiant light."),
            Sentence(index: 4, text: "And with it came the first sounds — wind through the trees, water over stones."),
        ],
        currentSentenceIndex: 2,
        currentWordProgress: 0.4,
        onSentenceClick: { index in print("Tapped sentence \(index)") }
    )
}
