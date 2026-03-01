import SwiftUI

// MARK: - TOCView
/// Table of Contents panel/sheet for EPUB chapter navigation.
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │  Table of Contents                   │
/// ├──────────────────────────────────────┤
/// │                                      │
/// │  ● Chapter 1: Introduction           │  ← Current (highlighted)
/// │    Chapter 2: Getting Started        │
/// │    Chapter 3: Core Concepts          │
/// │    Chapter 4: Advanced Topics        │
/// │    Chapter 5: Best Practices         │
/// │    Chapter 6: Conclusion             │
/// │                                      │
/// └──────────────────────────────────────┘
/// ```
///
/// **Presentation:**
/// - **iPad (regular width):** Shown as a persistent side panel to the right of
///   the content area, 280pt wide. The parent `ReaderView` manages this layout.
/// - **iPhone (compact width):** Shown as a `.sheet` with `.presentationDetents`.
///   The parent wraps it in a `NavigationStack` with a title and "Done" button.
///
/// **Behavior:**
/// - Tapping a chapter entry calls `onSelectChapter` with the chapter's ID.
/// - The current chapter is highlighted with the accent color and a leading indicator.
/// - Smooth scroll to the current chapter on appear for long TOCs.
///
/// **Data:**
/// Uses `TOCEntry` objects which contain `id`, `title`, and `href`. The `href` field
/// corresponds to the EPUB's internal navigation target and is used by the backend
/// to resolve chapter boundaries.
struct TOCView: View {

    // MARK: - Properties

    /// The table of contents entries, typically parsed from the EPUB's navigation document.
    let entries: [TOCEntry]

    /// The ID of the chapter currently being read. Used to highlight the active entry.
    let currentChapterId: String?

    /// Callback invoked when the user taps a TOC entry to navigate to that chapter.
    let onSelectChapter: (String) -> Void

    // MARK: - Body

    var body: some View {
        if entries.isEmpty {
            // MARK: Empty State
            emptyTOC
        } else {
            // MARK: TOC List
            ScrollViewReader { proxy in
                List {
                    ForEach(entries) { entry in
                        tocEntryRow(entry)
                    }
                }
                .listStyle(.plain)
                .onAppear {
                    // Scroll to the current chapter when the TOC appears.
                    // This is especially useful for long books where the current
                    // chapter might be far down the list.
                    if let currentId = currentChapterId {
                        // Slight delay to allow the list to render before scrolling
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                            withAnimation(.easeInOut(duration: 0.3)) {
                                proxy.scrollTo(currentId, anchor: .center)
                            }
                        }
                    }
                }
            }
        }
    }

    // MARK: - TOC Entry Row

    /// Renders a single TOC entry row with current chapter highlighting.
    ///
    /// **Active chapter:** Shows accent-colored text with a filled circle indicator
    /// and a subtle background tint.
    ///
    /// **Inactive chapter:** Shows standard primary text with a dimmer circle indicator.
    private func tocEntryRow(_ entry: TOCEntry) -> some View {
        let isCurrent = entry.id == currentChapterId

        return Button {
            onSelectChapter(entry.id)
        } label: {
            HStack(spacing: 12) {
                // Leading indicator — filled circle for current, empty for others
                Image(systemName: isCurrent ? "circle.fill" : "circle")
                    .font(.system(size: 8))
                    .foregroundStyle(isCurrent ? .accentColor : .quaternary)

                // Chapter title
                Text(entry.title)
                    .font(.subheadline)
                    .fontWeight(isCurrent ? .semibold : .regular)
                    .foregroundStyle(isCurrent ? .accentColor : .primary)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)

                Spacer()

                // Chevron indicating navigation
                if isCurrent {
                    Image(systemName: "speaker.wave.2.fill")
                        .font(.caption)
                        .foregroundStyle(.accentColor)
                }
            }
            .padding(.vertical, 4)
            .contentShape(Rectangle()) // Make the entire row tappable
        }
        .buttonStyle(.plain)
        .listRowBackground(
            isCurrent
                ? Color.accentColor.opacity(0.08)
                : Color.clear
        )
        .id(entry.id) // For ScrollViewReader to scroll to the current chapter
    }

    // MARK: - Empty TOC

    /// Shown when the EPUB has no navigation entries.
    /// This can happen with malformed EPUBs or books without a proper TOC structure.
    private var emptyTOC: some View {
        VStack(spacing: 12) {
            Image(systemName: "list.bullet.rectangle")
                .font(.system(size: 32))
                .foregroundStyle(.secondary)

            Text("No Table of Contents")
                .font(.subheadline)
                .fontWeight(.medium)
                .foregroundStyle(.secondary)

            Text("This book does not have a navigation structure.")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding()
    }
}

// MARK: - Note on Types
// The `TOCEntry` type used by this view is defined in Models/EpubChapter.swift.
// It contains: id (String), title (String), href (String?).

// MARK: - Preview

#Preview("With Entries") {
    NavigationStack {
        TOCView(
            entries: [
                TOCEntry(id: "ch-1", title: "Preface", href: "preface.xhtml"),
                TOCEntry(id: "ch-2", title: "Chapter 1: The Journey Begins", href: "ch01.xhtml"),
                TOCEntry(id: "ch-3", title: "Chapter 2: Into the Unknown", href: "ch02.xhtml"),
                TOCEntry(id: "ch-4", title: "Chapter 3: The Dark Forest", href: "ch03.xhtml"),
                TOCEntry(id: "ch-5", title: "Chapter 4: A New Dawn", href: "ch04.xhtml"),
                TOCEntry(id: "ch-6", title: "Epilogue", href: "epilogue.xhtml"),
            ],
            currentChapterId: "ch-3",
            onSelectChapter: { id in print("Selected chapter: \(id)") }
        )
        .navigationTitle("Table of Contents")
    }
}

#Preview("Empty") {
    TOCView(
        entries: [],
        currentChapterId: nil,
        onSelectChapter: { _ in }
    )
}
