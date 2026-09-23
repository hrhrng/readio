import Foundation
import Security

struct AppleCloudSession: Codable, Equatable, Sendable {
    let serverURL: URL
    let accessToken: String
    let deviceID: String
}

struct AppleDeviceAuthorization: Equatable, Sendable {
    let deviceCode: String
    let userCode: String
    let verificationURL: URL
    let verificationURLComplete: URL?
    let expiresInSeconds: UInt64
    let intervalSeconds: UInt64
}

enum AppleDeviceTokenResult: Equatable, Sendable {
    case pending
    case slowDown
    case authorized(accessToken: String)
    case denied
    case expired
}

enum AppleCloudLoginPoll: Equatable, Sendable {
    case pending
    case connected(AppleCloudSession)
    case denied
    case expired
}

enum AppleCloudError: Error, Equatable {
    case authenticationRequired
    case invalidResponse
    case httpStatus(Int)
    case authorizationFailed(String)
    case keychain(OSStatus)
}

protocol AppleCloudSessionPersistence: Sendable {
    func load() async throws -> AppleCloudSession?
    func save(_ session: AppleCloudSession) async throws
    func delete() async throws
}

protocol AppleDeviceAuthorizationTransport: Sendable {
    func requestCode(serverURL: URL) async throws -> AppleDeviceAuthorization
    func exchange(serverURL: URL, deviceCode: String) async throws -> AppleDeviceTokenResult
}

protocol AppleCloudSyncExecuting: Sendable {
    func sync(session: AppleCloudSession) async throws -> String
}

protocol AppleReplicaRefreshNotifying: Sendable {
    func didSync() async
}

extension Notification.Name {
    static let readioReplicaDidSync = Notification.Name("readioReplicaDidSync")
}

struct NotificationCenterAppleReplicaRefreshNotifier: AppleReplicaRefreshNotifying {
    func didSync() async {
        NotificationCenter.default.post(name: .readioReplicaDidSync, object: nil)
    }
}

actor KeychainAppleCloudSessionPersistence: AppleCloudSessionPersistence {
    private let service: String
    private let account: String

    init(
        service: String = "com.readio.cloud",
        account: String = "device-session"
    ) {
        self.service = service
        self.account = account
    }

    func load() async throws -> AppleCloudSession? {
        var query = baseQuery
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound {
            return nil
        }
        guard status == errSecSuccess, let data = item as? Data else {
            throw AppleCloudError.keychain(status)
        }
        return try JSONDecoder().decode(AppleCloudSession.self, from: data)
    }

    func save(_ session: AppleCloudSession) async throws {
        let data = try JSONEncoder().encode(session)
        SecItemDelete(baseQuery as CFDictionary)
        var attributes = baseQuery
        attributes[kSecValueData as String] = data
        attributes[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(attributes as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw AppleCloudError.keychain(status)
        }
    }

    func delete() async throws {
        let status = SecItemDelete(baseQuery as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw AppleCloudError.keychain(status)
        }
    }

    private var baseQuery: [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }
}

struct URLSessionAppleDeviceAuthorizationTransport: AppleDeviceAuthorizationTransport {
    private struct DeviceCodeResponse: Decodable {
        let deviceCode: String
        let userCode: String
        let verificationUri: URL
        let verificationUriComplete: URL?
        let expiresIn: UInt64
        let interval: UInt64?

        enum CodingKeys: String, CodingKey {
            case deviceCode = "device_code"
            case userCode = "user_code"
            case verificationUri = "verification_uri"
            case verificationUriComplete = "verification_uri_complete"
            case expiresIn = "expires_in"
            case interval
        }
    }

    private struct TokenResponse: Decodable {
        let accessToken: String

        enum CodingKeys: String, CodingKey {
            case accessToken = "access_token"
        }
    }

    private struct ErrorResponse: Decodable {
        let error: String?
        let code: String?
        let message: String?
    }

    let session: URLSession

    init(session: URLSession = .shared) {
        self.session = session
    }

    func requestCode(serverURL: URL) async throws -> AppleDeviceAuthorization {
        let response: DeviceCodeResponse = try await post(
            serverURL: serverURL,
            path: "api/auth/device/code",
            body: [
                "client_id": "readio-apple",
                "scope": "openid profile email",
            ]
        )
        return AppleDeviceAuthorization(
            deviceCode: response.deviceCode,
            userCode: response.userCode,
            verificationURL: response.verificationUri,
            verificationURLComplete: response.verificationUriComplete,
            expiresInSeconds: response.expiresIn,
            intervalSeconds: max(1, response.interval ?? 5)
        )
    }

    func exchange(serverURL: URL, deviceCode: String) async throws -> AppleDeviceTokenResult {
        let url = serverURL.appendingPathComponent("api/auth/device/token")
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: [
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
            "device_code": deviceCode,
            "client_id": "readio-apple",
        ])
        let (data, response) = try await session.data(for: request)
        guard let response = response as? HTTPURLResponse else {
            throw AppleCloudError.invalidResponse
        }
        if (200..<300).contains(response.statusCode) {
            let token = try JSONDecoder().decode(TokenResponse.self, from: data)
            return .authorized(accessToken: token.accessToken)
        }
        let payload = try? JSONDecoder().decode(ErrorResponse.self, from: data)
        let code = payload?.error ?? payload?.code ?? "http_\(response.statusCode)"
        switch code {
        case "authorization_pending": return .pending
        case "slow_down": return .slowDown
        case "access_denied": return .denied
        case "expired_token": return .expired
        default:
            throw AppleCloudError.authorizationFailed(payload?.message ?? code)
        }
    }

    private func post<Response: Decodable>(
        serverURL: URL,
        path: String,
        body: [String: String]
    ) async throws -> Response {
        var request = URLRequest(url: serverURL.appendingPathComponent(path))
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: body)
        let (data, response) = try await session.data(for: request)
        guard let response = response as? HTTPURLResponse else {
            throw AppleCloudError.invalidResponse
        }
        guard (200..<300).contains(response.statusCode) else {
            throw AppleCloudError.httpStatus(response.statusCode)
        }
        return try JSONDecoder().decode(Response.self, from: data)
    }
}

struct AppleCloudReplicaSyncExecutor: AppleCloudSyncExecuting {
    let persistence: any AppleReplicaSyncPersistence
    let contentSyncExecutor: any AppleCloudContentSyncExecuting
    let makeTransport: @Sendable (AppleCloudSession) -> any AppleReplicaTransport

    init(
        persistence: any AppleReplicaSyncPersistence,
        contentSyncExecutor: any AppleCloudContentSyncExecuting,
        makeTransport: @escaping @Sendable (AppleCloudSession) -> any AppleReplicaTransport = {
            URLSessionAppleReplicaTransport(
                baseURL: $0.serverURL,
                bearerToken: $0.accessToken
            )
        }
    ) {
        self.persistence = persistence
        self.contentSyncExecutor = contentSyncExecutor
        self.makeTransport = makeTransport
    }

    func sync(session: AppleCloudSession) async throws -> String {
        let coordinator = AppleReplicaSyncCoordinator(
            deviceId: session.deviceID,
            persistence: persistence,
            transport: makeTransport(session)
        )
        let viewJSON = try await coordinator.sync(localEntitiesJSON: "[]")
        try await contentSyncExecutor.sync(viewJSON: viewJSON, session: session)
        return viewJSON
    }
}

/// Optional cloud capability. Merely constructing or querying this actor never
/// performs network I/O, so every local feature remains usable while signed out.
actor AppleCloudService {
    static let shared = AppleCloudService(
        authorizationTransport: URLSessionAppleDeviceAuthorizationTransport(),
        sessionPersistence: KeychainAppleCloudSessionPersistence(),
        syncExecutor: AppleCloudReplicaSyncExecutor(
            persistence: AppleLocalReplicaStore.sharedStatePersistence,
            contentSyncExecutor: AppleCloudContentSyncExecutor(
                contentPersistence: AppleLocalReplicaStore.sharedContentPersistence
            )
        )
    )

    private let authorizationTransport: any AppleDeviceAuthorizationTransport
    private let sessionPersistence: any AppleCloudSessionPersistence
    private let syncExecutor: any AppleCloudSyncExecuting
    private let refreshNotifier: any AppleReplicaRefreshNotifying
    private let makeDeviceID: @Sendable () -> String
    private var activeSyncTask: Task<String, Error>?

    init(
        authorizationTransport: any AppleDeviceAuthorizationTransport,
        sessionPersistence: any AppleCloudSessionPersistence,
        syncExecutor: any AppleCloudSyncExecuting,
        refreshNotifier: any AppleReplicaRefreshNotifying = NotificationCenterAppleReplicaRefreshNotifier(),
        makeDeviceID: @escaping @Sendable () -> String = { UUID().uuidString }
    ) {
        self.authorizationTransport = authorizationTransport
        self.sessionPersistence = sessionPersistence
        self.syncExecutor = syncExecutor
        self.refreshNotifier = refreshNotifier
        self.makeDeviceID = makeDeviceID
    }

    func session() async throws -> AppleCloudSession? {
        try await sessionPersistence.load()
    }

    func beginLogin(serverURL: URL) async throws -> AppleDeviceAuthorization {
        try await authorizationTransport.requestCode(serverURL: serverURL)
    }

    func pollLogin(
        serverURL: URL,
        deviceCode: String
    ) async throws -> AppleCloudLoginPoll {
        switch try await authorizationTransport.exchange(
            serverURL: serverURL,
            deviceCode: deviceCode
        ) {
        case .pending, .slowDown:
            return .pending
        case .denied:
            return .denied
        case .expired:
            return .expired
        case .authorized(let accessToken):
            let connected = AppleCloudSession(
                serverURL: serverURL,
                accessToken: accessToken,
                deviceID: makeDeviceID()
            )
            try await sessionPersistence.save(connected)
            return .connected(connected)
        }
    }

    func sync() async throws -> String {
        guard let session = try await sessionPersistence.load() else {
            throw AppleCloudError.authenticationRequired
        }
        return try await synchronize(session: session)
    }

    /// Automatic refresh hook: checking local Keychain state is allowed while
    /// signed out, but no network effect is created until a session exists.
    func syncIfConnected() async throws -> Bool {
        guard let session = try await sessionPersistence.load() else {
            return false
        }
        _ = try await synchronize(session: session)
        return true
    }

    func logout() async throws {
        try await sessionPersistence.delete()
    }

    private func synchronize(session: AppleCloudSession) async throws -> String {
        if let activeSyncTask {
            return try await activeSyncTask.value
        }

        let syncExecutor = syncExecutor
        let refreshNotifier = refreshNotifier
        let task = Task {
            let viewJSON = try await syncExecutor.sync(session: session)
            await refreshNotifier.didSync()
            return viewJSON
        }
        activeSyncTask = task
        do {
            let viewJSON = try await task.value
            activeSyncTask = nil
            return viewJSON
        } catch {
            activeSyncTask = nil
            throw error
        }
    }
}
