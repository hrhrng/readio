import Foundation

// MARK: - EpubChapter
/// A single chapter of an EPUB book, containing structured content blocks.
///
/// EPUB documents are split into chapters by the backend during import. Each
/// chapter's HTML is parsed into an array of `ContentBlock` values that preserve
/// semantic structure (headings, paragraphs, lists, etc.) while enabling
/// sentence-level TTS playback.
///
/// **Content decoding:**
/// The `blocks` field comes from the server as a JSON array embedded within
/// the `LibraryItem.content` field. For EPUB items, `content` is a JSON string
/// containing an array of chapter objects, each with `id`, `title`, and `blocks`.
///
/// **Navigation:**
/// Use `TOCEntry` for table-of-contents navigation. The `EpubChapter.id`
/// corresponds to `TOCEntry.id`, enabling the reader to jump between chapters.
struct EpubChapter: Codable, Identifiable, Sendable {
    /// Unique chapter identifier, typically derived from the EPUB spine item ID
    /// or the chapter's filename within the EPUB archive (e.g., "chapter1.xhtml").
    let id: String

    /// Human-readable chapter title extracted from the EPUB's navigation document
    /// or inferred from the chapter's first heading element.
    let title: String

    /// Structured content blocks parsed from the chapter's HTML.
    /// These represent the semantic elements (headings, paragraphs, images, etc.)
    /// and contain pre-split sentences ready for TTS playback and highlighting.
    let blocks: [ContentBlock]
}

// MARK: - TOCEntry
/// A single entry in the EPUB's table of contents, used for chapter navigation.
///
/// TOC entries are extracted from the EPUB's navigation document (EPUB 3 `nav.xhtml`
/// or EPUB 2 `toc.ncx`). They provide a user-facing chapter list that may differ
/// from the actual spine order (some EPUBs have more TOC entries than spine items,
/// or vice versa).
///
/// **Navigation flow:**
/// 1. Display TOC entries in a sidebar/sheet
/// 2. User taps a TOC entry
/// 3. Look up the matching `EpubChapter` by `id`
/// 4. Scroll to the chapter (or to a specific anchor via `href`)
struct TOCEntry: Codable, Identifiable, Sendable {
    /// Unique identifier for this TOC entry. Matches `EpubChapter.id` for
    /// top-level chapter references.
    let id: String

    /// Display title shown in the table of contents list.
    let title: String

    /// Optional href for deep-linking into a chapter (e.g., "chapter3.xhtml#section-2").
    /// When nil, navigation targets the beginning of the chapter matching `id`.
    let href: String?
}
