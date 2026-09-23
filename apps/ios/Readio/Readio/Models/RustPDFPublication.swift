import Foundation

/// Native envelope for the shared PDF publication. PDF page extraction and
/// chapter decisions remain available in the persisted JSON for diagnostics,
/// while every reader surface projects only the embedded Reader IR.
struct RustPDFPublication: Decodable, Sendable {
    let title: String?
    let author: String?
    let canonicalText: String
    let pageCount: Int
    let reader: ReadioReaderIR

    enum CodingKeys: String, CodingKey {
        case title
        case author
        case canonicalText = "canonical_text"
        case pageCount = "page_count"
        case reader
    }

    static func decode(_ json: String) throws -> RustPDFPublication {
        try JSONDecoder().decode(Self.self, from: Data(json.utf8))
    }

    func projectedForApple() -> AppleReaderProjection {
        reader.projectedForApple()
    }
}
