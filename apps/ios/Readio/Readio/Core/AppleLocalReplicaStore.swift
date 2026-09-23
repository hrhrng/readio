import Foundation

struct AppleLibraryContentRecord: Equatable, Sendable {
    let itemId: String
    let content: String
    let fileData: Data?
    let mediaType: String?
    let fileName: String?
}

protocol AppleLibraryContentPersistence: Sendable {
    func load(itemID: String) async throws -> AppleLibraryContentRecord?
    func save(_ record: AppleLibraryContentRecord) async throws
    func delete(itemID: String) async throws
}

enum AppleLibraryContentPersistenceError: Error, Equatable {
    case invalidContentEncoding
}

/// Stores large local-only content separately from the CRDT metadata snapshot.
/// The manifest is written last, so an interrupted first write stays invisible.
actor FileAppleLibraryContentPersistence: AppleLibraryContentPersistence {
    private struct Manifest: Codable {
        let itemId: String
        let mediaType: String?
        let fileName: String?
        let hasFileData: Bool
    }

    private let rootDirectory: URL
    private let fileManager: FileManager

    init(rootDirectory: URL, fileManager: FileManager = .default) {
        self.rootDirectory = rootDirectory
        self.fileManager = fileManager
    }

    func load(itemID: String) async throws -> AppleLibraryContentRecord? {
        let directory = itemDirectory(itemID: itemID)
        let manifestURL = directory.appendingPathComponent("manifest.json")
        guard fileManager.fileExists(atPath: manifestURL.path) else {
            return nil
        }

        let manifest = try JSONDecoder().decode(
            Manifest.self,
            from: Data(contentsOf: manifestURL)
        )
        let contentData = try Data(
            contentsOf: directory.appendingPathComponent("content.txt")
        )
        guard let content = String(data: contentData, encoding: .utf8) else {
            throw AppleLibraryContentPersistenceError.invalidContentEncoding
        }
        let fileData = manifest.hasFileData
            ? try Data(contentsOf: directory.appendingPathComponent("original.bin"))
            : nil
        return AppleLibraryContentRecord(
            itemId: manifest.itemId,
            content: content,
            fileData: fileData,
            mediaType: manifest.mediaType,
            fileName: manifest.fileName
        )
    }

    func save(_ record: AppleLibraryContentRecord) async throws {
        let directory = itemDirectory(itemID: record.itemId)
        try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        try Data(record.content.utf8).write(
            to: directory.appendingPathComponent("content.txt"),
            options: .atomic
        )

        let originalURL = directory.appendingPathComponent("original.bin")
        if let fileData = record.fileData {
            try fileData.write(to: originalURL, options: .atomic)
        } else if fileManager.fileExists(atPath: originalURL.path) {
            try fileManager.removeItem(at: originalURL)
        }

        let manifest = Manifest(
            itemId: record.itemId,
            mediaType: record.mediaType,
            fileName: record.fileName,
            hasFileData: record.fileData != nil
        )
        try JSONEncoder().encode(manifest).write(
            to: directory.appendingPathComponent("manifest.json"),
            options: .atomic
        )
    }

    func delete(itemID: String) async throws {
        let directory = itemDirectory(itemID: itemID)
        guard fileManager.fileExists(atPath: directory.path) else {
            return
        }
        try fileManager.removeItem(at: directory)
    }

    private func itemDirectory(itemID: String) -> URL {
        let encoded = Data(itemID.utf8)
            .base64EncodedString()
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "=", with: "")
        return rootDirectory.appendingPathComponent(encoded.isEmpty ? "_" : encoded)
    }
}

/// Serializes native storage effects around the Rust-owned replica state.
/// The same persistence instance is shared with cloud sync, so local writes and
/// pulled changes always project through one durable state value.
actor AppleLocalReplicaStore {
    static let defaultBaseDirectory = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first?.appendingPathComponent("Readio", isDirectory: true)
            ?? FileManager.default.temporaryDirectory.appendingPathComponent(
                "Readio",
                isDirectory: true
            )

    static let sharedStatePersistence = FileAppleReplicaSyncPersistence(
        fileURL: defaultBaseDirectory.appendingPathComponent("replica.json")
    )

    static let sharedContentPersistence = FileAppleLibraryContentPersistence(
        rootDirectory: defaultBaseDirectory.appendingPathComponent("content", isDirectory: true)
    )

    static let shared = AppleLocalReplicaStore(
        statePersistence: sharedStatePersistence,
        contentPersistence: sharedContentPersistence
    )

    private let statePersistence: any AppleReplicaSyncPersistence
    private let contentPersistence: any AppleLibraryContentPersistence
    private let accessGate: AppleReplicaAccessGate
    private let sdk = ReadioSDK()

    init(
        statePersistence: any AppleReplicaSyncPersistence,
        contentPersistence: any AppleLibraryContentPersistence,
        accessGate: AppleReplicaAccessGate = .shared
    ) {
        self.statePersistence = statePersistence
        self.contentPersistence = contentPersistence
        self.accessGate = accessGate
    }

    func stageLocal(entitiesJSON: String) async throws {
        try await accessGate.withExclusiveAccess { [self] in
            var (record, state) = try await loadState()
            try state.stageLocal(entitiesJSON: entitiesJSON)
            record.stateJSON = state.serializedJSON
            try await statePersistence.save(record)
        }
    }

    func deleteLocal(entitiesJSON: String) async throws {
        try await accessGate.withExclusiveAccess { [self] in
            var (record, state) = try await loadState()
            try state.deleteLocal(entitiesJSON: entitiesJSON)
            record.stateJSON = state.serializedJSON
            try await statePersistence.save(record)
        }
    }

    func queryLibrary(queryJSON: String) async throws -> String {
        try await accessGate.withExclusiveAccess { [self] in
            let (_, state) = try await loadState()
            return try state.queryLibrary(queryJSON: queryJSON)
        }
    }

    func libraryItemJSON(itemID: String) async throws -> String {
        try await accessGate.withExclusiveAccess { [self] in
            let (_, state) = try await loadState()
            return try state.libraryItem(itemID: itemID)
        }
    }

    func settingsJSON() async throws -> String {
        try await accessGate.withExclusiveAccess { [self] in
            let (_, state) = try await loadState()
            return try state.settingsJSON()
        }
    }

    func bookmarksJSON(itemID: String) async throws -> String {
        try await accessGate.withExclusiveAccess { [self] in
            let (_, state) = try await loadState()
            return try state.bookmarksJSON(itemID: itemID)
        }
    }

    func readingPositionJSON(itemID: String) async throws -> String {
        try await accessGate.withExclusiveAccess { [self] in
            let (_, state) = try await loadState()
            return try state.readingPosition(itemID: itemID)
        }
    }

    func saveReadingPosition(itemID: String, mutationJSON: String) async throws -> String {
        try await accessGate.withExclusiveAccess { [self] in
            var (record, state) = try await loadState()
            let previousStateJSON = state.serializedJSON
            let response = try state.saveReadingPosition(
                itemID: itemID,
                mutationJSON: mutationJSON
            )
            if state.serializedJSON != previousStateJSON {
                record.stateJSON = state.serializedJSON
                try await statePersistence.save(record)
            }
            return response
        }
    }

    func libraryContent(itemID: String) async throws -> AppleLibraryContentRecord? {
        try await contentPersistence.load(itemID: itemID)
    }

    func saveLibraryContent(_ record: AppleLibraryContentRecord) async throws {
        try await contentPersistence.save(record)
    }

    func saveLibraryItem(
        metadataJSON: String,
        content: AppleLibraryContentRecord
    ) async throws {
        let payload = try JSONSerialization.jsonObject(with: Data(metadataJSON.utf8))
        let entity = [
            "entityKind": "library_item",
            "entityId": content.itemId,
            "payload": payload,
        ] as [String: Any]
        let entitiesJSON = String(
            decoding: try JSONSerialization.data(withJSONObject: [entity]),
            as: UTF8.self
        )

        try await accessGate.withExclusiveAccess { [self] in
            var (record, state) = try await loadState()
            try state.stageLocal(entitiesJSON: entitiesJSON)
            try await contentPersistence.save(content)
            record.stateJSON = state.serializedJSON
            try await statePersistence.save(record)
        }
    }

    func deleteLibraryItem(itemID: String, deleteContent: Bool = true) async throws {
        let entity = [[
            "entityKind": "library_item",
            "entityId": itemID,
        ]]
        let entitiesJSON = String(
            decoding: try JSONSerialization.data(withJSONObject: entity),
            as: UTF8.self
        )
        try await accessGate.withExclusiveAccess { [self] in
            var (record, state) = try await loadState()
            try state.deleteLocal(entitiesJSON: entitiesJSON)
            record.stateJSON = state.serializedJSON
            try await statePersistence.save(record)
            if deleteContent {
                try await contentPersistence.delete(itemID: itemID)
            }
        }
    }

    private func loadState() async throws -> (AppleReplicaSyncRecord, ReadioSyncState) {
        let record = try await statePersistence.load() ?? .empty
        return (record, sdk.makeSyncState(serializedJSON: record.stateJSON))
    }
}
