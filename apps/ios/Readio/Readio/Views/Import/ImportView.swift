import SwiftUI
import UniformTypeIdentifiers

// MARK: - ImportView
/// The import screen for adding new content to the Readio library.
///
/// **Two Import Methods:**
///
/// 1. **File Import** — Opens the system file picker for EPUB, PDF, and TXT files.
///    Uses SwiftUI's `.fileImporter` modifier which presents the system document picker.
///    Supported UTTypes: `.epub`, `.pdf`, `.plainText`.
///
/// 2. **URL Import** — Accepts a web URL and optional custom title. The backend
///    scrapes the URL content (article text, metadata) and creates a new library item.
///    Useful for saving articles, blog posts, and web pages for TTS reading.
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │  Import                 (nav title)  │
/// ├──────────────────────────────────────┤
/// │                                      │
/// │  ┌────────────────────────────────┐  │
/// │  │  📁 Import File               │  │  ← File picker button
/// │  │  EPUB, PDF, or TXT            │  │
/// │  └────────────────────────────────┘  │
/// │                                      │
/// │  ── Or import from URL ──────────── │
/// │                                      │
/// │  URL:   [ https://example.com    ]  │
/// │  Title: [ Optional custom title  ]  │
/// │                                      │
/// │  [ Import from URL ]                 │
/// │                                      │
/// │  ┌────────────────────────────────┐  │
/// │  │  ✅ Successfully imported!     │  │  ← Success/error state
/// │  │  [ Open Book ]                │  │
/// │  └────────────────────────────────┘  │
/// └──────────────────────────────────────┘
/// ```
///
/// **State Flow:**
/// idle → importing (with progress) → success (with "Open" option) / error (with retry)
struct ImportView: View {

    // MARK: - State

    /// The view model managing import operations (file upload, URL scraping).
    @State private var viewModel = ImportViewModel()

    /// Controls the visibility of the system file picker sheet.
    @State private var showingFilePicker = false

    /// Controls navigation to the reader view after a successful import.
    @State private var navigateToReader = false

    // MARK: - Body

    var body: some View {
        ScrollView {
            VStack(spacing: 24) {
                // MARK: File Import Section
                fileImportSection

                // MARK: Divider
                dividerWithLabel("Or import from URL")

                // MARK: URL Import Section
                urlImportSection

                // MARK: Import Status
                // Shows progress, success, or error states during/after import.
                if viewModel.isImporting || viewModel.importedItem != nil || viewModel.importError != nil {
                    importStatusSection
                }
            }
            .padding()
        }
        .navigationTitle("Import")
        // MARK: File Picker Modifier
        // `.fileImporter` presents the system document picker when `showingFilePicker` is true.
        // Supports multiple document types simultaneously.
        .fileImporter(
            isPresented: $showingFilePicker,
            allowedContentTypes: supportedFileTypes,
            allowsMultipleSelection: false
        ) { result in
            handleFileImportResult(result)
        }
        // Navigate to the reader when an item is successfully imported
        .navigationDestination(isPresented: $navigateToReader) {
            if let item = viewModel.importedItem {
                ReaderView(itemId: item.id)
            }
        }
    }

    // MARK: - Supported File Types

    /// The UTType values accepted by the file picker.
    /// - `.epub`: EPUB e-book files
    /// - `.pdf`: PDF documents
    /// - `.plainText`: Plain text files (.txt)
    private var supportedFileTypes: [UTType] {
        [
            .init(filenameExtension: "epub") ?? .data,
            .pdf,
            .plainText
        ]
    }

    // MARK: - File Import Section

    /// Large tappable area that opens the system file picker.
    /// Styled as a prominent card with an icon and supporting text.
    private var fileImportSection: some View {
        Button {
            showingFilePicker = true
        } label: {
            VStack(spacing: 12) {
                // Large folder icon
                Image(systemName: "doc.badge.plus")
                    .font(.system(size: 40))
                    .foregroundStyle(.accentColor)

                Text("Import File")
                    .font(.headline)
                    .foregroundStyle(.primary)

                Text("EPUB, PDF, or TXT files")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)

                // Supported format badges
                HStack(spacing: 8) {
                    FormatBadge(text: "EPUB", color: .indigo)
                    FormatBadge(text: "PDF", color: .red)
                    FormatBadge(text: "TXT", color: .gray)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 32)
            .background(
                RoundedRectangle(cornerRadius: 16)
                    .fill(Color(.secondarySystemGroupedBackground))
                    .overlay(
                        RoundedRectangle(cornerRadius: 16)
                            .strokeBorder(
                                style: StrokeStyle(lineWidth: 2, dash: [8, 4])
                            )
                            .foregroundStyle(Color(.separator))
                    )
            )
        }
        .buttonStyle(.plain)
        .disabled(viewModel.isImporting) // Prevent double-import
    }

    // MARK: - URL Import Section

    /// Form fields for importing content from a web URL.
    private var urlImportSection: some View {
        VStack(spacing: 14) {
            // MARK: URL Field
            VStack(alignment: .leading, spacing: 6) {
                Text("URL")
                    .font(.subheadline)
                    .fontWeight(.medium)
                    .foregroundStyle(.secondary)

                TextField("https://example.com/article", text: $viewModel.urlInput)
                    .textFieldStyle(.roundedBorder)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .disabled(viewModel.isImporting)
            }

            // MARK: Title Field (Optional)
            VStack(alignment: .leading, spacing: 6) {
                Text("Title (optional)")
                    .font(.subheadline)
                    .fontWeight(.medium)
                    .foregroundStyle(.secondary)

                TextField("Custom title for this article", text: $viewModel.titleInput)
                    .textFieldStyle(.roundedBorder)
                    .disabled(viewModel.isImporting)
            }

            // MARK: Import Button
            Button {
                Task { await viewModel.importURL() }
            } label: {
                HStack {
                    if viewModel.isImporting {
                        ProgressView()
                            .tint(.white)
                    }
                    Text("Import from URL")
                        .fontWeight(.medium)
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .disabled(viewModel.urlInput.isEmpty || viewModel.isImporting)
        }
    }

    // MARK: - Import Status Section

    /// Displays the current import status: progress, success, or error.
    @ViewBuilder
    private var importStatusSection: some View {
        if viewModel.isImporting {
            // MARK: Importing Progress
            VStack(spacing: 12) {
                ProgressView()
                    .controlSize(.large)

                Text("Importing...")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 24)
            .background(
                RoundedRectangle(cornerRadius: 12)
                    .fill(Color(.secondarySystemGroupedBackground))
            )

        } else if let importedItem = viewModel.importedItem {
            // MARK: Success State
            VStack(spacing: 12) {
                Image(systemName: "checkmark.circle.fill")
                    .font(.system(size: 36))
                    .foregroundStyle(.green)

                Text("Successfully imported!")
                    .font(.headline)
                    .foregroundStyle(.primary)

                Text(importedItem.title)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .multilineTextAlignment(.center)

                HStack(spacing: 12) {
                    // Open the imported item in the reader
                    Button {
                        navigateToReader = true
                    } label: {
                        Label("Open Book", systemImage: "book.fill")
                            .fontWeight(.medium)
                    }
                    .buttonStyle(.borderedProminent)

                    // Reset the import form for another import
                    Button {
                        viewModel.reset()
                    } label: {
                        Text("Import Another")
                            .fontWeight(.medium)
                    }
                    .buttonStyle(.bordered)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 24)
            .background(
                RoundedRectangle(cornerRadius: 12)
                    .fill(Color(.secondarySystemGroupedBackground))
            )

        } else if let error = viewModel.importError {
            // MARK: Error State
            VStack(spacing: 12) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.system(size: 36))
                    .foregroundStyle(.red)

                Text("Import Failed")
                    .font(.headline)
                    .foregroundStyle(.primary)

                Text(error)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)

                Button {
                    viewModel.reset()
                } label: {
                    Text("Try Again")
                        .fontWeight(.medium)
                }
                .buttonStyle(.bordered)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 24)
            .background(
                RoundedRectangle(cornerRadius: 12)
                    .fill(Color(.secondarySystemGroupedBackground))
            )
        }
    }

    // MARK: - Helpers

    /// Labeled divider that separates the file import and URL import sections.
    private func dividerWithLabel(_ label: String) -> some View {
        HStack {
            Rectangle()
                .fill(Color(.separator))
                .frame(height: 1)

            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
                .layoutPriority(1) // Prevent the text from being compressed

            Rectangle()
                .fill(Color(.separator))
                .frame(height: 1)
        }
    }

    /// Handles the result from the system file picker.
    ///
    /// On success, reads the file data from the URL and starts the import process.
    /// The ViewModel's `importFile(data:filename:)` API expects raw `Data` and a
    /// filename string, so we read the file contents here before passing them along.
    ///
    /// On failure, logs the error (the file picker handles its own error UI).
    private func handleFileImportResult(_ result: Result<[URL], Error>) {
        switch result {
        case .success(let urls):
            guard let fileURL = urls.first else { return }
            // Start accessing the security-scoped resource.
            // The system grants temporary access to the selected file.
            guard fileURL.startAccessingSecurityScopedResource() else {
                viewModel.importError = "Could not access the selected file."
                return
            }

            // Read the file data and extract the filename before releasing access.
            let filename = fileURL.lastPathComponent
            do {
                let data = try Data(contentsOf: fileURL)
                fileURL.stopAccessingSecurityScopedResource()

                Task {
                    await viewModel.importFile(data: data, filename: filename)
                }
            } catch {
                fileURL.stopAccessingSecurityScopedResource()
                viewModel.importError = "Could not read file: \(error.localizedDescription)"
            }

        case .failure(let error):
            // File picker was cancelled or encountered an error.
            // Cancellation is not an error from the user's perspective.
            if !error.localizedDescription.contains("cancel") {
                viewModel.importError = error.localizedDescription
            }
        }
    }
}

// MARK: - FormatBadge
/// A small pill badge displaying a file format name (e.g., "EPUB", "PDF").
/// Used in the file import section to show supported formats.
private struct FormatBadge: View {
    let text: String
    let color: Color

    var body: some View {
        Text(text)
            .font(.system(size: 11, weight: .bold, design: .rounded))
            .foregroundStyle(color)
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(
                Capsule()
                    .fill(color.opacity(0.12))
            )
    }
}

// MARK: - Preview

#Preview {
    NavigationStack {
        ImportView()
    }
}
