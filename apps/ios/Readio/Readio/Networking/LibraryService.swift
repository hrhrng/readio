import Foundation

enum LibraryServiceError: LocalizedError, Equatable {
    case itemNotFound(String)
    case invalidMetadata
    case unsupportedFileType(String)
    case unsupportedCategory(String)
    case invalidURL(String)
    case unsupportedURLScheme(String)
    case nonHTTPResponse
    case httpStatus(Int)
    case noReadableContent

    var errorDescription: String? {
        switch self {
        case .itemNotFound(let id):
            return "Library item not found: \(id)"
        case .invalidMetadata:
            return "The local library metadata is invalid"
        case .unsupportedFileType(let value):
            return "Unsupported local file type: \(value)"
        case .unsupportedCategory(let value):
            return "Unsupported library category: \(value)"
        case .invalidURL(let value):
            return "Invalid URL: \(value)"
        case .unsupportedURLScheme(let value):
            return "Unsupported URL scheme: \(value)"
        case .nonHTTPResponse:
            return "The URL did not return an HTTP response"
        case .httpStatus(let status):
            return "The URL returned HTTP \(status)"
        case .noReadableContent:
            return "Rust could not find readable document content at this URL"
        }
    }
}

struct AppleAcquiredURLResource: Sendable {
    let finalURL: URL
    let data: Data
    let mediaType: String?
}

protocol AppleURLAcquiring: Sendable {
    func acquire(url: URL) async throws -> AppleAcquiredURLResource
}

struct URLSessionAppleURLAcquirer: AppleURLAcquiring {
    let session: URLSession

    init(session: URLSession = .shared) {
        self.session = session
    }

    func acquire(url: URL) async throws -> AppleAcquiredURLResource {
        let (data, response) = try await session.data(from: url)
        guard let response = response as? HTTPURLResponse else {
            throw LibraryServiceError.nonHTTPResponse
        }
        guard (200..<300).contains(response.statusCode) else {
            throw LibraryServiceError.httpStatus(response.statusCode)
        }
        return AppleAcquiredURLResource(
            finalURL: response.url ?? url,
            data: data,
            mediaType: response.mimeType
        )
    }
}

/// Local-first library boundary for SwiftUI and AppKit.
///
/// Rust owns filtering, ordering, pagination, CRDT changes, and materialized
/// metadata. Swift owns file I/O. HTTP remains only as a URL acquisition and
/// parsing provider until that capability moves to the shared core/Worker.
final class LibraryService: Sendable {
    private static let builtinSampleItemID = "readio:sample:v1"

    private let localStore: AppleLocalReplicaStore
    private let urlAcquirer: any AppleURLAcquiring
    private let languageTag: String
    private let diagnostics: AppleDiagnosticLog
    private let sdk = ReadioSDK()

    init(
        localStore: AppleLocalReplicaStore = .shared,
        urlAcquirer: any AppleURLAcquiring = URLSessionAppleURLAcquirer(),
        languageTag: String = Locale.current.identifier,
        diagnostics: AppleDiagnosticLog = .shared
    ) {
        self.localStore = localStore
        self.urlAcquirer = urlAcquirer
        self.languageTag = languageTag
        self.diagnostics = diagnostics
    }

    func fetchItems(
        category: String? = nil,
        search: String? = nil,
        folderId: String? = nil,
        type: String? = nil,
        page: Int? = nil,
        pageSize: Int? = nil,
        sortBy: String? = nil,
        sortOrder: String? = nil,
        progressMin: Int? = nil,
        progressMax: Int? = nil
    ) async throws -> LibraryItemsResponse {
        let query = LocalLibraryQuery(
            category: category,
            search: search,
            folderId: folderId,
            type: type,
            page: page,
            pageSize: pageSize,
            sortBy: sortBy,
            sortOrder: normalizedSortOrder(sortOrder),
            progressMin: progressMin,
            progressMax: progressMax
        )
        let queryJSON = String(decoding: try JSONEncoder().encode(query), as: UTF8.self)
        let responseJSON = try await localStore.queryLibrary(queryJSON: queryJSON)
        return try JSONDecoder().decode(
            LibraryItemsResponse.self,
            from: Data(responseJSON.utf8)
        )
    }

    func fetchItem(id: String) async throws -> LibraryItem {
        if id == Self.builtinSampleItemID {
            let sample = try sdk.builtinSample(languageTag: languageTag)
            return LibraryItem(
                id: sample.itemID,
                legacyItemIds: sample.legacyItemIDs,
                locale: sample.locale,
                title: sample.title,
                author: sample.author,
                content: sample.content,
                type: .txt,
                progress: 0,
                date: "",
                category: .imported,
                folderId: "",
                source: sample.source,
                filePath: nil,
                coverImage: nil,
                voice: nil,
                speed: nil,
                currentChapter: nil,
                contentFormat: sample.contentFormat,
                markdownSource: sample.markdownSource,
                isBuiltinSample: sample.isBuiltinSample,
                importRecord: nil
            )
        }

        let metadataJSON = try await localStore.libraryItemJSON(itemID: id)
        guard var metadata = try JSONSerialization.jsonObject(
            with: Data(metadataJSON.utf8)
        ) as? [String: Any] else {
            throw LibraryServiceError.itemNotFound(id)
        }
        metadata["content"] = try await localStore.libraryContent(itemID: id)?.content ?? ""
        return try JSONDecoder().decode(
            LibraryItem.self,
            from: JSONSerialization.data(withJSONObject: metadata)
        )
    }

    func loadEpubAsset(itemID: String, assetID: String) async throws -> Data {
        guard let archive = try await localStore.libraryContent(itemID: itemID)?.fileData else {
            throw LibraryServiceError.itemNotFound(itemID)
        }
        return try sdk.extractEPUBAsset(archive, assetID: assetID)
    }

    /// Produces the sole reader value consumed by SwiftUI/AppKit. Persisted
    /// pre-Reader-IR imports are reparsed from their owned source bytes here,
    /// keeping compatibility work out of the GUI and out of playback.
    func loadReaderProjection(for item: LibraryItem) async throws -> AppleReaderProjection {
        switch item.type {
        case .txt:
            if let markdown = item.markdownSource
                ?? (item.contentFormat == "markdown" ? item.content : nil) {
                return try sdk.parseMarkdownReader(markdown).projectedForApple()
            }
            return try sdk.parsePlainTextReader(item.content).projectedForApple()
        case .web:
            if item.contentFormat == "readio-reader-ir-v1",
               let reader = try? JSONDecoder().decode(
                   ReadioReaderIR.self,
                   from: Data(item.content.utf8)
               ) {
                return reader.projectedForApple()
            }
            if let source = try await localStore.libraryContent(itemID: item.id)?.fileData {
                return try sdk.parseHTMLReader(
                    String(decoding: source, as: UTF8.self),
                    optionsJSON: readerHTMLOptions(
                        byteCount: source.count,
                        baseURL: item.source
                    )
                ).projectedForApple()
            }
            return try sdk.parsePlainTextReader(item.content).projectedForApple()
        case .pdf:
            if let publication = try? RustPDFPublication.decode(item.content) {
                return publication.projectedForApple()
            }
            guard let source = try await localStore.libraryContent(itemID: item.id)?.fileData else {
                throw LibraryServiceError.noReadableContent
            }
            return try RustPDFPublication.decode(sdk.parsePDF(source)).projectedForApple()
        case .epub:
            if let publication = try? RustEPUBPublication.decode(item.content) {
                return publication.projectedForApple()
            }
            guard let source = try await localStore.libraryContent(itemID: item.id)?.fileData else {
                throw LibraryServiceError.noReadableContent
            }
            return try RustEPUBPublication.decode(sdk.parseEPUB(source)).projectedForApple()
        }
    }

    func deleteItem(id: String) async throws {
        let metadataJSON = try await localStore.libraryItemJSON(itemID: id)
        let metadata = try JSONSerialization.jsonObject(
            with: Data(metadataJSON.utf8),
            options: .fragmentsAllowed
        ) as? [String: Any]
        let importRecord: ReadioImportRecord?
        if let rawRecord = metadata?["import_record"],
           JSONSerialization.isValidJSONObject(rawRecord) {
            importRecord = try JSONDecoder().decode(
                ReadioImportRecord.self,
                from: JSONSerialization.data(withJSONObject: rawRecord)
            )
        } else {
            importRecord = nil
        }
        let shouldDeleteContent = try importRecord.map { record in
            try sdk.planForget(record).contains { $0.type == .deleteOwnedContent }
        } ?? true
        try await localStore.deleteLibraryItem(
            itemID: id,
            deleteContent: shouldDeleteContent
        )
    }

    func updateProgress(id: String, progress: Int) async throws {
        try await patchItem(id: id, fields: ["progress": progress])
    }

    func updateVoice(id: String, voice: String?) async throws {
        try await patchItem(id: id, fields: ["voice": voice ?? NSNull()])
    }

    func updateSpeed(id: String, speed: Double?) async throws {
        try await patchItem(id: id, fields: ["speed": speed ?? NSNull()])
    }

    func updateChapter(id: String, chapter: String?) async throws {
        try await patchItem(id: id, fields: ["current_chapter": chapter ?? NSNull()])
    }

    func importFile(
        data: Data,
        filename: String,
        folderId: String = "f1",
        category: String = "imported",
        title: String? = nil
    ) async throws -> LibraryItem {
        let operationID = diagnostics.beginOperation()
        let itemType = try itemType(filename: filename)
        guard let itemCategory = LibraryItemCategory(rawValue: category) else {
            throw LibraryServiceError.unsupportedCategory(category)
        }
        let identity = try sdk.contentIdentity(data)
        let id = identity.digest
        let storageKey = "local:\(id)"
        let existingMetadataJSON = try await localStore.libraryItemJSON(itemID: id)
        let existingMetadata = try JSONSerialization.jsonObject(
            with: Data(existingMetadataJSON.utf8),
            options: .fragmentsAllowed
        ) as? [String: Any]
        let existingRecord: ReadioImportRecord?
        if let rawRecord = existingMetadata?["import_record"],
           JSONSerialization.isValidJSONObject(rawRecord) {
            existingRecord = try JSONDecoder().decode(
                ReadioImportRecord.self,
                from: JSONSerialization.data(withJSONObject: rawRecord)
            )
        } else if existingMetadata != nil {
            existingRecord = ReadioImportRecord(
                itemId: id,
                contentIdentity: identity,
                ownership: .copy,
                sourceKey: existingMetadata?["source"] as? String ?? filename,
                storageKey: existingMetadata?["file_path"] as? String ?? storageKey
            )
        } else {
            existingRecord = nil
        }
        let plan = try sdk.planImport(
            ReadioImportRequest(
                contentIdentity: identity,
                ownership: .copy,
                sourceKey: "apple-file:\(filename)",
                ownedStorageKey: storageKey,
                existing: existingRecord
            )
        )
        if plan.outcome == .reused {
            diagnostics.record(
                operationID: operationID,
                component: "apple-library",
                category: .import,
                name: "import_completed",
                fields: [
                    "source_kind": .string("file"),
                    "document_format": .string(documentFormat(itemType, filename: filename)),
                    "byte_count": .number(Double(data.count)),
                    "duplicate": .boolean(true),
                    "outcome": .string("reused"),
                ]
            )
            return try await fetchItem(id: plan.record.itemId)
        }
        let content: String
        var inferredTitle: String?
        var inferredAuthor: String?
        var inferredCoverImage: String?
        switch itemType {
        case .txt:
            content = String(decoding: data, as: UTF8.self)
            let reader = try isMarkdownFilename(filename)
                ? sdk.parseMarkdownReader(content)
                : sdk.parsePlainTextReader(content)
            recordParsingCompleted(
                operationID: operationID,
                format: documentFormat(itemType, filename: filename),
                byteCount: data.count,
                reader: reader
            )
        case .pdf:
            let parsedJSON = try sdk.parsePDF(data)
            let publication = try RustPDFPublication.decode(parsedJSON)
            guard !publication.canonicalText.isEmpty,
                  !publication.projectedForApple().chapters.isEmpty else {
                throw LibraryServiceError.noReadableContent
            }
            content = parsedJSON
            inferredTitle = publication.title
            inferredAuthor = publication.author
            recordParsingCompleted(
                operationID: operationID,
                format: "pdf",
                byteCount: data.count,
                reader: publication.reader
            )
        case .epub:
            let publicationJSON = try sdk.parseEPUB(data)
            let publication = try RustEPUBPublication.decode(publicationJSON)
            guard !publication.projectedForApple().chapters.isEmpty else {
                throw LibraryServiceError.noReadableContent
            }
            content = publicationJSON
            inferredTitle = publication.title
            inferredAuthor = publication.author
            inferredCoverImage = publication.coverAssetID.map {
                "readio-epub-asset:\($0)"
            }
            recordParsingCompleted(
                operationID: operationID,
                format: "epub",
                byteCount: data.count,
                reader: publication.reader
            )
        case .web:
            content = ""
        }
        let trimmedTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines)
        let explicitTitle = trimmedTitle.flatMap { value in
            value.isEmpty ? nil : value
        }
        let item = LibraryItem(
            id: id,
            title: normalizedTitle(explicitTitle ?? inferredTitle, fallbackFilename: filename),
            author: inferredAuthor,
            content: content,
            type: itemType,
            progress: 0,
            date: ISO8601DateFormatter().string(from: Date()),
            category: itemCategory,
            folderId: folderId,
            source: filename,
            filePath: "local:\(id)",
            coverImage: inferredCoverImage,
            voice: nil,
            speed: nil,
            currentChapter: nil,
            contentFormat: itemType == .pdf
                ? "readio-pdf-v1"
                : (itemType == .epub
                    ? "readio-epub-v1"
                    : (itemType == .txt
                        ? (isMarkdownFilename(filename) ? "markdown" : "plaintext")
                        : nil)),
            importRecord: plan.record
        )
        try await persist(item, fileData: data, mediaType: mediaType(itemType), fileName: filename)
        diagnostics.record(
            operationID: operationID,
            component: "apple-library",
            category: .import,
            name: "import_completed",
            fields: [
                "source_kind": .string("file"),
                "document_format": .string(documentFormat(itemType, filename: filename)),
                "byte_count": .number(Double(data.count)),
                "duplicate": .boolean(false),
                "outcome": .string(plan.outcome.rawValue),
            ]
        )
        return item
    }

    /// Swift performs only URL acquisition. Hostile markup filtering, budgets,
    /// semantic blocks, and canonical text all come from the shared Rust parser.
    func importURL(
        url: String,
        folderId: String = "f1",
        category: String = "imported",
        title: String? = nil
    ) async throws -> LibraryItem {
        let operationID = diagnostics.beginOperation()
        guard let requestedURL = URL(string: url), requestedURL.host != nil else {
            throw LibraryServiceError.invalidURL(url)
        }
        guard ["http", "https"].contains(requestedURL.scheme?.lowercased() ?? "") else {
            throw LibraryServiceError.unsupportedURLScheme(requestedURL.scheme ?? "")
        }
        guard let itemCategory = LibraryItemCategory(rawValue: category) else {
            throw LibraryServiceError.unsupportedCategory(category)
        }

        let resource = try await urlAcquirer.acquire(url: requestedURL)
        let html = String(decoding: resource.data, as: UTF8.self)
        let options = [
            "max_input_bytes": resource.data.count,
            "max_blocks": 100_000,
            "base_url": resource.finalURL.absoluteString,
        ] as [String: Any]
        let optionsJSON = String(
            decoding: try JSONSerialization.data(withJSONObject: options),
            as: UTF8.self
        )
        let reader = try sdk.parseHTMLReader(html, optionsJSON: optionsJSON)
        recordParsingCompleted(
            operationID: operationID,
            format: "html",
            byteCount: resource.data.count,
            reader: reader
        )
        guard !reader.textIndex.sentences.isEmpty else {
            throw LibraryServiceError.noReadableContent
        }
        let identity = try sdk.contentIdentity(Data(reader.document.canonicalText.utf8))
        let id = identity.digest
        let storageKey = "local:\(id)"
        let existingMetadataJSON = try await localStore.libraryItemJSON(itemID: id)
        let existingMetadata = try JSONSerialization.jsonObject(
            with: Data(existingMetadataJSON.utf8),
            options: .fragmentsAllowed
        ) as? [String: Any]
        let existingRecord: ReadioImportRecord?
        if let rawRecord = existingMetadata?["import_record"],
           JSONSerialization.isValidJSONObject(rawRecord) {
            existingRecord = try JSONDecoder().decode(
                ReadioImportRecord.self,
                from: JSONSerialization.data(withJSONObject: rawRecord)
            )
        } else if existingMetadata != nil {
            existingRecord = ReadioImportRecord(
                itemId: id,
                contentIdentity: identity,
                ownership: .copy,
                sourceKey: existingMetadata?["source"] as? String
                    ?? resource.finalURL.absoluteString,
                storageKey: existingMetadata?["file_path"] as? String ?? storageKey
            )
        } else {
            existingRecord = nil
        }
        let plan = try sdk.planImport(
            ReadioImportRequest(
                contentIdentity: identity,
                ownership: .copy,
                sourceKey: resource.finalURL.absoluteString,
                ownedStorageKey: storageKey,
                existing: existingRecord
            )
        )
        if plan.outcome == .reused {
            diagnostics.record(
                operationID: operationID,
                component: "apple-library",
                category: .import,
                name: "import_completed",
                fields: [
                    "source_kind": .string("url"),
                    "document_format": .string("html"),
                    "byte_count": .number(Double(resource.data.count)),
                    "duplicate": .boolean(true),
                    "outcome": .string("reused"),
                ]
            )
            return try await fetchItem(id: plan.record.itemId)
        }
        let explicitTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines)
        let resolvedTitle = explicitTitle.flatMap { $0.isEmpty ? nil : $0 }
            ?? reader.document.blocks.first(where: { $0.kind.type == .heading })?.text
            ?? resource.finalURL.host
            ?? "Untitled"
        let readerJSON = String(
            decoding: try JSONEncoder().encode(reader),
            as: UTF8.self
        )
        let item = LibraryItem(
            id: plan.record.itemId,
            title: resolvedTitle,
            content: readerJSON,
            type: .web,
            progress: 0,
            date: ISO8601DateFormatter().string(from: Date()),
            category: itemCategory,
            folderId: folderId,
            source: resource.finalURL.absoluteString,
            filePath: "local:\(id)",
            coverImage: nil,
            voice: nil,
            speed: nil,
            currentChapter: nil,
            contentFormat: "readio-reader-ir-v1",
            importRecord: plan.record
        )
        try await persist(
            item,
            fileData: resource.data,
            mediaType: resource.mediaType ?? "text/html",
            fileName: nil
        )
        diagnostics.record(
            operationID: operationID,
            component: "apple-library",
            category: .import,
            name: "import_completed",
            fields: [
                "source_kind": .string("url"),
                "document_format": .string("html"),
                "byte_count": .number(Double(resource.data.count)),
                "duplicate": .boolean(false),
                "outcome": .string(plan.outcome.rawValue),
            ]
        )
        return item
    }

    private func patchItem(id: String, fields: [String: Any]) async throws {
        let metadataJSON = try await localStore.libraryItemJSON(itemID: id)
        guard var metadata = try JSONSerialization.jsonObject(
            with: Data(metadataJSON.utf8)
        ) as? [String: Any] else {
            throw LibraryServiceError.itemNotFound(id)
        }
        for (key, value) in fields {
            metadata[key] = value
        }
        let entity = [[
            "entityKind": "library_item",
            "entityId": id,
            "payload": metadata,
        ] as [String: Any]]
        let entitiesJSON = String(
            decoding: try JSONSerialization.data(withJSONObject: entity),
            as: UTF8.self
        )
        try await localStore.stageLocal(entitiesJSON: entitiesJSON)
    }

    private func persist(
        _ item: LibraryItem,
        fileData: Data?,
        mediaType: String?,
        fileName: String?
    ) async throws {
        var metadata = try JSONSerialization.jsonObject(
            with: JSONEncoder().encode(item)
        ) as? [String: Any]
        guard metadata != nil else {
            throw LibraryServiceError.invalidMetadata
        }
        metadata?.removeValue(forKey: "content")
        let metadataJSON = String(
            decoding: try JSONSerialization.data(withJSONObject: metadata!),
            as: UTF8.self
        )
        try await localStore.saveLibraryItem(
            metadataJSON: metadataJSON,
            content: AppleLibraryContentRecord(
                itemId: item.id,
                content: item.content,
                fileData: fileData,
                mediaType: mediaType,
                fileName: fileName
            )
        )
    }

    private func normalizedSortOrder(_ value: String?) -> String? {
        switch value {
        case "asc", "ascending": return "ascending"
        case "desc", "descending": return "descending"
        default: return value
        }
    }

    private func itemType(filename: String) throws -> LibraryItemType {
        switch URL(fileURLWithPath: filename).pathExtension.lowercased() {
        case "txt", "md", "markdown": return .txt
        case "pdf": return .pdf
        case "epub": return .epub
        case let value: throw LibraryServiceError.unsupportedFileType(value)
        }
    }

    private func isMarkdownFilename(_ filename: String) -> Bool {
        ["md", "markdown"].contains(
            URL(fileURLWithPath: filename).pathExtension.lowercased()
        )
    }

    private func documentFormat(_ type: LibraryItemType, filename: String) -> String {
        guard type == .txt else { return type.rawValue }
        return isMarkdownFilename(filename) ? "markdown" : "plain_text"
    }

    private func recordParsingCompleted(
        operationID: String,
        format: String,
        byteCount: Int,
        reader: ReadioReaderIR
    ) {
        diagnostics.record(
            operationID: operationID,
            component: "apple-reader-parser",
            category: .parsing,
            name: "parse_completed",
            fields: [
                "document_format": .string(format),
                "byte_count": .number(Double(byteCount)),
                "block_count": .number(Double(reader.document.blocks.count)),
                "chapter_count": .number(Double(reader.chapters.count)),
                "outcome": .string("completed"),
            ]
        )
    }

    private func normalizedTitle(_ title: String?, fallbackFilename: String) -> String {
        if let title = title?.trimmingCharacters(in: .whitespacesAndNewlines), !title.isEmpty {
            return title
        }
        let fallback = URL(fileURLWithPath: fallbackFilename)
            .deletingPathExtension()
            .lastPathComponent
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return fallback.isEmpty ? "Untitled" : fallback
    }

    private func readerHTMLOptions(byteCount: Int, baseURL: String?) throws -> String {
        var options: [String: Any] = [
            "max_input_bytes": max(1, byteCount),
            "max_blocks": 100_000,
        ]
        if let baseURL, !baseURL.isEmpty {
            options["base_url"] = baseURL
        }
        return String(
            decoding: try JSONSerialization.data(withJSONObject: options),
            as: UTF8.self
        )
    }

    private func mediaType(_ type: LibraryItemType) -> String {
        switch type {
        case .pdf: return "application/pdf"
        case .epub: return "application/epub+zip"
        case .txt, .web: return "text/plain"
        }
    }
}

private struct LocalLibraryQuery: Encodable {
    let category: String?
    let search: String?
    let folderId: String?
    let type: String?
    let page: Int?
    let pageSize: Int?
    let sortBy: String?
    let sortOrder: String?
    let progressMin: Int?
    let progressMax: Int?

    enum CodingKeys: String, CodingKey {
        case category
        case search
        case folderId = "folder_id"
        case type
        case page
        case pageSize = "page_size"
        case sortBy = "sort_by"
        case sortOrder = "sort_order"
        case progressMin = "progress_min"
        case progressMax = "progress_max"
    }
}
