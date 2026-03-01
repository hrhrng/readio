import Foundation

// MARK: - APIError
/// Strongly-typed error cases for all API operations.
///
/// Each case carries enough context for the UI to display a meaningful error
/// message and for debugging to pinpoint the failure.
enum APIError: LocalizedError {
    /// The URL string could not be parsed into a valid `URL`.
    case invalidURL(String)

    /// The server returned a non-2xx HTTP status code.
    /// Includes the status code and the response body (if readable) for diagnostics.
    case httpError(statusCode: Int, message: String)

    /// The response body could not be decoded into the expected `Decodable` type.
    /// Wraps the underlying `DecodingError` for inspection.
    case decodingError(Error)

    /// A network-level failure (no connectivity, DNS resolution failed, timeout, etc.).
    /// Wraps the underlying `URLError` or other transport error.
    case networkError(Error)

    /// The server returned an empty body where a non-empty response was expected.
    case emptyResponse

    var errorDescription: String? {
        switch self {
        case .invalidURL(let url):
            return "Invalid URL: \(url)"
        case .httpError(let statusCode, let message):
            return "HTTP \(statusCode): \(message)"
        case .decodingError(let error):
            return "Failed to decode response: \(error.localizedDescription)"
        case .networkError(let error):
            return "Network error: \(error.localizedDescription)"
        case .emptyResponse:
            return "Server returned an empty response"
        }
    }
}

// MARK: - APIClient
/// Generic async HTTP client for communicating with the Readio backend API.
///
/// **Design decisions:**
/// - Singleton via `shared` for convenience; all service classes use this instance.
/// - `baseURL` is dynamically read from `AppSettings` so the user can change
///   the server address at runtime without restarting the app.
/// - JSON encoder/decoder use `.convertToSnakeCase` / `.convertFromSnakeCase`
///   to match the Python backend's snake_case convention.
/// - Zero external dependencies — uses only `URLSession` from Foundation.
///
/// **Error handling:**
/// All methods throw `APIError` with specific cases so callers can handle
/// different failure modes (e.g., show "server unreachable" vs "item not found").
///
/// **Thread safety:**
/// The client is stateless aside from the `URLSession` (which is thread-safe)
/// and the `baseURL` accessor (which reads from `UserDefaults`, also thread-safe).
final class APIClient: Sendable {
    // MARK: - Singleton
    /// Shared instance used by all service classes.
    static let shared = APIClient()

    // MARK: - URLSession
    /// The underlying URL session. Uses the default (shared) configuration
    /// with standard caching and cookie policies.
    private let session: URLSession

    // MARK: - JSON Encoder/Decoder
    // Configured once with snake_case key strategy to match the Python API's
    // Pydantic models which use snake_case field names.

    /// JSON encoder for serializing request bodies.
    /// Key strategy: camelCase Swift properties → snake_case JSON keys.
    nonisolated(unsafe) static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        return encoder
    }()

    /// JSON decoder for deserializing response bodies.
    /// Key strategy: snake_case JSON keys → camelCase Swift properties.
    nonisolated(unsafe) static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }()

    // MARK: - Initialization
    private init() {
        let configuration = URLSessionConfiguration.default
        // Set a reasonable timeout so the UI doesn't hang indefinitely
        // on unreachable servers. 30s is enough for large EPUB imports.
        configuration.timeoutIntervalForRequest = 30
        configuration.timeoutIntervalForResource = 120
        self.session = URLSession(configuration: configuration)
    }

    // MARK: - Base URL
    /// Reads the current server URL from UserDefaults.
    ///
    /// This is read dynamically on every request so the user can change
    /// the server address in settings without restarting the app.
    private var baseURL: String {
        UserDefaults.standard.string(forKey: AppSettingsKeys.serverURL) ?? "http://localhost:8000"
    }

    // MARK: - Internal Helpers

    /// Constructs a full URL from the base URL and an endpoint path.
    /// The path should start with "/" (e.g., "/api/library/items").
    private func buildURL(_ path: String) throws -> URL {
        let urlString = baseURL + path
        guard let url = URL(string: urlString) else {
            throw APIError.invalidURL(urlString)
        }
        return url
    }

    /// Executes a URLRequest and validates the HTTP response status code.
    /// Returns the raw response `Data` for further processing.
    ///
    /// - Throws: `APIError.httpError` for 4xx/5xx responses,
    ///           `APIError.networkError` for transport failures.
    private func execute(_ request: URLRequest) async throws -> Data {
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            throw APIError.networkError(error)
        }

        guard let httpResponse = response as? HTTPURLResponse else {
            throw APIError.networkError(
                URLError(.badServerResponse, userInfo: [NSLocalizedDescriptionKey: "Non-HTTP response received"])
            )
        }

        // 2xx status codes are success; everything else is an error.
        guard (200...299).contains(httpResponse.statusCode) else {
            // Try to extract a human-readable error message from the response body.
            // The Readio API returns errors as {"detail": "message"}.
            let message = Self.extractErrorMessage(from: data) ?? "Request failed"
            throw APIError.httpError(statusCode: httpResponse.statusCode, message: message)
        }

        return data
    }

    /// Attempts to extract an error message from a JSON response body.
    ///
    /// The Readio backend (FastAPI) returns errors in the format:
    /// `{"detail": "human-readable error message"}`
    /// If parsing fails, returns the raw UTF-8 string or nil.
    private static func extractErrorMessage(from data: Data) -> String? {
        // Try FastAPI's standard error format first
        if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let detail = json["detail"] as? String {
            return detail
        }
        // Fall back to raw string
        let raw = String(data: data, encoding: .utf8)
        return raw?.isEmpty == false ? raw : nil
    }

    // MARK: - GET

    /// Performs a GET request and decodes the JSON response into the specified type.
    ///
    /// - Parameter path: API endpoint path (e.g., "/api/library/items?page=1").
    /// - Returns: Decoded response of type `T`.
    /// - Throws: `APIError` on network, HTTP, or decoding failure.
    func get<T: Decodable>(_ path: String) async throws -> T {
        let url = try buildURL(path)
        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.setValue("application/json", forHTTPHeaderField: "Accept")

        let data = try await execute(request)

        do {
            return try Self.decoder.decode(T.self, from: data)
        } catch {
            throw APIError.decodingError(error)
        }
    }

    // MARK: - POST

    /// Performs a POST request with a JSON body and decodes the response.
    ///
    /// - Parameters:
    ///   - path: API endpoint path.
    ///   - body: Encodable value to serialize as the JSON request body.
    /// - Returns: Decoded response of type `T`.
    /// - Throws: `APIError` on network, HTTP, encoding, or decoding failure.
    func post<T: Decodable>(_ path: String, body: some Encodable) async throws -> T {
        let url = try buildURL(path)
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.httpBody = try Self.encoder.encode(body)

        let data = try await execute(request)

        do {
            return try Self.decoder.decode(T.self, from: data)
        } catch {
            throw APIError.decodingError(error)
        }
    }

    /// Performs a POST request with a JSON body that does not return a decoded response.
    ///
    /// Used for endpoints that return a simple status (e.g., `{"status": "ok"}`)
    /// where the caller doesn't need the response body.
    ///
    /// - Parameters:
    ///   - path: API endpoint path.
    ///   - body: Encodable value to serialize as the JSON request body.
    /// - Throws: `APIError` on network or HTTP failure.
    func post(_ path: String, body: some Encodable) async throws {
        let url = try buildURL(path)
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try Self.encoder.encode(body)

        _ = try await execute(request)
    }

    // MARK: - PATCH

    /// Performs a PATCH request with a JSON body.
    ///
    /// Used for partial updates (e.g., updating progress, voice, speed, chapter).
    /// Most PATCH endpoints return `{"status": "ok"}` which is discarded here.
    ///
    /// - Parameters:
    ///   - path: API endpoint path.
    ///   - body: Encodable value to serialize as the JSON request body.
    /// - Throws: `APIError` on network or HTTP failure.
    func patch(_ path: String, body: some Encodable) async throws {
        let url = try buildURL(path)
        var request = URLRequest(url: url)
        request.httpMethod = "PATCH"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try Self.encoder.encode(body)

        _ = try await execute(request)
    }

    /// Performs a PATCH request with a raw dictionary body.
    ///
    /// Used for the settings endpoint which accepts arbitrary key-value pairs
    /// (`PATCH /api/settings`) rather than a strongly-typed model.
    ///
    /// - Parameters:
    ///   - path: API endpoint path.
    ///   - dictionary: Key-value pairs to serialize as JSON.
    /// - Throws: `APIError` on network or HTTP failure.
    func patch(_ path: String, dictionary: [String: String?]) async throws {
        let url = try buildURL(path)
        var request = URLRequest(url: url)
        request.httpMethod = "PATCH"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: dictionary)

        _ = try await execute(request)
    }

    // MARK: - DELETE

    /// Performs a DELETE request.
    ///
    /// The Readio API returns 204 No Content for successful deletes,
    /// so no response body is expected or decoded.
    ///
    /// - Parameter path: API endpoint path (e.g., "/api/library/items/{id}").
    /// - Throws: `APIError` on network or HTTP failure.
    func delete(_ path: String) async throws {
        let url = try buildURL(path)
        var request = URLRequest(url: url)
        request.httpMethod = "DELETE"

        _ = try await execute(request)
    }

    // MARK: - Multipart Form Upload

    /// Performs a multipart/form-data POST request for file uploads.
    ///
    /// Used by the file import endpoint (`POST /api/library/import/file`) which
    /// expects the file as a form part along with metadata fields.
    ///
    /// **Multipart format:**
    /// ```
    /// --boundary
    /// Content-Disposition: form-data; name="file"; filename="book.epub"
    /// Content-Type: application/octet-stream
    ///
    /// <binary data>
    /// --boundary
    /// Content-Disposition: form-data; name="category"
    ///
    /// imported
    /// --boundary--
    /// ```
    ///
    /// - Parameters:
    ///   - path: API endpoint path.
    ///   - fileData: Raw bytes of the file to upload.
    ///   - filename: Original filename (used for format detection on the server).
    ///   - fields: Additional form fields to include (e.g., category, folder_id, title).
    /// - Returns: Decoded response of type `T` (typically `LibraryItem`).
    /// - Throws: `APIError` on network, HTTP, or decoding failure.
    func uploadMultipart<T: Decodable>(
        _ path: String,
        fileData: Data,
        filename: String,
        fields: [String: String] = [:]
    ) async throws -> T {
        let url = try buildURL(path)
        let boundary = "Readio-Upload-\(UUID().uuidString)"

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")

        // Build the multipart body
        var body = Data()

        // Append each text field as a form part
        for (key, value) in fields {
            body.appendMultipartField(name: key, value: value, boundary: boundary)
        }

        // Append the file data as a binary form part
        body.appendMultipartFile(name: "file", filename: filename, data: fileData, boundary: boundary)

        // Close the multipart body with the final boundary
        body.append("--\(boundary)--\r\n".data(using: .utf8)!)

        request.httpBody = body

        let data = try await execute(request)

        do {
            return try Self.decoder.decode(T.self, from: data)
        } catch {
            throw APIError.decodingError(error)
        }
    }
}

// MARK: - Data + Multipart Helpers
/// Extension on `Data` for constructing multipart/form-data request bodies.
///
/// These helpers handle the RFC 2046 boundary-delimited format that HTTP
/// file uploads require. Each part starts with `--boundary\r\n`, includes
/// Content-Disposition headers, and ends with `\r\n`.
private extension Data {
    /// Appends a text field to a multipart form body.
    ///
    /// Produces a part like:
    /// ```
    /// --boundary\r\n
    /// Content-Disposition: form-data; name="category"\r\n
    /// \r\n
    /// imported\r\n
    /// ```
    mutating func appendMultipartField(name: String, value: String, boundary: String) {
        var fieldString = "--\(boundary)\r\n"
        fieldString += "Content-Disposition: form-data; name=\"\(name)\"\r\n"
        fieldString += "\r\n"
        fieldString += "\(value)\r\n"
        append(fieldString.data(using: .utf8)!)
    }

    /// Appends a file part to a multipart form body.
    ///
    /// Produces a part like:
    /// ```
    /// --boundary\r\n
    /// Content-Disposition: form-data; name="file"; filename="book.epub"\r\n
    /// Content-Type: application/octet-stream\r\n
    /// \r\n
    /// <binary data>\r\n
    /// ```
    mutating func appendMultipartFile(name: String, filename: String, data fileData: Data, boundary: String) {
        var header = "--\(boundary)\r\n"
        header += "Content-Disposition: form-data; name=\"\(name)\"; filename=\"\(filename)\"\r\n"
        header += "Content-Type: application/octet-stream\r\n"
        header += "\r\n"
        append(header.data(using: .utf8)!)
        append(fileData)
        append("\r\n".data(using: .utf8)!)
    }
}
