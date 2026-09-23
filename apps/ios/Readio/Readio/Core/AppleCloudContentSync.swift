import CryptoKit
import Foundation

enum AppleCloudContentVariant: String, Codable, Hashable, Sendable {
    case reader
    case source
}

struct AppleCloudContentMetadata: Codable, Equatable, Sendable {
    let itemID: String
    let variant: AppleCloudContentVariant
    let byteLength: Int
    let contentType: String
    let sha256Base64: String
    let etag: String

    enum CodingKeys: String, CodingKey {
        case itemID = "itemId"
        case variant
        case byteLength
        case contentType
        case sha256Base64
        case etag
    }
}

struct AppleCloudContentDownload: Equatable, Sendable {
    let metadata: AppleCloudContentMetadata
    let data: Data
}

protocol AppleCloudContentTransport: Sendable {
    func head(
        itemID: String,
        variant: AppleCloudContentVariant
    ) async throws -> AppleCloudContentMetadata?

    func upload(
        itemID: String,
        variant: AppleCloudContentVariant,
        data: Data,
        contentType: String,
        sha256Base64: String
    ) async throws -> AppleCloudContentMetadata

    func download(
        itemID: String,
        variant: AppleCloudContentVariant
    ) async throws -> AppleCloudContentDownload?
}

protocol AppleCloudContentSyncExecuting: Sendable {
    func sync(viewJSON: String, session: AppleCloudSession) async throws
}

struct AppleCloudContentSyncExecutor: AppleCloudContentSyncExecuting {
    let contentPersistence: any AppleLibraryContentPersistence

    func sync(viewJSON: String, session: AppleCloudSession) async throws {
        try await AppleLibraryContentCloudReconciler(
            contentPersistence: contentPersistence,
            transport: URLSessionAppleCloudContentTransport(
                baseURL: session.serverURL,
                bearerToken: session.accessToken
            )
        ).reconcile(viewJSON: viewJSON)
    }
}

enum AppleCloudContentError: Error, Equatable {
    case invalidResponse
    case invalidMetadata
    case invalidReaderEncoding
    case httpStatus(Int)
}

struct URLSessionAppleCloudContentTransport: AppleCloudContentTransport {
    let baseURL: URL
    let bearerToken: String
    let session: URLSession

    init(
        baseURL: URL,
        bearerToken: String,
        session: URLSession = .shared
    ) {
        self.baseURL = baseURL
        self.bearerToken = bearerToken
        self.session = session
    }

    func head(
        itemID: String,
        variant: AppleCloudContentVariant
    ) async throws -> AppleCloudContentMetadata? {
        var request = authorizedRequest(itemID: itemID, variant: variant)
        request.httpMethod = "HEAD"
        let (_, response) = try await session.data(for: request)
        return try metadata(
            response: response,
            itemID: itemID,
            variant: variant,
            allowsMissing: true
        )
    }

    func upload(
        itemID: String,
        variant: AppleCloudContentVariant,
        data: Data,
        contentType: String,
        sha256Base64: String
    ) async throws -> AppleCloudContentMetadata {
        var request = authorizedRequest(itemID: itemID, variant: variant)
        request.httpMethod = "PUT"
        request.setValue(contentType, forHTTPHeaderField: "Content-Type")
        request.setValue(String(data.count), forHTTPHeaderField: "Content-Length")
        request.setValue(sha256Base64, forHTTPHeaderField: "X-Readio-Content-SHA256")
        request.httpBody = data
        let (responseData, response) = try await session.data(for: request)
        guard let response = response as? HTTPURLResponse else {
            throw AppleCloudContentError.invalidResponse
        }
        guard (200..<300).contains(response.statusCode) else {
            throw AppleCloudContentError.httpStatus(response.statusCode)
        }
        do {
            return try JSONDecoder().decode(AppleCloudContentMetadata.self, from: responseData)
        } catch {
            throw AppleCloudContentError.invalidMetadata
        }
    }

    func download(
        itemID: String,
        variant: AppleCloudContentVariant
    ) async throws -> AppleCloudContentDownload? {
        var request = authorizedRequest(itemID: itemID, variant: variant)
        request.httpMethod = "GET"
        let (data, response) = try await session.data(for: request)
        guard let metadata = try metadata(
            response: response,
            itemID: itemID,
            variant: variant,
            allowsMissing: true
        ) else {
            return nil
        }
        guard data.count == metadata.byteLength else {
            throw AppleCloudContentError.invalidMetadata
        }
        return AppleCloudContentDownload(metadata: metadata, data: data)
    }

    private func authorizedRequest(
        itemID: String,
        variant: AppleCloudContentVariant
    ) -> URLRequest {
        let url = baseURL
            .appendingPathComponent("v1")
            .appendingPathComponent("library")
            .appendingPathComponent("content")
            .appendingPathComponent(itemID)
            .appendingPathComponent(variant.rawValue)
        var request = URLRequest(url: url)
        request.setValue("Bearer \(bearerToken)", forHTTPHeaderField: "Authorization")
        return request
    }

    private func metadata(
        response: URLResponse,
        itemID: String,
        variant: AppleCloudContentVariant,
        allowsMissing: Bool
    ) throws -> AppleCloudContentMetadata? {
        guard let response = response as? HTTPURLResponse else {
            throw AppleCloudContentError.invalidResponse
        }
        if allowsMissing, response.statusCode == 404 {
            return nil
        }
        guard (200..<300).contains(response.statusCode) else {
            throw AppleCloudContentError.httpStatus(response.statusCode)
        }
        guard
            let byteLengthValue = response.value(forHTTPHeaderField: "Content-Length"),
            let byteLength = Int(byteLengthValue),
            byteLength > 0,
            let contentType = response.value(forHTTPHeaderField: "Content-Type"),
            let digest = response.value(forHTTPHeaderField: "X-Readio-Content-SHA256"),
            let etag = response.value(forHTTPHeaderField: "ETag")
        else {
            throw AppleCloudContentError.invalidMetadata
        }
        return AppleCloudContentMetadata(
            itemID: itemID,
            variant: variant,
            byteLength: byteLength,
            contentType: contentType,
            sha256Base64: digest,
            etag: etag
        )
    }
}

/// Reconciles immutable library bytes around the Rust-owned metadata projection.
/// Existing local bytes win for the same item ID; a device that only learned
/// metadata through Loro downloads the missing private reader/source variants.
struct AppleLibraryContentCloudReconciler: Sendable {
    let contentPersistence: any AppleLibraryContentPersistence
    let transport: any AppleCloudContentTransport

    func reconcile(viewJSON: String) async throws {
        let view: CloudContentView
        do {
            view = try JSONDecoder().decode(CloudContentView.self, from: Data(viewJSON.utf8))
        } catch {
            throw AppleCloudContentError.invalidMetadata
        }

        for (itemID, item) in view.state.libraryItems.sorted(by: { $0.key < $1.key }) {
            if let local = try await contentPersistence.load(itemID: itemID) {
                try await publish(
                    Data(local.content.utf8),
                    itemID: itemID,
                    variant: .reader,
                    contentType: readerContentType(item.type)
                )
                if let fileData = local.fileData {
                    try await publish(
                        fileData,
                        itemID: itemID,
                        variant: .source,
                        contentType: local.mediaType ?? "application/octet-stream"
                    )
                } else if let source = try await transport.download(
                    itemID: itemID,
                    variant: .source
                ) {
                    try await contentPersistence.save(AppleLibraryContentRecord(
                        itemId: itemID,
                        content: local.content,
                        fileData: source.data,
                        mediaType: source.metadata.contentType,
                        fileName: local.fileName ?? inferredFileName(item.source)
                    ))
                }
                continue
            }

            guard let reader = try await transport.download(
                itemID: itemID,
                variant: .reader
            ), let content = String(data: reader.data, encoding: .utf8) else {
                continue
            }
            let source = try await transport.download(itemID: itemID, variant: .source)
            try await contentPersistence.save(AppleLibraryContentRecord(
                itemId: itemID,
                content: content,
                fileData: source?.data,
                mediaType: source?.metadata.contentType,
                fileName: inferredFileName(item.source)
            ))
        }
    }

    private func publish(
        _ data: Data,
        itemID: String,
        variant: AppleCloudContentVariant,
        contentType: String
    ) async throws {
        guard !data.isEmpty else { return }
        let digest = Data(SHA256.hash(data: data)).base64EncodedString()
        if let remote = try await transport.head(itemID: itemID, variant: variant),
           remote.byteLength == data.count,
           remote.sha256Base64 == digest {
            return
        }
        _ = try await transport.upload(
            itemID: itemID,
            variant: variant,
            data: data,
            contentType: contentType,
            sha256Base64: digest
        )
    }

    private func readerContentType(_ itemType: String?) -> String {
        itemType == "epub"
            ? "application/vnd.readio.epub+json"
            : "text/plain; charset=utf-8"
    }

    private func inferredFileName(_ source: String?) -> String? {
        guard let source, !source.isEmpty else { return nil }
        if let url = URL(string: source), url.scheme != nil {
            return url.lastPathComponent.nilIfEmpty
        }
        return URL(fileURLWithPath: source).lastPathComponent.nilIfEmpty
    }
}

private struct CloudContentView: Decodable {
    let state: ContentState
}

private struct ContentState: Decodable {
    let libraryItems: [String: ContentLibraryItem]

    enum CodingKeys: String, CodingKey {
        case libraryItems = "library_items"
    }
}

private struct ContentLibraryItem: Decodable {
    let type: String?
    let source: String?
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
