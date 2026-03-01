import Foundation

// MARK: - StyledRun
/// A contiguous run of text sharing the same inline formatting.
///
/// Rich sentences are composed of one or more styled runs. For example,
/// the sentence "Click **here** for details" would be represented as three runs:
///   1. StyledRun(text: "Click ", bold: false, italic: false, href: nil)
///   2. StyledRun(text: "here", bold: true, italic: false, href: nil)
///   3. StyledRun(text: " for details", bold: false, italic: false, href: nil)
///
/// This matches the web frontend's `StyledRun` interface used by the
/// sentence-level TTS highlighting system.
struct StyledRun: Codable, Sendable {
    /// The text content of this run.
    let text: String

    /// Whether this run should be rendered in bold.
    let bold: Bool

    /// Whether this run should be rendered in italic.
    let italic: Bool

    /// Optional hyperlink URL. When present, this run is rendered as a tappable link.
    let href: String?

    // MARK: - CodingKeys & Custom Decoding
    // The web frontend omits false booleans from JSON to save bandwidth,
    // so we default `bold` and `italic` to false when absent.
    enum CodingKeys: String, CodingKey {
        case text, bold, italic, href
    }

    init(text: String, bold: Bool = false, italic: Bool = false, href: String? = nil) {
        self.text = text
        self.bold = bold
        self.italic = italic
        self.href = href
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        text = try container.decode(String.self, forKey: .text)
        bold = try container.decodeIfPresent(Bool.self, forKey: .bold) ?? false
        italic = try container.decodeIfPresent(Bool.self, forKey: .italic) ?? false
        href = try container.decodeIfPresent(String.self, forKey: .href)
    }
}

// MARK: - RichSentence
/// A sentence with its global index, plain text (for TTS), and styled runs (for rendering).
///
/// This is the EPUB-aware counterpart to the simpler `Sentence` struct. While
/// `Sentence` carries only plain text for TTS operations, `RichSentence` preserves
/// inline formatting so the reader view can render bold, italic, and hyperlinks
/// while still supporting sentence-level TTS highlighting.
///
/// The `index` is globally unique within a chapter, enabling the TTS player to
/// track which sentence is currently being spoken and scroll to it.
struct RichSentence: Codable, Identifiable, Sendable {
    /// Zero-based global index of this sentence within the chapter.
    let index: Int

    /// Plain text content (no formatting), used for TTS synthesis.
    let text: String

    /// Styled runs preserving inline formatting for rich rendering.
    let runs: [StyledRun]

    /// Conformance to `Identifiable` — the index is unique within a chapter.
    var id: Int { index }
}

// MARK: - ListItem
/// A single item in a list block, containing its own array of rich sentences.
///
/// Separated into its own struct because list items are independently
/// highlightable during TTS playback — each <li> maps to one or more sentences.
struct ListItem: Codable, Sendable {
    let sentences: [RichSentence]
}

// MARK: - ContentBlock
/// Represents a single structural block of content parsed from an EPUB chapter.
///
/// The EPUB reader on the web frontend parses chapter HTML into an array of
/// `ContentBlock` values. Each block type corresponds to a semantic HTML element:
///
/// | Block Type  | HTML Source          | Contains Sentences? |
/// |-------------|---------------------|---------------------|
/// | heading     | <h1>–<h6>           | Yes                 |
/// | paragraph   | <p>                 | Yes                 |
/// | image       | <img>               | No                  |
/// | list        | <ul>/<ol> with <li> | Yes (per item)      |
/// | blockquote  | <blockquote>        | Yes                 |
/// | code        | <pre><code>         | No                  |
/// | separator   | <hr>                | No                  |
///
/// Blocks that contain sentences support TTS playback and word-level
/// highlighting. Image, code, and separator blocks are skipped during TTS.
///
/// **Decoding:** The JSON uses a `type` discriminator field. Custom `Decodable`
/// conformance reads the type first, then decodes type-specific associated values.
enum ContentBlock: Sendable {
    /// A heading element (h1–h6). `level` 1 is the largest.
    /// `anchorId` is used for TOC navigation (scroll-to-heading).
    case heading(level: Int, sentences: [RichSentence], anchorId: String?)

    /// A paragraph of text with rich sentence-level formatting.
    case paragraph(sentences: [RichSentence], anchorId: String?)

    /// An inline image. `src` may be a relative path (within the EPUB) or a data URL.
    case image(src: String, alt: String?)

    /// An ordered or unordered list. Each item has its own sentence array
    /// so TTS can highlight individual list items during playback.
    case list(ordered: Bool, items: [ListItem], anchorId: String?)

    /// A blockquote with rich sentence content, typically rendered with a
    /// left border and italic styling.
    case blockquote(sentences: [RichSentence], anchorId: String?)

    /// A code block (preformatted text). Not included in TTS playback.
    /// `language` is an optional syntax highlighting hint (e.g., "swift", "python").
    case code(text: String, language: String?)

    /// A horizontal rule / thematic break. Rendered as a divider line.
    case separator
}

// MARK: - ContentBlock + Codable
extension ContentBlock: Codable {
    /// Discriminator key used in the JSON representation to identify block type.
    private enum BlockType: String, Codable {
        case heading, paragraph, image, list, blockquote, code, separator
    }

    /// All possible keys that may appear in a content block's JSON object.
    private enum CodingKeys: String, CodingKey {
        case type
        case level
        case sentences
        case anchorId
        case src
        case alt
        case ordered
        case items
        case text
        case language
    }

    // MARK: Decoding
    // Reads the `type` discriminator first, then decodes the associated fields
    // specific to that block type. Unknown types fall back to `.separator`.
    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let blockType = try container.decode(BlockType.self, forKey: .type)

        switch blockType {
        case .heading:
            let level = try container.decode(Int.self, forKey: .level)
            let sentences = try container.decode([RichSentence].self, forKey: .sentences)
            let anchorId = try container.decodeIfPresent(String.self, forKey: .anchorId)
            self = .heading(level: level, sentences: sentences, anchorId: anchorId)

        case .paragraph:
            let sentences = try container.decode([RichSentence].self, forKey: .sentences)
            let anchorId = try container.decodeIfPresent(String.self, forKey: .anchorId)
            self = .paragraph(sentences: sentences, anchorId: anchorId)

        case .image:
            let src = try container.decode(String.self, forKey: .src)
            let alt = try container.decodeIfPresent(String.self, forKey: .alt)
            self = .image(src: src, alt: alt)

        case .list:
            let ordered = try container.decode(Bool.self, forKey: .ordered)
            let items = try container.decode([ListItem].self, forKey: .items)
            let anchorId = try container.decodeIfPresent(String.self, forKey: .anchorId)
            self = .list(ordered: ordered, items: items, anchorId: anchorId)

        case .blockquote:
            let sentences = try container.decode([RichSentence].self, forKey: .sentences)
            let anchorId = try container.decodeIfPresent(String.self, forKey: .anchorId)
            self = .blockquote(sentences: sentences, anchorId: anchorId)

        case .code:
            let text = try container.decode(String.self, forKey: .text)
            let language = try container.decodeIfPresent(String.self, forKey: .language)
            self = .code(text: text, language: language)

        case .separator:
            self = .separator
        }
    }

    // MARK: Encoding
    // Writes the `type` discriminator and all associated values for the block.
    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)

        switch self {
        case .heading(let level, let sentences, let anchorId):
            try container.encode(BlockType.heading, forKey: .type)
            try container.encode(level, forKey: .level)
            try container.encode(sentences, forKey: .sentences)
            try container.encodeIfPresent(anchorId, forKey: .anchorId)

        case .paragraph(let sentences, let anchorId):
            try container.encode(BlockType.paragraph, forKey: .type)
            try container.encode(sentences, forKey: .sentences)
            try container.encodeIfPresent(anchorId, forKey: .anchorId)

        case .image(let src, let alt):
            try container.encode(BlockType.image, forKey: .type)
            try container.encode(src, forKey: .src)
            try container.encodeIfPresent(alt, forKey: .alt)

        case .list(let ordered, let items, let anchorId):
            try container.encode(BlockType.list, forKey: .type)
            try container.encode(ordered, forKey: .ordered)
            try container.encode(items, forKey: .items)
            try container.encodeIfPresent(anchorId, forKey: .anchorId)

        case .blockquote(let sentences, let anchorId):
            try container.encode(BlockType.blockquote, forKey: .type)
            try container.encode(sentences, forKey: .sentences)
            try container.encodeIfPresent(anchorId, forKey: .anchorId)

        case .code(let text, let language):
            try container.encode(BlockType.code, forKey: .type)
            try container.encode(text, forKey: .text)
            try container.encodeIfPresent(language, forKey: .language)

        case .separator:
            try container.encode(BlockType.separator, forKey: .type)
        }
    }
}
