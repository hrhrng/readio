import Foundation

struct ReadioSupportBundleFile: Equatable, Sendable {
    let filename: String
    let mediaType = "application/json"
    let data: Data

    init(bundle: ReadioSupportBundle) throws {
        let timestamp = ISO8601DateFormatter()
        timestamp.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let date = Date(
            timeIntervalSince1970: Double(bundle.generatedAtUnixMilliseconds) / 1_000
        )
        let filenameTimestamp = timestamp.string(from: date)
            .replacingOccurrences(of: ":", with: "-")
            .replacingOccurrences(of: ".", with: "-")
        filename = "readio-support-\(filenameTimestamp).json"
        var encoded = try JSONEncoder.readioSupportBundle.encode(bundle)
        encoded.append(0x0A)
        data = encoded
    }
}

/// An ephemeral fact log for support exports. It deliberately accepts fields
/// that may later prove unsafe: the only export path still passes the complete
/// snapshot through Rust Core's strict allowlist before bytes reach the user.
final class AppleDiagnosticLog: @unchecked Sendable {
    static let shared = AppleDiagnosticLog()

    private let capacity: Int
    private let now: @Sendable () -> UInt64
    private let makeOperationID: @Sendable () -> String
    private let lock = NSLock()
    private var events: [ReadioDiagnosticEventInput] = []

    init(
        capacity: Int = 768,
        now: @escaping @Sendable () -> UInt64 = {
            UInt64(Date().timeIntervalSince1970 * 1_000)
        },
        makeOperationID: @escaping @Sendable () -> String = {
            "op-\(UUID().uuidString.lowercased())"
        }
    ) {
        self.capacity = max(1, capacity)
        self.now = now
        self.makeOperationID = makeOperationID
    }

    func beginOperation() -> String {
        makeOperationID()
    }

    func record(
        operationID: String,
        component: String,
        category: ReadioDiagnosticCategory,
        name: String,
        fields: [String: ReadioDiagnosticFieldValue] = [:]
    ) {
        lock.lock()
        defer { lock.unlock() }
        events.append(
            ReadioDiagnosticEventInput(
                timestampUnixMilliseconds: now(),
                operationID: operationID,
                component: component,
                category: category,
                name: name,
                fields: fields
            )
        )
        if events.count > capacity {
            events.removeFirst(events.count - capacity)
        }
    }

    func snapshot() -> [ReadioDiagnosticEventInput] {
        lock.lock()
        defer { lock.unlock() }
        return events
    }

    func clear() {
        lock.lock()
        defer { lock.unlock() }
        events.removeAll(keepingCapacity: true)
    }

    func buildSupportBundle(
        appVersion: String,
        generatedAtUnixMilliseconds: UInt64? = nil
    ) throws -> ReadioSupportBundle {
        try ReadioSDK().buildSupportBundle(
            ReadioSupportBundleRequest(
                generatedAtUnixMilliseconds: generatedAtUnixMilliseconds ?? now(),
                platform: "apple",
                appVersion: appVersion,
                events: snapshot()
            )
        )
    }

    func buildSupportBundleFile(
        appVersion: String,
        generatedAtUnixMilliseconds: UInt64? = nil
    ) throws -> ReadioSupportBundleFile {
        try ReadioSupportBundleFile(
            bundle: buildSupportBundle(
                appVersion: appVersion,
                generatedAtUnixMilliseconds: generatedAtUnixMilliseconds
            )
        )
    }
}
