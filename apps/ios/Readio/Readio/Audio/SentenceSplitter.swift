import Foundation

// MARK: - SentenceSplitter
/// Stateless utility for splitting text into `Sentence` values at natural language
/// boundaries, with full CJK (Chinese / Japanese / Korean) support.
///
/// This is a faithful port of the web app's `lib/sentences.ts` + `lib/cjk.ts`.
///
/// **Boundary rules (matching the TypeScript `BOUNDARY` regex):**
///
/// 1. **English / Latin punctuation:** split after `.`, `!`, or `?` when followed
///    by whitespace. The whitespace is consumed and not included in either sentence.
///
/// 2. **CJK punctuation:** split after `\u3002` (。), `\uFF01` (！), or `\uFF1F` (？)
///    when the *next* character is a CJK ideograph (zero-width split — no characters
///    are consumed). This avoids splitting before closing quotes like "你好。"她说.
///
/// Because Swift's `Regex` lookbehind support is limited and
/// `NSRegularExpression` cannot do variable-length lookbehinds reliably, the
/// implementation uses a straightforward character-scanning approach that is
/// both efficient and easy to reason about.
///
/// **CJK Unicode ranges covered:**
/// - `\u2E80`–`\u9FFF`: CJK Radicals, Kangxi, Ideographs (Unified + Ext A)
/// - `\uAC00`–`\uD7AF`: Hangul Syllables
/// - `\uF900`–`\uFAFF`: CJK Compatibility Ideographs
enum SentenceSplitter {

    // MARK: - Public API

    /// Split a block of text into an array of `Sentence` values at natural
    /// sentence boundaries.
    ///
    /// Empty or whitespace-only segments are discarded. The returned sentences
    /// have contiguous, zero-based indices.
    ///
    /// - Parameter text: The raw text to split (may contain mixed Latin + CJK).
    /// - Returns: An array of `Sentence` values with trimmed text and sequential indices.
    static func splitSentences(_ text: String) -> [Sentence] {
        guard !text.isEmpty else { return [] }

        var sentences: [Sentence] = []
        var currentStart = text.startIndex   // start of the current sentence fragment
        let chars = Array(text.unicodeScalars) // work with scalars for boundary checks
        var i = 0

        // Walk through the text scalar-by-scalar, looking for boundary points.
        // When a boundary is found we slice the accumulated range into a sentence.
        while i < chars.count {
            let scalar = chars[i]

            // ---------------------------------------------------------------
            // Branch 1: English punctuation (.!?) followed by whitespace
            // ---------------------------------------------------------------
            if isLatinTerminator(scalar) {
                // Peek at the next character — if it's whitespace we have a boundary
                let nextIdx = i + 1
                if nextIdx < chars.count && chars[nextIdx].properties.isWhitespace {
                    // End of sentence is up to and including the punctuation mark
                    let sentenceEnd = text.unicodeScalars.index(
                        text.unicodeScalars.startIndex,
                        offsetBy: i + 1
                    )
                    let fragment = String(text[currentStart..<sentenceEnd]).trimmingCharacters(in: .whitespacesAndNewlines)
                    if !fragment.isEmpty {
                        sentences.append(Sentence(index: sentences.count, text: fragment))
                    }
                    // Skip over the whitespace run following the punctuation
                    var skip = nextIdx
                    while skip < chars.count && chars[skip].properties.isWhitespace {
                        skip += 1
                    }
                    currentStart = text.unicodeScalars.index(
                        text.unicodeScalars.startIndex,
                        offsetBy: skip
                    )
                    i = skip
                    continue
                }
            }

            // ---------------------------------------------------------------
            // Branch 2: CJK punctuation (。！？) followed by a CJK character
            // ---------------------------------------------------------------
            if isCJKTerminator(scalar) {
                let nextIdx = i + 1
                if nextIdx < chars.count && isCJKScalar(chars[nextIdx]) {
                    // Zero-width split: the punctuation belongs to the current
                    // sentence; the next CJK character starts a new sentence.
                    let sentenceEnd = text.unicodeScalars.index(
                        text.unicodeScalars.startIndex,
                        offsetBy: i + 1
                    )
                    let fragment = String(text[currentStart..<sentenceEnd]).trimmingCharacters(in: .whitespacesAndNewlines)
                    if !fragment.isEmpty {
                        sentences.append(Sentence(index: sentences.count, text: fragment))
                    }
                    currentStart = sentenceEnd
                    i = nextIdx
                    continue
                }
            }

            i += 1
        }

        // Flush any remaining text after the last boundary
        if currentStart < text.endIndex {
            let fragment = String(text[currentStart...]).trimmingCharacters(in: .whitespacesAndNewlines)
            if !fragment.isEmpty {
                sentences.append(Sentence(index: sentences.count, text: fragment))
            }
        }

        return sentences
    }

    // MARK: - CJK Utilities

    /// Check whether a `Character` falls within one of the CJK Unicode ranges.
    ///
    /// Covers CJK Unified Ideographs, Extension A, Compatibility Ideographs,
    /// Hangul Syllables, and CJK Radicals Supplement / Kangxi Radicals.
    ///
    /// - Parameter char: The character to test.
    /// - Returns: `true` if the character is a CJK ideograph or syllable.
    static func isCJK(_ char: Character) -> Bool {
        guard let scalar = char.unicodeScalars.first else { return false }
        return isCJKScalar(scalar)
    }

    /// Count the number of "tokens" in a text string for word-level progress tracking.
    ///
    /// Tokenization strategy (mirrors `lib/cjk.ts → countTokens`):
    /// - Each CJK character counts as one token (Chinese text has no word separators).
    /// - Each contiguous run of non-CJK, non-whitespace characters counts as one token
    ///   (i.e., a Latin "word").
    /// - Whitespace runs are *not* counted.
    ///
    /// This is used to compute `currentWordProgress` during TTS playback: the
    /// fraction `wordsPlayed / totalTokens` drives per-word highlighting in the reader.
    ///
    /// - Parameter text: The text to tokenize and count.
    /// - Returns: The number of non-whitespace tokens.
    static func countTokens(_ text: String) -> Int {
        let tokens = tokenize(text)
        // Exclude whitespace-only tokens (same as the web app)
        return tokens.filter { !$0.allSatisfy(\.isWhitespace) }.count
    }

    /// Tokenize text into an array of strings suitable for word-level TTS highlighting.
    ///
    /// Produces three kinds of tokens (mirrors `lib/cjk.ts → tokenize`):
    /// 1. **CJK character** — each ideograph becomes its own single-character token.
    /// 2. **Latin word** — a contiguous run of non-CJK, non-whitespace characters.
    /// 3. **Whitespace** — a run of whitespace characters, preserved for layout.
    ///
    /// Examples:
    /// ```
    /// tokenize("Hello world")       → ["Hello", " ", "world"]
    /// tokenize("这是句子")            → ["这", "是", "句", "子"]
    /// tokenize("Hello 你好 world")   → ["Hello", " ", "你", "好", " ", "world"]
    /// ```
    ///
    /// - Parameter text: The text to tokenize.
    /// - Returns: An ordered array of token strings.
    static func tokenize(_ text: String) -> [String] {
        var tokens: [String] = []
        var currentRun = ""
        var currentRunType: RunType = .none // tracks what kind of characters we're accumulating

        for char in text {
            let type = runType(of: char)

            // If the character type changes, flush the accumulated run
            if type != currentRunType && !currentRun.isEmpty {
                // CJK characters are always emitted individually, so flush even
                // within a sequence of CJK chars (each is its own token).
                tokens.append(currentRun)
                currentRun = ""
            }

            currentRun.append(char)
            currentRunType = type

            // CJK characters are single-char tokens — flush immediately
            if type == .cjk {
                tokens.append(currentRun)
                currentRun = ""
                currentRunType = .none
            }
        }

        // Flush any remaining accumulated run
        if !currentRun.isEmpty {
            tokens.append(currentRun)
        }

        return tokens
    }

    // MARK: - Private Helpers

    /// Classifies a character into one of three run types for the tokenizer.
    private enum RunType: Equatable {
        case none        // initial / reset state
        case whitespace  // whitespace run
        case cjk         // single CJK character (always flushed immediately)
        case latin       // non-CJK, non-whitespace run (Latin "word")
    }

    /// Determine the run type for a given character.
    private static func runType(of char: Character) -> RunType {
        if char.isWhitespace { return .whitespace }
        if isCJK(char) { return .cjk }
        return .latin
    }

    /// Check whether a Unicode scalar is a CJK ideograph/syllable.
    /// Covers the same ranges as the TypeScript CJK_CHAR regex.
    private static func isCJKScalar(_ scalar: Unicode.Scalar) -> Bool {
        let value = scalar.value
        // CJK Radicals Supplement + Kangxi Radicals + Ideographic Description + CJK Symbols
        // + Hiragana + Katakana + CJK Unified Ideographs Extension A + CJK Unified Ideographs
        if value >= 0x2E80 && value <= 0x9FFF { return true }
        // Hangul Syllables
        if value >= 0xAC00 && value <= 0xD7AF { return true }
        // CJK Compatibility Ideographs
        if value >= 0xF900 && value <= 0xFAFF { return true }
        return false
    }

    /// Returns `true` for English sentence-ending punctuation: `.`, `!`, `?`
    private static func isLatinTerminator(_ scalar: Unicode.Scalar) -> Bool {
        scalar == "." || scalar == "!" || scalar == "?"
    }

    /// Returns `true` for CJK sentence-ending punctuation: 。(U+3002), ！(U+FF01), ？(U+FF1F)
    private static func isCJKTerminator(_ scalar: Unicode.Scalar) -> Bool {
        scalar.value == 0x3002 || scalar.value == 0xFF01 || scalar.value == 0xFF1F
    }
}
