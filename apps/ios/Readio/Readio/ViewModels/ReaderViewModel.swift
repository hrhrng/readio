import Foundation
import SwiftUI

enum AppleSearchWalkDirection: Sendable {
    case previous
    case next
}

struct AppleReaderSearchHit: Codable, Equatable, Identifiable, Sendable {
    let hitId: String
    let charRange: [UInt64]
    let blockId: String
    let snippet: String
    let snippetMatchRange: [UInt32]

    var id: String { hitId }

    enum CodingKeys: String, CodingKey {
        case hitId = "hit_id"
        case charRange = "char_range"
        case blockId = "block_id"
        case snippet
        case snippetMatchRange = "snippet_match_range"
    }
}

struct AppleReaderSearchResult: Equatable, Sendable {
    let foldedQuery: String
    let totalOccurrences: UInt64
    let totalPassages: UInt64
}

private struct AppleReaderSearchPage: Decodable {
    let foldedQuery: String
    let totalOccurrences: UInt64
    let totalPassages: UInt64
    let hits: [AppleReaderSearchHit]
    let nextCursor: String?

    enum CodingKeys: String, CodingKey {
        case foldedQuery = "folded_query"
        case totalOccurrences = "total_occurrences"
        case totalPassages = "total_blocks"
        case hits
        case nextCursor = "next_cursor"
    }
}

// MARK: - ReaderViewModel
/// View model for the Reader screen that displays document content and manages
/// projection of parser-owned Reader IR for rendering and playback.
///
/// The reader is the core content consumption screen. It:
/// 1. Fetches the full `LibraryItem` (including `content`) from the backend.
/// 2. Loads one Reader IR from the local-first library boundary.
/// 3. Manages chapter navigation for EPUB documents.
/// 4. Persists reading progress back to the server.
///
/// ## Content Handling by Document Type
///
/// TXT, Web, PDF, and EPUB converge on the same `ReadioReaderIR`. SwiftUI only
/// maps Rust-provided blocks, sentence ranges, inline spans, and navigation into
/// native view values; it does not parse or split document content.
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

    /// Structured EPUB or PDF chapters.
    /// Each chapter contains shared semantic blocks with rich sentences.
    var chapters: [EpubChapter] = []

    /// Table of contents entries parsed from the EPUB navigation document.
    /// Used to render the TOC panel and for chapter-level navigation.
    var tocEntries: [TOCEntry] = []

    /// The ID of the currently selected structured EPUB/PDF chapter.
    /// `nil` for unstructured items or when no chapter is selected.
    var currentChapterId: String? = nil

    /// Exact sentence restored from the Rust scalar-position projection.
    var restoredSentenceIndex = 0

    /// Current optimistic-concurrency version for the item cursor.
    var positionVersion: UInt64 = 0

    /// Rust-projected bookmarks for the open item.
    var bookmarks: [AppleBookmark] = []

    /// Full local-book search projection returned by Rust Core.
    var searchQuery = ""
    var searchResult: AppleReaderSearchResult? = nil
    var searchHits: [AppleReaderSearchHit] = []
    var activeSearchHitIndex: Int? = nil
    var searchError: String? = nil

    /// Whether the item data is being loaded from the backend.
    var isLoading = false

    /// Non-nil when a load or parse error occurred.
    var error: String? = nil

    /// Parser-owned value currently projected by the GUI and playback engine.
    private(set) var readerIR: ReadioReaderIR? = nil
    private(set) var highlightDocument: ReadioHighlightDocument? = nil

    // MARK: - Dependencies

    /// Service layer for fetching and updating library items.
    private let libraryService: LibraryService
    private let positionService: AppleReadingPositionService
    private let bookmarkService: AppleBookmarkService
    private let sdk = ReadioSDK()
    private let sessionID: String
    private var documentRevision = ""
    private var currentDocumentJSON = ""
    private var readerProjection: AppleReaderProjection? = nil
    private var lastSavedCharOffset: Int?
    private var pendingProgressSentenceIndex: Int?
    private var progressSaveTask: Task<Void, Never>?

    init(
        libraryService: LibraryService = LibraryService(),
        positionService: AppleReadingPositionService = AppleReadingPositionService(),
        bookmarkService: AppleBookmarkService = AppleBookmarkService(),
        sessionID: String = UUID().uuidString
    ) {
        self.libraryService = libraryService
        self.positionService = positionService
        self.bookmarkService = bookmarkService
        self.sessionID = sessionID
    }

    // MARK: - Public API

    /// Fetch the full library item by ID and extract sentences from its content.
    ///
    /// This is the primary entry point, called when the reader screen appears.
    /// After loading:
    /// - Every type is installed from Reader IR.
    /// - Structured publications restore the last-read chapter when possible.
    ///
    /// - Parameter id: The library item ID to load.
    func loadItem(id: String) async {
        isLoading = true
        error = nil

        do {
            let loadedItem = try await libraryService.fetchItem(id: id)
            item = loadedItem

            let projection = try await libraryService.loadReaderProjection(for: loadedItem)
            try installReaderProjection(projection, for: loadedItem)
            try await restoreReadingPosition(itemID: loadedItem.id)
            bookmarks = try await bookmarkService.list(itemID: loadedItem.id)
        } catch {
            self.error = "Failed to load item: \(error.localizedDescription)"
        }

        isLoading = false
    }

    /// Refreshes the active GUI/playback slice from the already-installed IR.
    func extractSentences() {
        installActiveSentenceProjection()
    }

    func loadEpubAsset(assetID: String) async throws -> Data {
        guard let itemID = item?.id else {
            throw LibraryServiceError.invalidMetadata
        }
        return try await libraryService.loadEpubAsset(
            itemID: itemID,
            assetID: assetID
        )
    }

    /// Select a specific EPUB chapter by its ID.
    ///
    /// Updates the current chapter, re-extracts sentences for TTS, and persists
    /// the chapter selection to the server so it's restored on next visit.
    ///
    /// - Parameter id: The chapter ID to select.
    func selectChapter(id: String) async {
        guard let item, readerProjection != nil else { return }

        let navigation = tocEntries.first(where: { $0.id == id })
        let chapterId = navigation?.chapterId ?? id
        currentChapterId = chapterId
        installActiveSentenceProjection()

        if usesStructuredChapters(item) {
            try? await libraryService.updateChapter(id: item.id, chapter: chapterId)
        }
        restoredSentenceIndex = navigation?.sentenceIndex ?? 0
        lastSavedCharOffset = nil
    }

    /// Save the current reading progress to the server.
    ///
    /// Progress is computed as a percentage: `sentenceIndex / totalSentences * 100`.
    /// Clamped to 0–100 range. Called by the player/reader UI as the user advances
    /// through sentences.
    ///
    /// - Parameter sentenceIndex: The zero-based index of the current sentence.
    func saveProgress(sentenceIndex: Int) async {
        guard let item,
              !sentences.isEmpty,
              sentences.indices.contains(sentenceIndex),
              !documentRevision.isEmpty else { return }

        let charOffset = sentences[sentenceIndex].charOffset
        guard charOffset != lastSavedCharOffset else { return }

        do {
            let locator = try createTextLocator(at: UInt64(charOffset))
            let response = try await positionService.save(
                itemID: item.id,
                mutation: AppleReadingPositionMutation(
                    documentRevision: documentRevision,
                    charOffset: UInt64(charOffset),
                    affinity: .forward,
                    observedAtUnixMilliseconds: Int64(Date().timeIntervalSince1970 * 1_000),
                    sessionId: sessionID,
                    mutationId: UUID().uuidString,
                    baseVersion: positionVersion,
                    locator: locator
                )
            )
            positionVersion = response.version
            guard response.status == .applied else {
                if let current = response.position,
                   current.documentRevision == documentRevision {
                    restoredSentenceIndex = resolveSentenceIndex(at: current.charOffset)
                    lastSavedCharOffset = Int(exactly: current.charOffset)
                }
                return
            }
            restoredSentenceIndex = sentenceIndex
            lastSavedCharOffset = charOffset
        } catch {
            self.error = "Failed to save reading position: \(error.localizedDescription)"
            return
        }

        // Compatibility percentage is a projection of the same whole-book
        // Unicode-scalar cursor; sentence counts are not a distance metric.
        let progress: Int
        do {
            progress = try summaryProgressPercentage(at: UInt64(charOffset))
        } catch {
            self.error = "Failed to project reading progress: \(error.localizedDescription)"
            return
        }

        // Only save if the progress actually changed to avoid unnecessary network calls
        guard progress != item.progress else { return }

        try? await libraryService.updateProgress(id: item.id, progress: progress)
    }

    private func summaryProgressPercentage(at charOffset: UInt64) throws -> Int {
        guard !currentDocumentJSON.isEmpty else {
            throw ReadioSDKError.missingValue
        }
        let requestJSON = String(
            decoding: try JSONSerialization.data(withJSONObject: [
                "char_offset": charOffset,
                "affinity": ApplePositionAffinity.forward.rawValue,
            ]),
            as: UTF8.self
        )
        let projectionJSON = try sdk.projectOffset(
            documentJSON: currentDocumentJSON,
            requestJSON: requestJSON
        )
        guard let projection = try JSONSerialization.jsonObject(
            with: Data(projectionJSON.utf8)
        ) as? [String: Any],
            let basisPoints = projection["progress_basis_points"] as? NSNumber
        else {
            throw ReadioSDKError.missingValue
        }
        return min(100, max(0, (basisPoints.intValue + 50) / 100))
    }

    /// Coalesces rapid Auto/read-aloud cursor changes behind one serialized
    /// writer. Every mutation therefore observes the version produced by the
    /// previous mutation, while pending work collapses to the newest sentence.
    func enqueueProgressSave(sentenceIndex: Int) {
        pendingProgressSentenceIndex = sentenceIndex
        guard progressSaveTask == nil else { return }
        progressSaveTask = Task { @MainActor [weak self] in
            await self?.drainProgressSaves()
        }
    }

    /// Awaitable durability barrier used by lifecycle handling and tests.
    func flushProgressSaves() async {
        while let task = progressSaveTask {
            await task.value
        }
    }

    func addBookmark(sentenceIndex: Int, note: String = "") async {
        guard let item,
              sentences.indices.contains(sentenceIndex),
              !currentDocumentJSON.isEmpty else { return }
        let charOffset = UInt64(sentences[sentenceIndex].charOffset)
        let existing = bookmarks.first(where: { $0.charOffset == charOffset })
        let now = Int64(Date().timeIntervalSince1970 * 1_000)
        let trimmedNote = note.trimmingCharacters(in: .whitespacesAndNewlines)
        let resolvedNote = trimmedNote.isEmpty
            ? sentences[sentenceIndex].text
            : trimmedNote
        do {
            let transition = try await bookmarkService.upsert(
                documentJSON: currentDocumentJSON,
                request: AppleBookmarkUpsertRequest(
                    bookmarkId: existing?.id ?? UUID().uuidString,
                    itemId: item.id,
                    charOffset: charOffset,
                    affinity: .forward,
                    note: resolvedNote,
                    createdAtUnixMilliseconds: existing?.createdAtUnixMilliseconds ?? now,
                    updatedAtUnixMilliseconds: now,
                    baseVersion: existing?.version ?? 0
                )
            )
            storeBookmark(transition.bookmark)
        } catch {
            self.error = "Failed to save bookmark: \(error.localizedDescription)"
        }
    }

    func renameBookmark(_ bookmark: AppleBookmark, note: String) async {
        guard !currentDocumentJSON.isEmpty else { return }
        let sentence = resolveSentenceIndex(at: bookmark.charOffset)
        let trimmedNote = note.trimmingCharacters(in: .whitespacesAndNewlines)
        let resolvedNote = trimmedNote.isEmpty && sentences.indices.contains(sentence)
            ? sentences[sentence].text
            : trimmedNote
        do {
            let transition = try await bookmarkService.upsert(
                documentJSON: currentDocumentJSON,
                request: AppleBookmarkUpsertRequest(
                    bookmarkId: bookmark.id,
                    itemId: bookmark.itemId,
                    charOffset: bookmark.charOffset,
                    affinity: bookmark.affinity,
                    note: resolvedNote,
                    createdAtUnixMilliseconds: bookmark.createdAtUnixMilliseconds,
                    updatedAtUnixMilliseconds: Int64(
                        Date().timeIntervalSince1970 * 1_000
                    ),
                    baseVersion: bookmark.version
                )
            )
            storeBookmark(transition.bookmark)
        } catch {
            if let item {
                bookmarks = (try? await bookmarkService.list(itemID: item.id))
                    ?? bookmarks
            }
            self.error = "Failed to rename bookmark: \(error.localizedDescription)"
        }
    }

    func deleteBookmark(_ bookmark: AppleBookmark) async {
        do {
            try await bookmarkService.delete(bookmarkID: bookmark.id)
            bookmarks.removeAll(where: { $0.id == bookmark.id })
        } catch {
            self.error = "Failed to delete bookmark: \(error.localizedDescription)"
        }
    }

    func sentenceIndex(for bookmark: AppleBookmark) -> Int {
        resolveSentenceIndex(at: bookmark.charOffset)
    }

    /// Resolves one whole-book bookmark into the active chapter projection.
    /// Structured EPUB and PDF readers keep chapter-local playback arrays, so
    /// resolving only against the currently visible array can seek to the right
    /// numeric index in the wrong chapter.
    @discardableResult
    func activateBookmark(_ bookmark: AppleBookmark) -> Int? {
        if item.map(usesStructuredChapters) == true,
           let sentence = selectReaderProjection(containing: bookmark.charOffset) {
            restoredSentenceIndex = sentence
            return sentence
        }
        let sentence = resolveSentenceIndex(at: bookmark.charOffset)
        restoredSentenceIndex = sentence
        return sentence
    }

    /// Searches the indexed reader document entirely in Rust. Pages are drained
    /// locally so previous/next walking can wrap deterministically at either end.
    func search(_ query: String) {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            searchQuery = ""
            searchResult = nil
            searchHits = []
            activeSearchHitIndex = nil
            searchError = nil
            return
        }
        searchQuery = trimmed
        guard !currentDocumentJSON.isEmpty, !documentRevision.isEmpty else {
            searchError = "This book is still being indexed"
            return
        }

        do {
            var cursor: String? = nil
            var allHits: [AppleReaderSearchHit] = []
            var summary: AppleReaderSearchResult?
            repeat {
                let request: [String: Any] = [
                    "document_revision": documentRevision,
                    "query": trimmed,
                    "cursor": cursor ?? NSNull(),
                    "limit": 256,
                ]
                let requestJSON = String(
                    decoding: try JSONSerialization.data(withJSONObject: request),
                    as: UTF8.self
                )
                let pageJSON = try sdk.searchDocument(
                    documentJSON: currentDocumentJSON,
                    requestJSON: requestJSON
                )
                let page = try JSONDecoder().decode(
                    AppleReaderSearchPage.self,
                    from: Data(pageJSON.utf8)
                )
                summary = AppleReaderSearchResult(
                    foldedQuery: page.foldedQuery,
                    totalOccurrences: page.totalOccurrences,
                    totalPassages: page.totalPassages
                )
                allHits.append(contentsOf: page.hits)
                cursor = page.nextCursor
            } while cursor != nil

            searchResult = summary
            searchHits = allHits
            activeSearchHitIndex = nil
            searchError = nil
        } catch {
            searchResult = nil
            searchHits = []
            activeSearchHitIndex = nil
            searchError = "Search failed: \(error.localizedDescription)"
        }
    }

    /// Activates a source-range hit and returns its sentence projection for the
    /// player. The query and hit list remain intact while reading continues.
    @discardableResult
    func activateSearchHit(at index: Int) -> Int? {
        guard searchHits.indices.contains(index),
              let charOffset = searchHits[index].charRange.first else { return nil }
        activeSearchHitIndex = index
        if item.map(usesStructuredChapters) == true,
           let sentence = selectReaderProjection(containing: charOffset) {
            return sentence
        }
        return resolveSentenceIndex(at: charOffset)
    }

    @discardableResult
    func walkSearchHits(_ direction: AppleSearchWalkDirection) -> Int? {
        guard !searchHits.isEmpty else { return nil }
        let target: Int
        switch (activeSearchHitIndex, direction) {
        case (nil, .next):
            target = 0
        case (nil, .previous):
            target = searchHits.count - 1
        case let (.some(index), .next):
            target = (index + 1) % searchHits.count
        case let (.some(index), .previous):
            target = (index - 1 + searchHits.count) % searchHits.count
        }
        return activateSearchHit(at: target)
    }

    /// Refresh lightweight Rust projections after a successful cloud sync.
    /// The open document and playback engine stay intact; only synced cursor and
    /// bookmark state are rematerialized from the shared local replica.
    func refreshReplicaProjection() async {
        guard let item else { return }
        do {
            async let refreshedBookmarks = bookmarkService.list(itemID: item.id)
            async let refreshedPosition = positionService.fetch(itemID: item.id)
            let (nextBookmarks, nextPosition) = try await (
                refreshedBookmarks,
                refreshedPosition
            )
            bookmarks = nextBookmarks
            if let nextPosition {
            positionVersion = nextPosition.version
            if let charOffset = try resolvedCharOffset(for: nextPosition) {
                if usesStructuredChapters(item) {
                    _ = selectReaderProjection(containing: charOffset)
                }
                restoredSentenceIndex = resolveSentenceIndex(
                    at: charOffset
                )
                    lastSavedCharOffset = Int(exactly: charOffset)
                }
            } else {
                positionVersion = 0
                restoredSentenceIndex = 0
                lastSavedCharOffset = nil
            }
        } catch {
            self.error = "Failed to refresh synced reading state: \(error.localizedDescription)"
        }
    }

    // MARK: - Private Helpers

    private func installReaderProjection(
        _ projection: AppleReaderProjection,
        for item: LibraryItem
    ) throws {
        readerProjection = projection
        readerIR = projection.reader
        chapters = projection.chapters
        tocEntries = projection.navigation
        let requestedChapter = usesStructuredChapters(item)
            ? item.currentChapter.flatMap { requested in
                projection.chapters.contains(where: { $0.id == requested })
                    ? requested
                    : nil
            }
            : nil
        currentChapterId = requestedChapter ?? projection.chapters.first?.id
        installActiveSentenceProjection()
        documentRevision = projection.reader.document.revision.revisionDigest
        currentDocumentJSON = String(
            decoding: try JSONEncoder().encode(projection.reader.document),
            as: UTF8.self
        )
        highlightDocument = try sdk.makeHighlightDocument(reader: projection.reader)
    }

    private func installActiveSentenceProjection() {
        guard let readerProjection else {
            sentences = []
            return
        }
        let chapterID = currentChapterId ?? readerProjection.chapters.first?.id
        currentChapterId = chapterID
        let chapterSentences = readerProjection.sentences(in: chapterID)
        sentences = chapterSentences.isEmpty
            ? readerProjection.allSentences
            : chapterSentences
    }

    private func createTextLocator(at charOffset: UInt64) throws -> AppleTextLocator {
        guard !currentDocumentJSON.isEmpty else {
            throw ReadioSDKError.missingValue
        }
        let requestJSON = String(
            decoding: try JSONSerialization.data(withJSONObject: [
                "char_offset": charOffset,
                "affinity": ApplePositionAffinity.forward.rawValue,
            ]),
            as: UTF8.self
        )
        let locatorJSON = try sdk.createTextLocator(
            documentJSON: currentDocumentJSON,
            requestJSON: requestJSON
        )
        return try JSONDecoder().decode(
            AppleTextLocator.self,
            from: Data(locatorJSON.utf8)
        )
    }

    private func restoreReadingPosition(itemID: String) async throws {
        guard let position = try await positionService.fetch(itemID: itemID) else {
            restoredSentenceIndex = 0
            positionVersion = 0
            lastSavedCharOffset = nil
            return
        }
        positionVersion = position.version
        guard let charOffset = try resolvedCharOffset(for: position) else {
            restoredSentenceIndex = 0
            lastSavedCharOffset = nil
            return
        }
        if item.map(usesStructuredChapters) == true {
            _ = selectReaderProjection(containing: charOffset)
        }
        restoredSentenceIndex = resolveSentenceIndex(at: charOffset)
        lastSavedCharOffset = Int(exactly: charOffset)
    }

    private func resolvedCharOffset(
        for position: AppleReadingPositionRecord
    ) throws -> UInt64? {
        if position.documentRevision == documentRevision {
            return position.charOffset
        }
        guard let locator = position.locator, !currentDocumentJSON.isEmpty else {
            return nil
        }
        let locatorJSON = String(
            decoding: try JSONEncoder().encode(locator),
            as: UTF8.self
        )
        let resolutionJSON = try sdk.restoreTextLocator(
            documentJSON: currentDocumentJSON,
            locatorJSON: locatorJSON
        )
        guard let resolution = try JSONSerialization.jsonObject(
            with: Data(resolutionJSON.utf8)
        ) as? [String: Any],
            let projection = resolution["projection"] as? [String: Any],
            let charOffset = projection["char_offset"] as? NSNumber
        else {
            throw ReadioSDKError.missingValue
        }
        return charOffset.uint64Value
    }

    private func resolveSentenceIndex(at charOffset: UInt64) -> Int {
        guard let offset = Int(exactly: charOffset) else {
            return max(0, sentences.count - 1)
        }
        return sentences.last(where: { $0.charOffset <= offset })?.index ?? 0
    }

    /// Selects the chapter containing one whole-book scalar coordinate and
    /// returns the chapter-local sentence cursor used by the playback reducer.
    private func selectReaderProjection(containing charOffset: UInt64) -> Int? {
        guard let readerProjection,
              let chapterID = readerProjection.chapterID(containing: charOffset) else {
            return nil
        }
        currentChapterId = chapterID
        installActiveSentenceProjection()
        return resolveSentenceIndex(at: charOffset)
    }

    private func isStructuredPDF(_ item: LibraryItem) -> Bool {
        item.type == .pdf && item.contentFormat == "readio-pdf-v1"
    }

    private func usesStructuredChapters(_ item: LibraryItem) -> Bool {
        item.type == .epub || isStructuredPDF(item)
    }

    private func drainProgressSaves() async {
        while let sentence = pendingProgressSentenceIndex {
            pendingProgressSentenceIndex = nil
            await saveProgress(sentenceIndex: sentence)
        }
        progressSaveTask = nil
    }

    private func storeBookmark(_ bookmark: AppleBookmark) {
        bookmarks.removeAll(where: { $0.id == bookmark.id })
        bookmarks.append(bookmark)
        bookmarks.sort {
            ($0.charOffset, $0.createdAtUnixMilliseconds, $0.id)
                < ($1.charOffset, $1.createdAtUnixMilliseconds, $1.id)
        }
    }
}
