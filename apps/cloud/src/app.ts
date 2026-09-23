export interface AuthenticatedUser {
  userId: string
}

export type SyncEntityKind =
  | "library_item"
  | "reading_position"
  | "bookmark"
  | "setting"

export interface SyncMutation {
  mutationId: string
  entityKind: SyncEntityKind
  entityId: string
  operation: "upsert" | "delete"
  baseVersion: number
  clientTimestampUnixMs: number
  payload: unknown
}

export interface SyncPushBatch {
  deviceId: string
  mutations: SyncMutation[]
}

export interface SyncMutationResult {
  mutationId: string
  status: "applied" | "replayed" | "conflict"
  version?: number
  cursor?: number
  current?: SyncChange
}

export interface SyncPushResult {
  cursor: number
  results: SyncMutationResult[]
}

export interface SyncChange {
  cursor: number
  entityKind: SyncEntityKind
  entityId: string
  operation: "upsert" | "delete"
  version: number
  serverTimestampUnixMs: number
  payload: unknown
}

export interface SyncPullResult {
  cursor: number
  hasMore: boolean
  changes: SyncChange[]
}

export interface ReplicaChangeInput {
  changeId: string
  changeBase64: string
}

export interface ReplicaPushBatch {
  deviceId: string
  ackCursor: number
  headsBase64: string
  changes: ReplicaChangeInput[]
}

export interface ReplicaPushResult {
  cursor: number
  acceptedChangeIds: string[]
}

export interface ReplicaStoredChange extends ReplicaChangeInput {
  cursor: number
  deviceId: string
}

export interface ReplicaPullResult {
  cursor: number
  hasMore: boolean
  changes: ReplicaStoredChange[]
}

export interface ReplicaSnapshotInput {
  deviceId: string
  cursor: number
  byteLength: number
  body: ReadableStream<Uint8Array>
}

export interface ReplicaSnapshotRecord {
  cursor: number
  etag: string
  byteLength: number
}

export interface ReplicaSnapshotDownload extends ReplicaSnapshotRecord {
  body: ReadableStream<Uint8Array>
}

export type LibraryContentVariant = "reader" | "source"

export interface LibraryContentUploadInput {
  itemId: string
  variant: LibraryContentVariant
  byteLength: number
  contentType: string
  sha256: ArrayBuffer
  sha256Base64: string
  body: ReadableStream<Uint8Array>
}

export interface LibraryContentRecord {
  itemId: string
  variant: LibraryContentVariant
  byteLength: number
  contentType: string
  sha256Base64: string
  etag: string
}

export interface LibraryContentDownload extends LibraryContentRecord {
  body: ReadableStream<Uint8Array>
}

export interface TTSJobInput {
  modelId: string
  language: string
  text: string
  voiceId: string
  speed: number
}

export interface TTSJobRecord {
  id: string
  operationId: string
  status: "queued" | "running" | "completed" | "failed"
  createdAtUnixMs: number
  audioURL?: string
  errorCode?: string
}

export interface TTSVoiceRecord {
  id: string
  label: string
}

export interface TTSModelRecord {
  id: string
  provider: string
  label: string
  languages: string[]
  voices: TTSVoiceRecord[]
  defaultVoiceId: string
}

export interface TTSModelCatalog {
  schemaVersion: 1
  defaultModelId: string
  models: TTSModelRecord[]
}

export interface SyncService {
  push(userId: string, batch: SyncPushBatch): Promise<SyncPushResult>
  pull(userId: string, cursor: number, limit: number): Promise<SyncPullResult>
}

export interface ReplicaService {
  push(userId: string, batch: ReplicaPushBatch): Promise<ReplicaPushResult>
  pull(userId: string, cursor: number, limit: number): Promise<ReplicaPullResult>
  putSnapshot(
    userId: string,
    input: ReplicaSnapshotInput,
  ): Promise<ReplicaSnapshotRecord | null>
  getSnapshot(userId: string): Promise<ReplicaSnapshotDownload | null>
}

export interface LibraryContentService {
  put(userId: string, input: LibraryContentUploadInput): Promise<LibraryContentRecord>
  head(
    userId: string,
    itemId: string,
    variant: LibraryContentVariant,
  ): Promise<LibraryContentRecord | null>
  get(
    userId: string,
    itemId: string,
    variant: LibraryContentVariant,
  ): Promise<LibraryContentDownload | null>
}

export interface TTSService {
  models(): Promise<TTSModelCatalog>
  create(userId: string, input: TTSJobInput, operationId: string): Promise<TTSJobRecord>
  get(userId: string, jobId: string): Promise<TTSJobRecord | null>
  audio(userId: string, jobId: string): Promise<Response | null>
}

export interface CloudAppDependencies {
  authenticate(request: Request): Promise<AuthenticatedUser | null>
  authHandler(request: Request): Promise<Response>
  fetchURL(input: URL, init: RequestInit): Promise<Response>
  sync: SyncService
  replica: ReplicaService
  content: LibraryContentService
  tts: TTSService
}

export interface CloudApp {
  fetch(request: Request): Promise<Response>
}

const JSON_HEADERS = {
  "content-type": "application/json; charset=utf-8",
  "cache-control": "no-store",
}
const MAX_JSON_BODY_BYTES = 5 * 1024 * 1024
const MAX_URL_SOURCE_BYTES = 5 * 1024 * 1024
const MAX_URL_REDIRECTS = 5

function diagnosticOperationId(request: Request): string {
  const candidate = request.headers.get("x-readio-operation-id")?.trim() ?? ""
  if (/^[A-Za-z0-9][A-Za-z0-9._:-]{0,95}$/.test(candidate)) {
    return candidate
  }
  return `op-${crypto.randomUUID()}`
}

function json(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: JSON_HEADERS,
  })
}

function error(status: number, code: string, message: string): Response {
  return json({ error: { code, message } }, status)
}

function libraryContentHeaders(stored: LibraryContentRecord): HeadersInit {
  return {
    "cache-control": "private, no-store",
    "content-type": stored.contentType,
    "content-length": String(stored.byteLength),
    "x-readio-content-sha256": stored.sha256Base64,
    etag: stored.etag,
  }
}

async function requireUser(
  request: Request,
  dependencies: CloudAppDependencies,
): Promise<AuthenticatedUser | Response> {
  const user = await dependencies.authenticate(request)
  return (
    user ??
    error(
      401,
      "auth_required",
      "Sign in to use cloud sync or cloud TTS.",
    )
  )
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
}

async function readObject(
  request: Request,
): Promise<Record<string, unknown> | Response | null> {
  const declaredLength = request.headers.get("content-length")
  if (declaredLength !== null) {
    const parsed = Number(declaredLength)
    if (!Number.isSafeInteger(parsed) || parsed < 0) return null
    if (parsed > MAX_JSON_BODY_BYTES) {
      return error(413, "request_too_large", "JSON request body exceeds the size limit.")
    }
  }
  if (!request.body) return null

  try {
    const reader = request.body.getReader()
    const chunks: Uint8Array[] = []
    let byteLength = 0
    while (true) {
      const next = await reader.read()
      if (next.done) break
      byteLength += next.value.byteLength
      if (byteLength > MAX_JSON_BODY_BYTES) {
        await reader.cancel()
        return error(413, "request_too_large", "JSON request body exceeds the size limit.")
      }
      chunks.push(next.value)
    }

    const encoded = new Uint8Array(byteLength)
    let offset = 0
    for (const chunk of chunks) {
      encoded.set(chunk, offset)
      offset += chunk.byteLength
    }
    const value: unknown = JSON.parse(new TextDecoder().decode(encoded))
    return isRecord(value) ? value : null
  } catch {
    return null
  }
}

function isSyncPushBatch(
  value: Record<string, unknown>,
): value is Record<string, unknown> & SyncPushBatch {
  return (
    typeof value.deviceId === "string" &&
    value.deviceId.length > 0 &&
    value.deviceId.length <= 128 &&
    Array.isArray(value.mutations) &&
    value.mutations.length <= 100
  )
}

const CHANGE_ID_PATTERN = /^[0-9a-f]{64}$/
const BASE64_PATTERN = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/
const MAX_REPLICA_CHANGES = 32
const MAX_REPLICA_CHANGE_BASE64_BYTES = 512 * 1024
const MAX_REPLICA_BATCH_BASE64_BYTES = 4 * 1024 * 1024
const MAX_REPLICA_SNAPSHOT_BYTES = 16 * 1024 * 1024
const MAX_READER_CONTENT_BYTES = 32 * 1024 * 1024
const MAX_SOURCE_CONTENT_BYTES = 200 * 1024 * 1024
const SHA256_BASE64_PATTERN = /^[A-Za-z0-9+/]{43}=$/

function isBase64(value: string, maximumLength: number): boolean {
  return value.length <= maximumLength && value.length % 4 === 0 && BASE64_PATTERN.test(value)
}

function isReplicaPushBatch(
  value: Record<string, unknown>,
): value is Record<string, unknown> & ReplicaPushBatch {
  if (
    typeof value.deviceId !== "string" ||
    value.deviceId.length === 0 ||
    value.deviceId.length > 128 ||
    typeof value.ackCursor !== "number" ||
    !Number.isSafeInteger(value.ackCursor) ||
    value.ackCursor < 0 ||
    typeof value.headsBase64 !== "string" ||
    !isBase64(value.headsBase64, 8 * 1024) ||
    !Array.isArray(value.changes) ||
    value.changes.length > MAX_REPLICA_CHANGES
  ) {
    return false
  }

  let encodedBytes = 0
  for (const candidate of value.changes) {
    if (!isRecord(candidate)) return false
    const change = candidate
    if (
      typeof change.changeId !== "string" ||
      !CHANGE_ID_PATTERN.test(change.changeId) ||
      typeof change.changeBase64 !== "string" ||
      change.changeBase64.length === 0 ||
      !isBase64(change.changeBase64, MAX_REPLICA_CHANGE_BASE64_BYTES)
    ) {
      return false
    }
    encodedBytes += change.changeBase64.length
    if (encodedBytes > MAX_REPLICA_BATCH_BASE64_BYTES) return false
  }
  return true
}

function parseTTSInput(value: Record<string, unknown>): TTSJobInput | null {
  if (
    typeof value.modelId !== "string" ||
    !/^[a-z0-9.-]+\/[a-z0-9.-]+$/.test(value.modelId) ||
    value.modelId.length > 128 ||
    typeof value.language !== "string" ||
    !/^[A-Za-z]{2,8}(?:[-_][A-Za-z0-9]{1,8})*$/.test(value.language) ||
    value.language.length > 64 ||
    typeof value.text !== "string" ||
    value.text.length === 0 ||
    value.text.length > 20_000 ||
    typeof value.voiceId !== "string" ||
    value.voiceId.length === 0 ||
    value.voiceId.length > 128 ||
    typeof value.speed !== "number" ||
    !Number.isFinite(value.speed) ||
    value.speed < 0.5 ||
    value.speed > 3
  ) {
    return null
  }

  return {
    modelId: value.modelId,
    language: value.language,
    text: value.text,
    voiceId: value.voiceId,
    speed: value.speed,
  }
}

function integerQuery(
  value: string | null,
  fallback: number,
  minimum: number,
  maximum: number,
): number | null {
  if (value === null) return fallback
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) && parsed >= minimum && parsed <= maximum
    ? parsed
    : null
}

function sha256Bytes(value: string | null): ArrayBuffer | null {
  if (!value || !SHA256_BASE64_PATTERN.test(value)) return null
  try {
    const decoded = atob(value)
    if (decoded.length !== 32) return null
    return Uint8Array.from(decoded, (character) => character.charCodeAt(0)).buffer
  } catch {
    return null
  }
}

function decodedItemId(value: string): string | null {
  try {
    const decoded = decodeURIComponent(value)
    return decoded.length > 0 &&
      decoded.length <= 256 &&
      !/[\u0000-\u001f\u007f]/.test(decoded)
      ? decoded
      : null
  } catch {
    return null
  }
}

function acquisitionURL(value: unknown): URL | null {
  if (typeof value !== "string" || value.length === 0 || value.length > 2_048) {
    return null
  }
  try {
    const url = new URL(value)
    if (
      (url.protocol !== "https:" && url.protocol !== "http:") ||
      url.username.length > 0 ||
      url.password.length > 0 ||
      url.hostname.length === 0 ||
      !isPublicAcquisitionHostname(url.hostname)
    ) {
      return null
    }
    return url
  } catch {
    return null
  }
}

function isPublicAcquisitionHostname(hostname: string): boolean {
  const normalized = hostname.toLowerCase().replace(/\.$/, "")
  if (
    normalized === "localhost" ||
    normalized.endsWith(".localhost") ||
    normalized.endsWith(".local") ||
    normalized.endsWith(".localdomain") ||
    normalized.endsWith(".internal") ||
    normalized.endsWith(".home.arpa")
  ) {
    return false
  }

  // WHATWG URL parsing canonicalizes legacy integer/octal IPv4 spellings.
  // Reject all literal IPs so neither those spellings nor IPv6 can bypass the
  // host allow-boundary on an anonymous outbound-fetch endpoint.
  if (
    /^(?:\d{1,3}\.){3}\d{1,3}$/.test(normalized) ||
    normalized.startsWith("[") ||
    normalized.includes(":")
  ) {
    return false
  }
  return true
}

async function boundedHTML(response: Response): Promise<string | Response> {
  const contentType = response.headers.get("content-type")?.trim() ?? ""
  const mediaType = contentType.split(";", 1)[0].trim().toLowerCase()
  if (mediaType !== "text/html" && mediaType !== "application/xhtml+xml") {
    return error(415, "unsupported_media_type", "The URL did not return HTML content.")
  }

  const declaredLength = response.headers.get("content-length")
  if (declaredLength !== null) {
    const parsed = Number(declaredLength)
    if (!Number.isSafeInteger(parsed) || parsed < 0) {
      return error(502, "invalid_upstream", "The URL returned an invalid content length.")
    }
    if (parsed > MAX_URL_SOURCE_BYTES) {
      return error(413, "source_too_large", "The URL source exceeds the import size limit.")
    }
  }
  if (!response.body) {
    return error(502, "empty_upstream", "The URL returned no source body.")
  }

  const reader = response.body.getReader()
  const chunks: Uint8Array[] = []
  let byteLength = 0
  try {
    while (true) {
      const next = await reader.read()
      if (next.done) break
      byteLength += next.value.byteLength
      if (byteLength > MAX_URL_SOURCE_BYTES) {
        await reader.cancel()
        return error(413, "source_too_large", "The URL source exceeds the import size limit.")
      }
      chunks.push(next.value)
    }
  } catch {
    return error(
      502,
      "acquisition_failed",
      "The URL source stream was interrupted.",
    )
  }

  const bytes = new Uint8Array(byteLength)
  let offset = 0
  for (const chunk of chunks) {
    bytes.set(chunk, offset)
    offset += chunk.byteLength
  }
  return new TextDecoder().decode(bytes)
}

export function createCloudApp(dependencies: CloudAppDependencies): CloudApp {
  return {
    async fetch(request) {
      const url = new URL(request.url)

      if (url.pathname.startsWith("/api/auth/")) {
        return dependencies.authHandler(request)
      }

      if (request.method === "GET" && url.pathname === "/v1/capabilities") {
        return json({
          apiVersion: 1,
          localUseRequiresAuth: false,
          urlAcquisitionRequiresAuth: false,
          syncRequiresAuth: true,
          contentSyncRequiresAuth: true,
          cloudTTSRequiresAuth: true,
        })
      }

      if (request.method === "POST" && url.pathname === "/v1/acquire/url") {
        const body = await readObject(request)
        if (body instanceof Response) return body
        const initialTarget = acquisitionURL(body?.url)
        if (!initialTarget) {
          return error(400, "invalid_url", "A public HTTP or HTTPS URL is required.")
        }

        let target = initialTarget
        let upstream: Response | null = null
        for (let redirects = 0; redirects <= MAX_URL_REDIRECTS; redirects += 1) {
          try {
            upstream = await dependencies.fetchURL(target, {
              method: "GET",
              redirect: "manual",
              headers: {
                accept: "text/html,application/xhtml+xml;q=0.9",
              },
            })
          } catch (cause) {
            console.error(JSON.stringify({
              message: "url acquisition failed",
              origin: target.origin,
              error: cause instanceof Error ? cause.message : String(cause),
            }))
            return error(502, "acquisition_failed", "The URL could not be acquired.")
          }

          if (![301, 302, 303, 307, 308].includes(upstream.status)) break
          const location = upstream.headers.get("location")
          await upstream.body?.cancel()
          if (!location) {
            return error(502, "invalid_redirect", "The URL returned a redirect without a target.")
          }
          if (redirects === MAX_URL_REDIRECTS) {
            return error(502, "redirect_limit", "The URL exceeded the redirect limit.")
          }
          let redirected: URL | null = null
          try {
            redirected = acquisitionURL(new URL(location, target).href)
          } catch {
            // The stable acquisition error below is the public boundary for a
            // malformed Location header from an untrusted upstream.
          }
          if (!redirected) {
            return error(502, "invalid_redirect", "The URL redirected to an invalid target.")
          }
          target = redirected
        }
        if (!upstream) return error(502, "acquisition_failed", "The URL could not be acquired.")
        if (!upstream.ok) {
          return error(
            502,
            "upstream_status",
            `The URL returned HTTP ${upstream.status}.`,
          )
        }
        const html = await boundedHTML(upstream)
        if (html instanceof Response) return html
        return json({
          final_url: target.href,
          media_type: upstream.headers.get("content-type") ?? "text/html",
          html,
        })
      }

      if (request.method === "GET" && url.pathname === "/v1/tts/models") {
        return json(await dependencies.tts.models())
      }

      const libraryContentMatch = url.pathname.match(
        /^\/v1\/library\/content\/([^/]+)\/(reader|source)$/,
      )
      if (["GET", "HEAD", "PUT"].includes(request.method) && libraryContentMatch) {
        const user = await requireUser(request, dependencies)
        if (user instanceof Response) return user
        const itemId = decodedItemId(libraryContentMatch[1])
        const variant = libraryContentMatch[2]
        if (!itemId || (variant !== "reader" && variant !== "source")) {
          return error(400, "invalid_request", "The library item id is invalid.")
        }

        if (request.method === "PUT") {
          const lengthHeader = request.headers.get("content-length")
          const maximumLength =
            variant === "reader" ? MAX_READER_CONTENT_BYTES : MAX_SOURCE_CONTENT_BYTES
          const byteLength = lengthHeader
            ? integerQuery(lengthHeader, 0, 1, maximumLength)
            : null
          const contentType = request.headers.get("content-type")?.trim() ?? ""
          const sha256Base64 = request.headers.get("x-readio-content-sha256") ?? ""
          const sha256 = sha256Bytes(sha256Base64)
          if (
            byteLength === null ||
            contentType.length === 0 ||
            contentType.length > 256 ||
            /[\u0000-\u001f\u007f]/.test(contentType) ||
            !sha256 ||
            !request.body
          ) {
            return error(
              400,
              "invalid_request",
              "A bounded content stream, media type, and SHA-256 digest are required.",
            )
          }
          const stored = await dependencies.content.put(user.userId, {
            itemId,
            variant,
            byteLength,
            contentType,
            sha256,
            sha256Base64,
            body: request.body,
          })
          return json(stored, 201)
        }

        if (request.method === "HEAD") {
          const stored = await dependencies.content.head(user.userId, itemId, variant)
          return stored
            ? new Response(null, { headers: libraryContentHeaders(stored) })
            : error(404, "not_found", "Library content not found.")
        }
        const stored = await dependencies.content.get(user.userId, itemId, variant)
        return stored
          ? new Response(stored.body, { headers: libraryContentHeaders(stored) })
          : error(404, "not_found", "Library content not found.")
      }

      if (request.method === "POST" && url.pathname === "/v1/sync/push") {
        const user = await requireUser(request, dependencies)
        if (user instanceof Response) return user

        const body = await readObject(request)
        if (body instanceof Response) return body
        if (!body || !isSyncPushBatch(body)) {
          return error(400, "invalid_request", "A deviceId and mutations array are required.")
        }

        return json(await dependencies.sync.push(user.userId, body))
      }

      if (request.method === "GET" && url.pathname === "/v1/sync/pull") {
        const user = await requireUser(request, dependencies)
        if (user instanceof Response) return user

        const cursor = integerQuery(url.searchParams.get("cursor"), 0, 0, Number.MAX_SAFE_INTEGER)
        const limit = integerQuery(url.searchParams.get("limit"), 100, 1, 500)
        if (cursor === null || limit === null) {
          return error(400, "invalid_request", "cursor or limit is invalid.")
        }

        return json(await dependencies.sync.pull(user.userId, cursor, limit))
      }

      if (request.method === "POST" && url.pathname === "/v1/replica/push") {
        const user = await requireUser(request, dependencies)
        if (user instanceof Response) return user

        const body = await readObject(request)
        if (body instanceof Response) return body
        if (!body || !isReplicaPushBatch(body)) {
          return error(
            400,
            "invalid_request",
            "A deviceId, acknowledgement, heads, and bounded CRDT changes are required.",
          )
        }

        const batch: ReplicaPushBatch = {
          deviceId: body.deviceId,
          ackCursor: body.ackCursor,
          headsBase64: body.headsBase64,
          changes: body.changes.map((change) => ({
            changeId: change.changeId,
            changeBase64: change.changeBase64,
          })),
        }
        return json(await dependencies.replica.push(user.userId, batch))
      }

      if (request.method === "GET" && url.pathname === "/v1/replica/pull") {
        const user = await requireUser(request, dependencies)
        if (user instanceof Response) return user

        const cursor = integerQuery(url.searchParams.get("cursor"), 0, 0, Number.MAX_SAFE_INTEGER)
        const limit = integerQuery(url.searchParams.get("limit"), 100, 1, 500)
        if (cursor === null || limit === null) {
          return error(400, "invalid_request", "cursor or limit is invalid.")
        }

        return json(await dependencies.replica.pull(user.userId, cursor, limit))
      }

      if (request.method === "PUT" && url.pathname === "/v1/replica/snapshot") {
        const user = await requireUser(request, dependencies)
        if (user instanceof Response) return user

        const deviceId = request.headers.get("x-readio-device-id")
        const cursorHeader = request.headers.get("x-readio-replica-cursor")
        const lengthHeader = request.headers.get("content-length")
        const cursor = cursorHeader
          ? integerQuery(cursorHeader, 0, 0, Number.MAX_SAFE_INTEGER)
          : null
        const byteLength = lengthHeader
          ? integerQuery(lengthHeader, 0, 1, MAX_REPLICA_SNAPSHOT_BYTES)
          : null
        if (
          !deviceId ||
          deviceId.length > 128 ||
          cursor === null ||
          byteLength === null ||
          request.headers.get("content-type") !== "application/octet-stream" ||
          !request.body
        ) {
          return error(
            400,
            "invalid_request",
            "A bounded binary snapshot, device id, and replica cursor are required.",
          )
        }

        const snapshot = await dependencies.replica.putSnapshot(user.userId, {
          deviceId,
          cursor,
          byteLength,
          body: request.body,
        })
        return snapshot
          ? json(snapshot, 201)
          : error(409, "snapshot_ahead", "Snapshot cursor is ahead of the stored change log.")
      }

      if (request.method === "GET" && url.pathname === "/v1/replica/snapshot") {
        const user = await requireUser(request, dependencies)
        if (user instanceof Response) return user

        const snapshot = await dependencies.replica.getSnapshot(user.userId)
        if (!snapshot) return error(404, "not_found", "Replica snapshot not found.")
        return new Response(snapshot.body, {
          headers: {
            "cache-control": "no-store",
            "content-type": "application/octet-stream",
            "content-length": String(snapshot.byteLength),
            etag: snapshot.etag,
            "x-readio-replica-cursor": String(snapshot.cursor),
          },
        })
      }

      if (request.method === "POST" && url.pathname === "/v1/tts/jobs") {
        const user = await requireUser(request, dependencies)
        if (user instanceof Response) return user

        const body = await readObject(request)
        if (body instanceof Response) return body
        const input = body ? parseTTSInput(body) : null
        if (!input) {
          return error(
            400,
            "invalid_request",
            "modelId, language, text, voiceId, and speed are required.",
          )
        }

        return json(
          await dependencies.tts.create(
            user.userId,
            input,
            diagnosticOperationId(request),
          ),
          202,
        )
      }

      const ttsJobMatch = url.pathname.match(/^\/v1\/tts\/jobs\/([^/]+)$/)
      if (request.method === "GET" && ttsJobMatch) {
        const user = await requireUser(request, dependencies)
        if (user instanceof Response) return user

        const job = await dependencies.tts.get(user.userId, decodeURIComponent(ttsJobMatch[1]))
        return job ? json(job) : error(404, "not_found", "TTS job not found.")
      }

      const ttsAudioMatch = url.pathname.match(/^\/v1\/tts\/jobs\/([^/]+)\/audio$/)
      if (request.method === "GET" && ttsAudioMatch) {
        const user = await requireUser(request, dependencies)
        if (user instanceof Response) return user

        const audio = await dependencies.tts.audio(
          user.userId,
          decodeURIComponent(ttsAudioMatch[1]),
        )
        return audio ?? error(404, "not_found", "TTS audio not found.")
      }

      return error(404, "not_found", "Route not found.")
    },
  }
}
