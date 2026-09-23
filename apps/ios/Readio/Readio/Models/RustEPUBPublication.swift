import Foundation

/// Thin native envelope around Rust's EPUB parser result. Archive structure,
/// semantic blocks, navigation targets, and sentence ranges remain in Reader IR;
/// Apple only supplies the media type needed by its image decoder.
struct RustEPUBPublication: Decodable, Sendable {
    let title: String?
    let author: String?
    let coverAssetID: String?
    let assets: [RustEPUBAsset]
    let reader: ReadioReaderIR

    enum CodingKeys: String, CodingKey {
        case title
        case author
        case coverAssetID = "cover_asset_id"
        case assets
        case reader
    }

    static func decode(_ json: String) throws -> RustEPUBPublication {
        try JSONDecoder().decode(Self.self, from: Data(json.utf8))
    }

    func projectedForApple() -> AppleReaderProjection {
        reader.projectedForApple(
            assetMediaTypes: Dictionary(
                uniqueKeysWithValues: assets.map { ($0.id, $0.mediaType) }
            )
        )
    }
}

struct RustEPUBAsset: Decodable, Sendable {
    let id: String
    let mediaType: String

    enum CodingKeys: String, CodingKey {
        case id
        case mediaType = "media_type"
    }
}
