import Foundation

/// Serializes the complete load/transform/save transaction across local writes
/// and cloud sync. Actor isolation alone is reentrant at network awaits, so the
/// explicit gate prevents a late sync save from replacing a newer local state.
actor AppleReplicaAccessGate {
    static let shared = AppleReplicaAccessGate()

    private var isLocked = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func withExclusiveAccess<Value: Sendable>(
        _ operation: @Sendable () async throws -> Value
    ) async rethrows -> Value {
        await acquire()
        do {
            let value = try await operation()
            release()
            return value
        } catch {
            release()
            throw error
        }
    }

    private func acquire() async {
        guard isLocked else {
            isLocked = true
            return
        }
        await withCheckedContinuation { continuation in
            waiters.append(continuation)
        }
    }

    private func release() {
        guard !waiters.isEmpty else {
            isLocked = false
            return
        }
        waiters.removeFirst().resume()
    }
}

struct AppleReplicaChange: Codable, Equatable, Sendable {
    let changeId: String
    let changeBase64: String
}

struct AppleReplicaStoredChange: Codable, Equatable, Sendable {
    let cursor: UInt64
    let changeId: String
    let deviceId: String
    let changeBase64: String

    var change: AppleReplicaChange {
        AppleReplicaChange(changeId: changeId, changeBase64: changeBase64)
    }
}

struct AppleReplicaPullPage: Codable, Equatable, Sendable {
    let cursor: UInt64
    let hasMore: Bool
    let changes: [AppleReplicaStoredChange]
}

struct AppleReplicaPushRequest: Codable, Equatable, Sendable {
    let deviceId: String
    let ackCursor: UInt64
    let headsBase64: String
    let changes: [AppleReplicaChange]
}

struct AppleReplicaPushResult: Codable, Equatable, Sendable {
    let cursor: UInt64
    let acceptedChangeIds: [String]
}

struct AppleReplicaSyncRecord: Codable, Equatable, Sendable {
    var cursor: UInt64
    var stateJSON: String

    static let empty = AppleReplicaSyncRecord(cursor: 0, stateJSON: "{}")
}

protocol AppleReplicaSyncPersistence: Sendable {
    func load() async throws -> AppleReplicaSyncRecord?
    func save(_ record: AppleReplicaSyncRecord) async throws
}

actor FileAppleReplicaSyncPersistence: AppleReplicaSyncPersistence {
    private let fileURL: URL
    private let fileManager: FileManager

    init(fileURL: URL, fileManager: FileManager = .default) {
        self.fileURL = fileURL
        self.fileManager = fileManager
    }

    func load() async throws -> AppleReplicaSyncRecord? {
        guard fileManager.fileExists(atPath: fileURL.path) else {
            return nil
        }
        return try JSONDecoder().decode(
            AppleReplicaSyncRecord.self,
            from: Data(contentsOf: fileURL)
        )
    }

    func save(_ record: AppleReplicaSyncRecord) async throws {
        try fileManager.createDirectory(
            at: fileURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let data = try JSONEncoder().encode(record)
        try data.write(to: fileURL, options: .atomic)
    }
}

protocol AppleReplicaTransport: Sendable {
    func pull(after cursor: UInt64, limit: Int) async throws -> AppleReplicaPullPage
    func push(_ request: AppleReplicaPushRequest) async throws -> AppleReplicaPushResult
    func uploadSnapshot(_ data: Data, deviceId: String, cursor: UInt64) async throws
}

enum AppleReplicaTransportError: Error, Equatable {
    case invalidEndpoint
    case nonHTTPResponse
    case httpStatus(Int)
}

/// The Cloudflare replica transport exposed to Apple clients through the SDK.
///
/// This type owns only HTTP effects. Replica rules stay in `ReadioSyncState`,
/// so SwiftUI and AppKit never need to understand CRDT changes or outboxes.
struct URLSessionAppleReplicaTransport: AppleReplicaTransport {
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

    func pull(after cursor: UInt64, limit: Int) async throws -> AppleReplicaPullPage {
        var components = URLComponents(
            url: endpoint("v1/replica/pull"),
            resolvingAgainstBaseURL: false
        )
        components?.queryItems = [
            URLQueryItem(name: "cursor", value: String(cursor)),
            URLQueryItem(name: "limit", value: String(limit)),
        ]
        guard let url = components?.url else {
            throw AppleReplicaTransportError.invalidEndpoint
        }

        var request = authorizedRequest(url: url)
        request.httpMethod = "GET"
        return try JSONDecoder().decode(
            AppleReplicaPullPage.self,
            from: await perform(request)
        )
    }

    func push(_ payload: AppleReplicaPushRequest) async throws -> AppleReplicaPushResult {
        var request = authorizedRequest(url: endpoint("v1/replica/push"))
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(payload)
        return try JSONDecoder().decode(
            AppleReplicaPushResult.self,
            from: await perform(request)
        )
    }

    func uploadSnapshot(_ data: Data, deviceId: String, cursor: UInt64) async throws {
        var request = authorizedRequest(url: endpoint("v1/replica/snapshot"))
        request.httpMethod = "PUT"
        request.setValue("application/octet-stream", forHTTPHeaderField: "Content-Type")
        request.setValue(deviceId, forHTTPHeaderField: "X-Readio-Device-Id")
        request.setValue(String(cursor), forHTTPHeaderField: "X-Readio-Replica-Cursor")
        request.httpBody = data
        _ = try await perform(request)
    }

    private func endpoint(_ path: String) -> URL {
        baseURL.appendingPathComponent(path)
    }

    private func authorizedRequest(url: URL) -> URLRequest {
        var request = URLRequest(url: url)
        request.setValue("Bearer \(bearerToken)", forHTTPHeaderField: "Authorization")
        return request
    }

    private func perform(_ request: URLRequest) async throws -> Data {
        let (data, response) = try await session.data(for: request)
        guard let response = response as? HTTPURLResponse else {
            throw AppleReplicaTransportError.nonHTTPResponse
        }
        guard (200..<300).contains(response.statusCode) else {
            throw AppleReplicaTransportError.httpStatus(response.statusCode)
        }
        return data
    }
}

enum AppleReplicaSyncError: Error, Equatable {
    case invalidSDKView
    case invalidSnapshot
    case stalledPull
    case unacknowledgedChanges
}

/// Executes platform effects around the Rust-owned synchronization state.
///
/// URLSession and file persistence are injected. Change staging, import,
/// acknowledgement, snapshots, heads, and materialization remain inside
/// `ReadioSyncState`, which crosses the private C ABI through `ReadioSDK`.
struct AppleReplicaSyncCoordinator: Sendable {
    let deviceId: String
    let persistence: any AppleReplicaSyncPersistence
    let transport: any AppleReplicaTransport
    let accessGate: AppleReplicaAccessGate
    let diagnostics: AppleDiagnosticLog

    private let sdk = ReadioSDK()

    init(
        deviceId: String,
        persistence: any AppleReplicaSyncPersistence,
        transport: any AppleReplicaTransport,
        accessGate: AppleReplicaAccessGate = .shared,
        diagnostics: AppleDiagnosticLog = .shared
    ) {
        self.deviceId = deviceId
        self.persistence = persistence
        self.transport = transport
        self.accessGate = accessGate
        self.diagnostics = diagnostics
    }

    func sync(localEntitiesJSON: String) async throws -> String {
        let operationID = diagnostics.beginOperation()
        return try await accessGate.withExclusiveAccess {
            try await syncExclusively(
                localEntitiesJSON: localEntitiesJSON,
                operationID: operationID
            )
        }
    }

    private func syncExclusively(
        localEntitiesJSON: String,
        operationID: String
    ) async throws -> String {
        var record = try await persistence.load() ?? .empty
        var state = sdk.makeSyncState(serializedJSON: record.stateJSON)

        (state, record) = try await pullAll(
            state: state,
            record: record,
            operationID: operationID
        )

        try state.stageLocal(entitiesJSON: localEntitiesJSON)
        record.stateJSON = state.serializedJSON
        try await persistence.save(record)

        (state, record) = try await pushAll(
            state: state,
            record: record,
            operationID: operationID
        )
        (state, record) = try await pullAll(
            state: state,
            record: record,
            operationID: operationID
        )

        let viewJSON = try state.viewJSON()
        let view = try decodeTransportView(viewJSON)
        guard let snapshot = Data(base64Encoded: view.snapshotBase64), !snapshot.isEmpty else {
            throw AppleReplicaSyncError.invalidSnapshot
        }
        try await transport.uploadSnapshot(snapshot, deviceId: deviceId, cursor: record.cursor)
        diagnostics.record(
            operationID: operationID,
            component: "apple-replica-sync",
            category: .sync,
            name: "snapshot_uploaded",
            fields: [
                "phase": .string("snapshot"),
                "direction": .string("push"),
                "cursor": .number(Double(record.cursor)),
                "outcome": .string("completed"),
            ]
        )
        return viewJSON
    }

    private func pullAll(
        state initialState: ReadioSyncState,
        record initialRecord: AppleReplicaSyncRecord,
        operationID: String
    ) async throws -> (ReadioSyncState, AppleReplicaSyncRecord) {
        var state = initialState
        var record = initialRecord

        while true {
            let page = try await transport.pull(after: record.cursor, limit: 100)
            if page.hasMore && page.cursor == record.cursor {
                throw AppleReplicaSyncError.stalledPull
            }
            let changes = page.changes.map(\.change)
            let changesJSON = String(decoding: try JSONEncoder().encode(changes), as: UTF8.self)
            try state.importRemote(changesJSON: changesJSON)
            record.cursor = page.cursor
            record.stateJSON = state.serializedJSON
            try await persistence.save(record)
            diagnostics.record(
                operationID: operationID,
                component: "apple-replica-sync",
                category: .sync,
                name: "pull_completed",
                fields: [
                    "phase": .string("pull"),
                    "direction": .string("pull"),
                    "change_count": .number(Double(changes.count)),
                    "cursor": .number(Double(page.cursor)),
                    "outcome": .string("completed"),
                ]
            )
            if !page.hasMore {
                return (state, record)
            }
        }
    }

    private func pushAll(
        state initialState: ReadioSyncState,
        record initialRecord: AppleReplicaSyncRecord,
        operationID: String
    ) async throws -> (ReadioSyncState, AppleReplicaSyncRecord) {
        var state = initialState
        var record = initialRecord

        while true {
            let before = try decodeTransportView(state.viewJSON())
            let batch = Array(before.outbox.prefix(100))
            let result = try await transport.push(AppleReplicaPushRequest(
                deviceId: deviceId,
                ackCursor: record.cursor,
                headsBase64: before.headsBase64,
                changes: batch
            ))
            _ = result.cursor // A high-water mark is not an imported cursor.
            try state.acknowledge(changeIDs: result.acceptedChangeIds)
            record.stateJSON = state.serializedJSON
            try await persistence.save(record)
            diagnostics.record(
                operationID: operationID,
                component: "apple-replica-sync",
                category: .sync,
                name: "push_completed",
                fields: [
                    "phase": .string("push"),
                    "direction": .string("push"),
                    "change_count": .number(Double(result.acceptedChangeIds.count)),
                    "cursor": .number(Double(record.cursor)),
                    "outcome": .string("completed"),
                ]
            )

            let after = try decodeTransportView(state.viewJSON())
            if after.outbox.isEmpty {
                return (state, record)
            }
            if after.outbox.count == before.outbox.count {
                throw AppleReplicaSyncError.unacknowledgedChanges
            }
        }
    }

    private func decodeTransportView(_ json: String) throws -> TransportView {
        guard let data = json.data(using: .utf8) else {
            throw AppleReplicaSyncError.invalidSDKView
        }
        do {
            return try JSONDecoder().decode(TransportView.self, from: data)
        } catch {
            throw AppleReplicaSyncError.invalidSDKView
        }
    }
}

private struct TransportView: Decodable {
    let outbox: [AppleReplicaChange]
    let snapshotBase64: String
    let headsBase64: String
}
