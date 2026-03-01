import SwiftUI

// MARK: - SentenceTextView
/// Renders a single sentence with optional word-level TTS highlighting.
///
/// **States:**
///
/// 1. **Inactive** — Normal body text, subtle hover/tap feedback.
/// ```
///   The quick brown fox jumps over the lazy dog.
/// ```
///
/// 2. **Active (with word progress)** — Background highlighted, individual words
///    colored up to the current TTS playback position.
/// ```
///   [The quick brown] fox jumps over the lazy dog.
///    ^^^^^^^^^^^^^^^^
///    highlighted words (spoken)
/// ```
///
/// **Word-Level Highlighting Algorithm:**
/// 1. Split the sentence text into tokens using whitespace.
/// 2. Calculate the highlighted word index: `floor(wordProgress * tokenCount)`.
/// 3. Build an `AttributedString` (or `Text` concatenation) where tokens up to
///    the highlighted index use the accent color, and remaining tokens use default color.
///
/// **Performance:**
/// This view is designed to be embedded in a `LazyVStack` with potentially thousands
/// of siblings. Only the active sentence performs word-level splitting; inactive sentences
/// render a simple `Text` view with no overhead.
///
/// **Interaction:**
/// Tapping the sentence calls `onTap`, which typically tells the TTS engine to
/// start playback from this sentence's index.
struct SentenceTextView: View {

    // MARK: - Properties

    /// The sentence data containing the index and text content.
    let sentence: Sentence

    /// Whether this sentence is currently being spoken by the TTS engine.
    let isActive: Bool

    /// Word-level progress within this sentence (0.0 to 1.0).
    /// Only meaningful when `isActive` is true.
    let wordProgress: Double

    /// Callback invoked when the user taps this sentence.
    let onTap: () -> Void

    // MARK: - Body

    var body: some View {
        Button(action: onTap) {
            if isActive {
                // MARK: Active Sentence — Word-Level Highlighting
                // Split into tokens and highlight up to the current word.
                activeTextView
            } else {
                // MARK: Inactive Sentence — Plain Text
                // Simple text rendering with no per-word overhead.
                Text(sentence.text)
                    .font(.body)
                    .foregroundStyle(.primary)
                    .multilineTextAlignment(.leading)
            }
        }
        .buttonStyle(.plain)
        .padding(.vertical, 4)
        .padding(.horizontal, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(isActive ? Color.accentColor.opacity(0.1) : Color.clear)
        )
        .animation(.easeInOut(duration: 0.15), value: isActive)
    }

    // MARK: - Active Text View

    /// Renders the sentence with per-word highlighting based on TTS playback progress.
    ///
    /// **Algorithm:**
    /// 1. Tokenize the sentence text by splitting on whitespace boundaries.
    /// 2. Compute how many tokens have been "spoken" based on `wordProgress`.
    /// 3. Concatenate `Text` views: spoken tokens use accent color with bold weight,
    ///    unspoken tokens use the default primary color.
    ///
    /// This approach uses SwiftUI's `Text` concatenation (`+` operator) rather than
    /// `AttributedString` because it's more performant for simple color changes and
    /// doesn't require Foundation's attributed string infrastructure.
    private var activeTextView: some View {
        let tokens = tokenize(sentence.text)
        let totalTokens = tokens.filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }.count
        // Calculate the index of the currently highlighted word.
        // Clamp to valid range to prevent out-of-bounds issues.
        let highlightedIndex = totalTokens > 0
            ? min(Int(floor(wordProgress * Double(totalTokens))), totalTokens - 1)
            : -1

        // Build the text by concatenating individually styled token Text views.
        var wordIndex = 0
        let styledText = tokens.reduce(Text("")) { accumulated, token in
            if token.trimmingCharacters(in: .whitespaces).isEmpty {
                // Whitespace token — render as-is, no word index increment
                return accumulated + Text(token)
            } else {
                let currentWordIndex = wordIndex
                wordIndex += 1

                // Determine if this word should be highlighted
                let isHighlighted = currentWordIndex == highlightedIndex
                let isSpoken = currentWordIndex < highlightedIndex

                if isHighlighted {
                    // Currently spoken word — accent color with bold
                    return accumulated + Text(token)
                        .foregroundColor(.accentColor)
                        .fontWeight(.semibold)
                } else if isSpoken {
                    // Already spoken word — slightly dimmer accent
                    return accumulated + Text(token)
                        .foregroundColor(.accentColor.opacity(0.7))
                } else {
                    // Not yet spoken — default color
                    return accumulated + Text(token)
                        .foregroundColor(.primary)
                }
            }
        }

        return styledText
            .font(.body)
            .multilineTextAlignment(.leading)
    }

    // MARK: - Tokenization

    /// Splits text into tokens while preserving whitespace as separate tokens.
    ///
    /// This tokenizer handles both Latin-script languages (space-separated words)
    /// and CJK text (character-by-character). It preserves the original spacing
    /// so the reconstructed text looks identical to the input.
    ///
    /// **Examples:**
    /// - `"Hello world"` → `["Hello", " ", "world"]`
    /// - `"The  quick"` → `["The", "  ", "quick"]`
    ///
    /// **Note:** This mirrors the web frontend's `tokenize()` function from `lib/cjk.ts`.
    private func tokenize(_ text: String) -> [String] {
        var tokens: [String] = []
        var currentToken = ""
        var inWhitespace = false

        for char in text {
            let charIsWhitespace = char.isWhitespace

            if charIsWhitespace != inWhitespace && !currentToken.isEmpty {
                // Transition between word and whitespace — flush current token
                tokens.append(currentToken)
                currentToken = ""
            }

            currentToken.append(char)
            inWhitespace = charIsWhitespace
        }

        // Flush any remaining token
        if !currentToken.isEmpty {
            tokens.append(currentToken)
        }

        return tokens
    }
}

// MARK: - Preview

#Preview("Active Sentence") {
    VStack(spacing: 12) {
        SentenceTextView(
            sentence: Sentence(index: 0, text: "The quick brown fox jumps over the lazy dog."),
            isActive: true,
            wordProgress: 0.4,
            onTap: {}
        )

        SentenceTextView(
            sentence: Sentence(index: 1, text: "This sentence is not currently active."),
            isActive: false,
            wordProgress: 0,
            onTap: {}
        )

        SentenceTextView(
            sentence: Sentence(index: 2, text: "Another active sentence with different progress."),
            isActive: true,
            wordProgress: 0.8,
            onTap: {}
        )
    }
    .padding()
}
