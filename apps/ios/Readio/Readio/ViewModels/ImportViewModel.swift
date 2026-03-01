import Foundation
import SwiftUI

// MARK: - ImportViewModel
/// View model for the Import screen, handling both file upload and URL import flows.
///
/// The import screen provides two ways to add content to the library:
///
/// 1. **File import:** The user picks a file (EPUB, PDF, TXT) via the system file
///    picker. The file data is uploaded to `POST /api/library/import/file` as a
///    multipart form. Supported formats are determined by the backend.
///
/// 2. **URL import:** The user pastes a URL (article, web page). The backend
///    scrapes the content via `POST /api/library/import/url`, extracts text,
///    and creates a new library item of type `.web`.
///
/// Both flows are mutually exclusive — only one import can run at a time.
///
/// ## State Machine
///
/// ```
/// idle → importing → success (importedItem set)
///                   → error (importError set)
/// ```
///
/// After a successful import, the caller can navigate to the imported item's
/// reader/player screen using `importedItem.id`.
///
/// ## Usage Example
///
/// ```swift
/// struct ImportView: View {
///     @State private var viewModel = ImportViewModel()
///
///     var body: some View {
///         VStack {
///             TextField("URL", text: $viewModel.urlInput)
///             Button("Import") { Task { await viewModel.importURL() } }
///
///             if viewModel.isImporting { ProgressView() }
///             if let error = viewModel.importError { Text(error).foregroundColor(.red) }
///             if let item = viewModel.importedItem { Text("Imported: \(item.title)") }
///         }
///     }
/// }
/// ```
@MainActor
@Observable
final class ImportViewModel {

    // MARK: - Published State

    /// Whether an import operation is currently in progress.
    /// The UI should disable the import buttons and show a progress indicator.
    var isImporting = false

    /// Non-nil when the import failed. Contains a user-friendly error message.
    /// Cleared on the next import attempt or on `reset()`.
    var importError: String? = nil

    /// The successfully imported library item. Non-nil only after a successful
    /// import. The caller can navigate to `importedItem.id` for reading.
    var importedItem: LibraryItem? = nil

    /// The URL entered by the user for URL-based import.
    /// Bound to a `TextField` in the view layer.
    var urlInput = ""

    /// Optional title override entered by the user.
    /// If empty, the backend derives the title from the content (article title,
    /// filename, etc.).
    var titleInput = ""

    // MARK: - Dependencies

    /// Service layer for importing files and URLs into the library.
    private let libraryService = LibraryService()

    // MARK: - Public API

    /// Import a file (EPUB, PDF, TXT) by uploading its data to the backend.
    ///
    /// The file data should come from a `fileImporter` or document picker.
    /// The filename is used by the backend to determine the document type
    /// and to generate a default title if none is provided.
    ///
    /// - Parameters:
    ///   - data: Raw file bytes to upload.
    ///   - filename: Original filename including extension (e.g., "book.epub").
    func importFile(data: Data, filename: String) async {
        guard !isImporting else { return }

        isImporting = true
        importError = nil
        importedItem = nil

        do {
            let item = try await libraryService.importFile(
                data: data,
                filename: filename,
                title: titleInput.isEmpty ? nil : titleInput
            )
            importedItem = item
        } catch {
            importError = "File import failed: \(error.localizedDescription)"
        }

        isImporting = false
    }

    /// Import content from a URL by sending it to the backend for scraping.
    ///
    /// The backend fetches the URL, extracts readable text content, and creates
    /// a new library item of type `.web`. The optional `titleInput` overrides
    /// the automatically extracted title.
    func importURL() async {
        // Validate that the URL input is not empty and looks like a URL
        let trimmedURL = urlInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedURL.isEmpty else {
            importError = "Please enter a URL"
            return
        }

        guard trimmedURL.hasPrefix("http://") || trimmedURL.hasPrefix("https://") else {
            importError = "Please enter a valid URL starting with http:// or https://"
            return
        }

        guard !isImporting else { return }

        isImporting = true
        importError = nil
        importedItem = nil

        do {
            let item = try await libraryService.importURL(
                url: trimmedURL,
                title: titleInput.isEmpty ? nil : titleInput
            )
            importedItem = item
        } catch {
            importError = "URL import failed: \(error.localizedDescription)"
        }

        isImporting = false
    }

    /// Reset all import state to the initial idle state.
    ///
    /// Called when the user dismisses the import sheet or wants to start
    /// a fresh import without residual state from a previous attempt.
    func reset() {
        isImporting = false
        importError = nil
        importedItem = nil
        urlInput = ""
        titleInput = ""
    }
}
