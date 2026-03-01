import Foundation
import SwiftUI

// MARK: - ReaderViewModel
/// View model for the Reader screen that displays document content and manages
/// sentence extraction for TTS playback.
///
/// The reader is the core content consumption screen. It:
/// 1. Fetches the full `LibraryItem` (including `content`) from the backend.
/// 2. Extracts `Sentence` values from the content based on document type.
/// 3. Manages chapter navigation for EPUB documents.
/// 4. Persists reading progress back to the server.
///
/// ## Content Handling by Document Type
///
/// | Type   | Content Format           | Sentence Extraction                          |
/// |--------|--------------------------|----------------------------------------------|
/// | `txt`  | Plain text string        | `SentenceSplitter.splitSentences(content)`   |
/// | `web`  | Plain text or HTML       | Same as txt (HTML tags stripped by backend)   |
/// | `pdf`  | Plain text (extracted)   | Same as txt                                  |
/// | `epub` | JSON array of chapters   | Parse JSON → extract sentences from blocks   |
///
/// For EPUB, the `content` field is a JSON-encoded array of `EpubChapter` objects.
/// Each chapter contains `ContentBlock` values, each of which contains `RichSentence`
/// values. The reader extracts plain `Sentence` values from these for TTS, while
/// the view layer renders the `RichSentence`/`ContentBlock` types for styled display.
///
/// ## Chapter Navigation (EPUB)
///
/// EPUB items have a table of contents (`tocEntries`) parsed from the EPUB's
/// navigation document. When the user selects a chapter:
/// 1. The `currentChapterId` is updated locally and synced to the server.
/// 2. Sentences are re-extracted from the selected chapter's content blocks.
/// 3. The TTS engine (via `PlayerViewModel`) is notified to use the new sentences.
///
/// ## Progress Persistence
///
/// Reading progress is reported as a percentage (0–100) based on the current
/// sentence index relative to the total sentence count. The debounced
/// `saveProgress()` method PATCHes this to the server so the user's position
/// is preserved across sessions and devices.
@MainActor
@Observable
final class ReaderViewModel {

    // MARK: - Published State

    /// The full library item with content. `nil` until `loadItem()` completes.
    var item: LibraryItem? = nil

    /// Extracted sentences for the current chapter/document, ready for TTS.
    /// For EPUB, this changes when the user switches chapters.
    var sentences: [Sentence] = []

    /// Parsed EPUB chapters (only populated for `.epub` items).
    /// Each chapter contains structured content blocks with rich sentences.
    var chapters: [EpubChapter] = []

    /// Table of contents entries parsed from the EPUB navigation document.
    /// Used to render the TOC panel and for chapter-level navigation.
    var tocEntries: [TOCEntry] = []

    /// The ID of the currently selected chapter (EPUB only).
    /// `nil` for non-EPUB items or when no specific chapter is selected.
    var currentChapterId: String? = nil

    /// Whether the item data is being loaded from the backend.
    var isLoading = false

    /// Non-nil when a load or parse error occurred.
    var error: String? = nil

    // MARK: - Dependencies

    /// Service layer for fetching and updating library items.
    private let libraryService = LibraryService()

    // MARK: - Public API

    /// Fetch the full library item by ID and extract sentences from its content.
    ///
    /// This is the primary entry point, called when the reader screen appears.
    /// After loading:
    /// - For plain text types: sentences are extracted via `SentenceSplitter`.
    /// - For EPUB: chapters are parsed from JSON, TOC entries are extracted,
    ///   and the last-read chapter (or the first) is selected.
    ///
    /// - Parameter id: The library item ID to load.
    func loadItem(id: String) async {
        isLoading = true
        error = nil

        do {
            let loadedItem = try await libraryService.fetchItem(id: id)
            item = loadedItem

            // Extract sentences based on document type
            switch loadedItem.type {
            case .epub:
                parseEpubContent(loadedItem)
            case .txt, .web, .pdf:
                extractPlainTextSentences(loadedItem)
            }
        } catch {
            self.error = "Failed to load item: \(error.localizedDescription)"
        }

        isLoading = false
    }

    /// Extract sentences from the item's content based on its type.
    ///
    /// Called after `loadItem()` and when switching chapters. For non-EPUB types
    /// this uses `SentenceSplitter`; for EPUB it extracts from the current chapter's
    /// content blocks.
    func extractSentences() {
        guard let item else { return }

        switch item.type {
        case .epub:
            extractEpubSentences()
        case .txt, .web, .pdf:
            extractPlainTextSentences(item)
        }
    }

    /// Select a specific EPUB chapter by its ID.
    ///
    /// Updates the current chapter, re-extracts sentences for TTS, and persists
    /// the chapter selection to the server so it's restored on next visit.
    ///
    /// - Parameter id: The chapter ID to select.
    func selectChapter(id: String) async {
        guard item?.type == .epub else { return }

        currentChapterId = id
        extractEpubSentences()

        // Persist the chapter selection to the server
        if let itemId = item?.id {
            try? await libraryService.updateChapter(id: itemId, chapter: id)
        }
    }

    /// Save the current reading progress to the server.
    ///
    /// Progress is computed as a percentage: `sentenceIndex / totalSentences * 100`.
    /// Clamped to 0–100 range. Called by the player/reader UI as the user advances
    /// through sentences.
    ///
    /// - Parameter sentenceIndex: The zero-based index of the current sentence.
    func saveProgress(sentenceIndex: Int) async {
        guard let item, !sentences.isEmpty else { return }

        // Compute progress as a percentage of total sentences
        let progress = min(100, max(0, Int(Double(sentenceIndex) / Double(sentences.count) * 100)))

        // Only save if the progress actually changed to avoid unnecessary network calls
        guard progress != item.progress else { return }

        try? await libraryService.updateProgress(id: item.id, progress: progress)
    }

    // MARK: - Private Helpers

    /// Extract sentences from plain text content using `SentenceSplitter`.
    ///
    /// Used for `.txt`, `.web`, and `.pdf` types where the content is a single
    /// text string (HTML may be stripped by the backend before reaching here).
    ///
    /// - Parameter item: The library item whose `content` field contains plain text.
    private func extractPlainTextSentences(_ item: LibraryItem) {
        sentences = SentenceSplitter.splitSentences(item.content)
    }

    /// Parse the EPUB content JSON into chapters, extract TOC entries, and
    /// select the initial chapter.
    ///
    /// The `content` field for EPUB items is a JSON-encoded array of `EpubChapter`
    /// objects. Each chapter has an `id`, `title`, and array of `ContentBlock` values.
    ///
    /// - Parameter item: The library item with EPUB-type content.
    private func parseEpubContent(_ item: LibraryItem) {
        guard let contentData = item.content.data(using: .utf8) else {
            error = "Failed to read EPUB content"
            return
        }

        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let parsedChapters = try decoder.decode([EpubChapter].self, from: contentData)
            chapters = parsedChapters

            // Extract TOC entries from chapters for navigation
            tocEntries = parsedChapters.map { chapter in
                TOCEntry(id: chapter.id, title: chapter.title, href: nil)
            }

            // Restore the last-read chapter, or default to the first
            let restoredChapterId = item.currentChapter ?? parsedChapters.first?.id
            currentChapterId = restoredChapterId

            // Extract sentences from the selected chapter
            extractEpubSentences()
        } catch {
            self.error = "Failed to parse EPUB content: \(error.localizedDescription)"
            // Fallback: treat the content as plain text
            sentences = SentenceSplitter.splitSentences(item.content)
        }
    }

    /// Extract `Sentence` values from the current EPUB chapter's content blocks.
    ///
    /// Walks through each `ContentBlock` in the selected chapter, collecting
    /// text from `RichSentence` values and falling back to `SentenceSplitter`
    /// for blocks without pre-split sentences.
    ///
    /// The extracted `Sentence` values are plain-text only (for TTS). The view
    /// layer uses the original `ContentBlock`/`RichSentence` types for styled rendering.
    private func extractEpubSentences() {
        guard let chapter = chapters.first(where: { $0.id == currentChapterId }) ?? chapters.first else {
            sentences = []
            return
        }

        var extracted: [Sentence] = []
        var runningIndex = 0

        // Walk through each content block in the chapter and extract sentences
        for block in chapter.blocks {
            let blockSentences = extractSentencesFromBlock(block, startIndex: runningIndex)
            extracted.append(contentsOf: blockSentences)
            runningIndex += blockSentences.count
        }

        sentences = extracted
    }

    /// Extract `Sentence` values from a single content block.
    ///
    /// Different block types contain sentences in different structures:
    /// - `heading`, `paragraph`, `blockquote`: directly contain `RichSentence` arrays.
    /// - `list`: each list item contains a `RichSentence` array.
    /// - `code`: treated as a single sentence with the full code text.
    /// - `image`, `separator`: no extractable text, skipped.
    ///
    /// - Parameters:
    ///   - block: The content block to extract from.
    ///   - startIndex: The starting sentence index for this block.
    /// - Returns: Array of extracted `Sentence` values.
    private func extractSentencesFromBlock(_ block: ContentBlock, startIndex: Int) -> [Sentence] {
        var sentences: [Sentence] = []
        var idx = startIndex

        switch block {
        case .heading(_, let richSentences, _),
             .paragraph(let richSentences, _),
             .blockquote(let richSentences, _):
            // These block types directly contain an array of RichSentence values
            for rs in richSentences {
                let trimmed = rs.text.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !trimmed.isEmpty else { continue }
                sentences.append(Sentence(index: idx, text: trimmed))
                idx += 1
            }

        case .list(_, let items, _):
            // List items each contain their own RichSentence arrays
            for item in items {
                for rs in item.sentences {
                    let trimmed = rs.text.trimmingCharacters(in: .whitespacesAndNewlines)
                    guard !trimmed.isEmpty else { continue }
                    sentences.append(Sentence(index: idx, text: trimmed))
                    idx += 1
                }
            }

        case .code(let text, _):
            // Code blocks are treated as single sentences
            let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty {
                sentences.append(Sentence(index: idx, text: trimmed))
            }

        case .image, .separator:
            // No extractable text from image or separator blocks
            break
        }

        return sentences
    }
}
