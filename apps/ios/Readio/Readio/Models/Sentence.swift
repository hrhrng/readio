import Foundation

// MARK: - Sentence
/// A single sentence extracted from document content for TTS playback.
///
/// The TTS player works at sentence granularity: each sentence is synthesized
/// individually (or in small batches), and the UI highlights the active sentence
/// during playback. The `index` serves as both the sentence's position in the
/// document and its unique identity for tracking playback progress.
///
/// **Relationship to ContentBlock:**
/// For plain text and web content, sentences are split client-side using regex.
/// For EPUB content, sentences come pre-split as `RichSentence` objects within
/// `ContentBlock` values — use `RichSentence` for rendering with inline styles,
/// and this simpler `Sentence` struct for TTS-only operations.
struct Sentence: Identifiable, Sendable {
    /// Zero-based position of this sentence within the chapter/document.
    /// Used as the playback cursor and for scroll-to-sentence navigation.
    let index: Int

    /// The raw text content of the sentence, stripped of any HTML/markup.
    /// This is the string sent to the TTS provider for synthesis.
    let text: String

    /// Conformance to `Identifiable` — the index is unique within a chapter.
    var id: Int { index }
}
