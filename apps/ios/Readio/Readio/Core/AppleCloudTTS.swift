import AVFoundation
import Foundation

protocol AppleCloudSessionProviding: Sendable {
    func session() async throws -> AppleCloudSession?
}

protocol AppleCloudTTSJobTransport: Sendable {
    func create(
        session: AppleCloudSession,
        text: String,
        voiceID: String,
        speed: Double,
        operationID: String
    ) async throws -> AppleCloudTTSJob

    func get(
        session: AppleCloudSession,
        jobID: String,
        operationID: String
    ) async throws -> AppleCloudTTSJob

    func audio(
        session: AppleCloudSession,
        jobID: String,
        path: String,
        operationID: String
    ) async throws -> Data
}

enum AppleCloudTTSJobStatus: String, Codable, Sendable {
    case queued
    case running
    case completed
    case failed
}

struct AppleCloudTTSJob: Codable, Equatable, Sendable {
    let id: String
    let status: AppleCloudTTSJobStatus
    let createdAtUnixMilliseconds: Int64
    let audioPath: String?
    let errorCode: String?

    enum CodingKeys: String, CodingKey {
        case id
        case status
        case createdAtUnixMilliseconds = "createdAtUnixMs"
        case audioPath = "audioURL"
        case errorCode
    }
}

enum AppleCloudTTSError: Error, Equatable {
    case failed(String)
    case pollingTimedOut
    case invalidAudioPath
}

extension AppleCloudError: ReadioTTSUserPresentableError {
    var ttsUserMessage: String {
        switch self {
        case .authenticationRequired:
            "Sign in to use Cloud TTS, or choose Native in Settings."
        case .httpStatus(401), .httpStatus(403):
            "Your Cloud TTS session expired. Sign in again in Settings."
        default:
            "Cloud TTS is unavailable. Please retry."
        }
    }
}

extension AppleCloudTTSError: ReadioTTSUserPresentableError {
    var ttsUserMessage: String {
        switch self {
        case .failed(let code):
            "Cloud TTS failed (\(code)). Please retry."
        case .pollingTimedOut:
            "Cloud TTS is taking too long. Please retry."
        case .invalidAudioPath:
            "Cloud TTS returned an invalid private audio response."
        }
    }
}

struct URLSessionAppleCloudTTSJobTransport: AppleCloudTTSJobTransport {
    private struct CreateBody: Encodable {
        let text: String
        let voiceId: String
        let speed: Double
    }

    let urlSession: URLSession

    init(urlSession: URLSession = .shared) {
        self.urlSession = urlSession
    }

    func create(
        session: AppleCloudSession,
        text: String,
        voiceID: String,
        speed: Double,
        operationID: String
    ) async throws -> AppleCloudTTSJob {
        var request = authenticatedRequest(
            session: session,
            url: session.serverURL.appendingPathComponent("v1/tts/jobs"),
            operationID: operationID
        )
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(
            CreateBody(text: text, voiceId: voiceID, speed: speed)
        )
        return try await decodedResponse(for: request)
    }

    func get(
        session: AppleCloudSession,
        jobID: String,
        operationID: String
    ) async throws -> AppleCloudTTSJob {
        let url = session.serverURL
            .appendingPathComponent("v1/tts/jobs")
            .appendingPathComponent(jobID)
        return try await decodedResponse(
            for: authenticatedRequest(
                session: session,
                url: url,
                operationID: operationID
            )
        )
    }

    func audio(
        session: AppleCloudSession,
        jobID: String,
        path: String,
        operationID: String
    ) async throws -> Data {
        guard let url = privateURL(path: path, relativeTo: session.serverURL) else {
            throw AppleCloudTTSError.invalidAudioPath
        }
        let (data, response) = try await urlSession.data(
            for: authenticatedRequest(
                session: session,
                url: url,
                operationID: operationID
            )
        )
        try validate(response)
        return data
    }

    private func decodedResponse<Response: Decodable>(
        for request: URLRequest
    ) async throws -> Response {
        let (data, response) = try await urlSession.data(for: request)
        try validate(response)
        return try JSONDecoder().decode(Response.self, from: data)
    }

    private func authenticatedRequest(
        session: AppleCloudSession,
        url: URL,
        operationID: String
    ) -> URLRequest {
        var request = URLRequest(url: url)
        request.setValue("Bearer \(session.accessToken)", forHTTPHeaderField: "Authorization")
        request.setValue("no-store", forHTTPHeaderField: "Cache-Control")
        request.setValue(operationID, forHTTPHeaderField: "x-readio-operation-id")
        return request
    }

    private func validate(_ response: URLResponse) throws {
        guard let response = response as? HTTPURLResponse else {
            throw AppleCloudError.invalidResponse
        }
        guard (200..<300).contains(response.statusCode) else {
            throw AppleCloudError.httpStatus(response.statusCode)
        }
    }

    /// Never send a bearer token to an absolute URL supplied by a job payload.
    /// Cloud audio is private and must stay on the authenticated server origin.
    private func privateURL(path: String, relativeTo serverURL: URL) -> URL? {
        guard var components = URLComponents(url: serverURL, resolvingAgainstBaseURL: false),
              path.hasPrefix("/"),
              !path.hasPrefix("//") else {
            return nil
        }
        components.path = path
        components.query = nil
        components.fragment = nil
        return components.url
    }
}

final class AppleCloudTTSClient: Sendable {
    private let account: any AppleCloudSessionProviding
    private let transport: any AppleCloudTTSJobTransport
    private let pollInterval: Duration
    private let maximumPollCount: Int
    private let diagnostics: AppleDiagnosticLog

    init(
        account: any AppleCloudSessionProviding,
        transport: any AppleCloudTTSJobTransport,
        pollInterval: Duration = .milliseconds(500),
        maximumPollCount: Int = 120,
        diagnostics: AppleDiagnosticLog = .shared
    ) {
        self.account = account
        self.transport = transport
        self.pollInterval = pollInterval
        self.maximumPollCount = max(1, maximumPollCount)
        self.diagnostics = diagnostics
    }

    func synthesize(
        text: String,
        voiceID: String,
        speed: Double
    ) async throws -> Data {
        let operationID = diagnostics.beginOperation()
        let startedAt = Date()
        diagnostics.record(
            operationID: operationID,
            component: "apple-cloud-tts",
            category: .tts,
            name: "job_requested",
            fields: ["provider": .string("cloud")]
        )
        guard let session = try await account.session() else {
            diagnostics.record(
                operationID: operationID,
                component: "apple-cloud-tts",
                category: .tts,
                name: "job_failed",
                fields: [
                    "provider": .string("cloud"),
                    "outcome": .string("failed"),
                    "reason_code": .string("authentication_required"),
                ]
            )
            throw AppleCloudError.authenticationRequired
        }

        var job = try await transport.create(
            session: session,
            text: text,
            voiceID: voiceID,
            speed: speed,
            operationID: operationID
        )
        for poll in 0..<maximumPollCount {
            diagnostics.record(
                operationID: operationID,
                component: "apple-cloud-tts",
                category: .tts,
                name: "job_\(job.status.rawValue)",
                fields: [
                    "provider": .string("cloud"),
                    "job_id": .string(job.id),
                ]
            )
            switch job.status {
            case .completed:
                guard let audioPath = job.audioPath else {
                    throw AppleCloudTTSError.invalidAudioPath
                }
                let audio = try await transport.audio(
                    session: session,
                    jobID: job.id,
                    path: audioPath,
                    operationID: operationID
                )
                diagnostics.record(
                    operationID: operationID,
                    component: "apple-cloud-tts",
                    category: .tts,
                    name: "audio_downloaded",
                    fields: [
                        "provider": .string("cloud"),
                        "job_id": .string(job.id),
                        "duration_ms": .number(
                            Date().timeIntervalSince(startedAt) * 1_000
                        ),
                    ]
                )
                return audio
            case .failed:
                throw AppleCloudTTSError.failed(job.errorCode ?? "tts_failed")
            case .queued, .running:
                guard poll + 1 < maximumPollCount else { break }
                if pollInterval > .zero {
                    try await Task.sleep(for: pollInterval)
                }
                job = try await transport.get(
                    session: session,
                    jobID: job.id,
                    operationID: operationID
                )
            }
        }
        throw AppleCloudTTSError.pollingTimedOut
    }
}

extension AppleCloudService: AppleCloudSessionProviding {}

@MainActor
final class AppleCloudTTSProvider: ReadioAppleTTSProvider {
    nonisolated static let defaultVoiceID = AppleTTSPreferences.defaultCloudVoiceID
    let displayName = "Cloud TTS"
    let diagnosticProviderID = "cloud"

    private let client: AppleCloudTTSClient
    private let voiceID: String
    private var renderTasks: [UUID: Task<Data, Error>] = [:]

    init(
        voiceID: String = AppleCloudTTSProvider.defaultVoiceID,
        client: AppleCloudTTSClient = AppleCloudTTSClient(
            account: AppleCloudService.shared,
            transport: URLSessionAppleCloudTTSJobTransport()
        )
    ) {
        self.voiceID = voiceID
        self.client = client
    }

    func voiceCatalog() -> ReadioNativeVoiceCatalog {
        ReadioNativeVoiceCatalog(
            voices: [
                ReadioNativeVoice(
                    id: voiceID,
                    name: "Cloud expressive narrator",
                    language: "en",
                    gender: nil,
                    quality: .premium,
                    isPersonal: false
                ),
            ],
            defaultVoiceID: voiceID
        )
    }

    func render(_ request: ReadioNativeSpeechRequest) async throws -> ReadioNativeSpeechAudio {
        let taskID = UUID()
        let client = client
        let voiceID = request.voiceID ?? voiceID
        let task = Task {
            try await client.synthesize(
                text: request.text,
                voiceID: voiceID,
                speed: Double(request.rateMultiplier)
            )
        }
        renderTasks[taskID] = task
        defer { renderTasks.removeValue(forKey: taskID) }

        let data = try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
        let duration = (try? AVAudioPlayer(data: data).duration * 1_000) ?? 0
        return ReadioNativeSpeechAudio(
            data: data,
            durationMilliseconds: duration,
            sampleRate: 0
        )
    }

    func cancelAll() {
        let tasks = renderTasks.values
        renderTasks.removeAll()
        for task in tasks {
            task.cancel()
        }
    }
}

@MainActor
enum AppleTTSProviderFactory {
    static func make(preferences: AppleTTSPreferences) -> any ReadioAppleTTSProvider {
        switch preferences.mode {
        case .native:
            AppleNativeTTSProvider()
        case .cloud:
            AppleCloudTTSProvider(voiceID: preferences.cloudVoiceID)
        }
    }
}
