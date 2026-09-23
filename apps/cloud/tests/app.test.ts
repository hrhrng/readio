import assert from "node:assert/strict"
import test from "node:test"

import {
  createCloudApp,
  type AuthenticatedUser,
  type CloudAppDependencies,
  type ReplicaPullResult,
  type ReplicaPushBatch,
  type ReplicaPushResult,
  type ReplicaSnapshotDownload,
  type ReplicaSnapshotInput,
  type ReplicaSnapshotRecord,
  type SyncPullResult,
  type SyncPushBatch,
  type SyncPushResult,
  type TTSJobInput,
  type TTSJobRecord,
} from "../src/app"

function request(path: string, init?: RequestInit) {
  return new Request(`https://cloud.read-io.com${path}`, init)
}

function dependencies(
  fetchURL?: CloudAppDependencies["fetchURL"],
) {
  const syncCalls: Array<{ operation: "push" | "pull"; userId: string }> = []
  const replicaCalls: Array<{ userId: string; batch: ReplicaPushBatch }> = []
  const snapshotCalls: Array<{
    userId: string
    deviceId: string
    cursor: number
    bytes: number[]
  }> = []
  const contentCalls: Array<{
    userId: string
    itemId: string
    variant: string
    bytes: number[]
  }> = []
  const ttsCalls: Array<{
    userId: string
    input: TTSJobInput
    operationId: string
  }> = []
  const acquisitionCalls: string[] = []

  const dependencies: CloudAppDependencies = {
    authenticate: async (incoming): Promise<AuthenticatedUser | null> => {
      const token = incoming.headers.get("authorization")?.replace(/^Bearer\s+/i, "")
      return token ? { userId: token } : null
    },
    authHandler: async () => new Response("auth endpoint"),
    fetchURL: async (target: URL, init: RequestInit) => {
      acquisitionCalls.push(target.href)
      return fetchURL
        ? fetchURL(target, init)
        : new Response(
            "<html><body><main><h1>Cloud article</h1></main></body></html>",
            {
              headers: { "content-type": "text/html; charset=utf-8" },
            },
          )
    },
    sync: {
      async push(userId: string, _batch: SyncPushBatch): Promise<SyncPushResult> {
        syncCalls.push({ operation: "push", userId })
        return { cursor: 7, results: [] }
      },
      async pull(userId: string, _cursor: number, _limit: number): Promise<SyncPullResult> {
        syncCalls.push({ operation: "pull", userId })
        return { cursor: 7, hasMore: false, changes: [] }
      },
    },
    replica: {
      async push(userId: string, batch: ReplicaPushBatch): Promise<ReplicaPushResult> {
        replicaCalls.push({ userId, batch })
        return {
          cursor: 9,
          acceptedChangeIds: batch.changes.map((change) => change.changeId),
        }
      },
      async pull(): Promise<ReplicaPullResult> {
        return { cursor: 9, hasMore: false, changes: [] }
      },
      async putSnapshot(
        userId: string,
        input: ReplicaSnapshotInput,
      ): Promise<ReplicaSnapshotRecord | null> {
        snapshotCalls.push({
          userId,
          deviceId: input.deviceId,
          cursor: input.cursor,
          bytes: [...new Uint8Array(await new Response(input.body).arrayBuffer())],
        })
        return { cursor: input.cursor, etag: "snapshot-etag", byteLength: input.byteLength }
      },
      async getSnapshot(): Promise<ReplicaSnapshotDownload | null> {
        return null
      },
    },
    content: {
      async put(userId, input) {
        contentCalls.push({
          userId,
          itemId: input.itemId,
          variant: input.variant,
          bytes: [...new Uint8Array(await new Response(input.body).arrayBuffer())],
        })
        return {
          itemId: input.itemId,
          variant: input.variant,
          byteLength: input.byteLength,
          contentType: input.contentType,
          sha256Base64: input.sha256Base64,
          etag: "content-etag",
        }
      },
      async get(userId, itemId, variant) {
        if (userId !== "reader-a" || itemId !== "book-a" || variant !== "reader") {
          return null
        }
        return {
          itemId,
          variant,
          byteLength: 4,
          contentType: "text/plain; charset=utf-8",
          sha256Base64: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
          etag: "content-etag",
          body: new Response("book").body!,
        }
      },
      async head(userId, itemId, variant) {
        if (userId !== "reader-a" || itemId !== "book-a" || variant !== "reader") {
          return null
        }
        return {
          itemId,
          variant,
          byteLength: 4,
          contentType: "text/plain; charset=utf-8",
          sha256Base64: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
          etag: "content-etag",
        }
      },
    },
    tts: {
      async models() {
        return {
          schemaVersion: 1 as const,
          defaultModelId: "readio/auto",
          models: [
            {
              id: "readio/auto",
              provider: "readio",
              label: "Readio Auto",
              languages: ["*"],
              voices: [{ id: "default", label: "Automatic" }],
              defaultVoiceId: "default",
            },
          ],
        }
      },
      async create(
        userId: string,
        input: TTSJobInput,
        operationId: string,
      ): Promise<TTSJobRecord> {
        ttsCalls.push({ userId, input, operationId })
        return {
          id: "tts-1",
          status: "queued",
          createdAtUnixMs: 1,
          operationId,
        }
      },
      async get(): Promise<TTSJobRecord | null> {
        return null
      },
      async audio(): Promise<Response | null> {
        return null
      },
    },
  }

  return {
    app: createCloudApp(dependencies),
    syncCalls,
    replicaCalls,
    snapshotCalls,
    contentCalls,
    ttsCalls,
    acquisitionCalls,
  }
}

test("anonymous local readers acquire raw HTML through the Cloudflare boundary", async () => {
  const { app, acquisitionCalls } = dependencies()
  const response = await app.fetch(
    request("/v1/acquire/url", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ url: "https://example.com/article" }),
    }),
  )

  assert.equal(response.status, 200)
  assert.deepEqual(await response.json(), {
    final_url: "https://example.com/article",
    media_type: "text/html; charset=utf-8",
    html: "<html><body><main><h1>Cloud article</h1></main></body></html>",
  })
  assert.deepEqual(acquisitionCalls, ["https://example.com/article"])
})

test("URL acquisition follows redirects only through revalidated targets", async () => {
  const { app, acquisitionCalls } = dependencies(async (target) => {
    if (target.pathname === "/article") {
      return new Response(null, {
        status: 302,
        headers: { location: "/final-article" },
      })
    }
    return new Response("<main><h1>Final article</h1></main>", {
      headers: { "content-type": "text/html" },
    })
  })
  const response = await app.fetch(
    request("/v1/acquire/url", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ url: "https://example.com/article" }),
    }),
  )

  assert.equal(response.status, 200)
  assert.deepEqual(await response.json(), {
    final_url: "https://example.com/final-article",
    media_type: "text/html",
    html: "<main><h1>Final article</h1></main>",
  })
  assert.deepEqual(acquisitionCalls, [
    "https://example.com/article",
    "https://example.com/final-article",
  ])
})

test("URL acquisition rejects local network targets before outbound fetch", async () => {
  const { app, acquisitionCalls } = dependencies()
  const response = await app.fetch(
    request("/v1/acquire/url", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ url: "http://127.0.0.1/private" }),
    }),
  )

  assert.equal(response.status, 400)
  assert.deepEqual(await response.json(), {
    error: {
      code: "invalid_url",
      message: "A public HTTP or HTTPS URL is required.",
    },
  })
  assert.deepEqual(acquisitionCalls, [])
})

test("URL acquisition refuses redirects into a local network target", async () => {
  const { app, acquisitionCalls } = dependencies(async () =>
    new Response(null, {
      status: 302,
      headers: { location: "http://localhost/private" },
    }),
  )
  const response = await app.fetch(
    request("/v1/acquire/url", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ url: "https://example.com/article" }),
    }),
  )

  assert.equal(response.status, 502)
  assert.deepEqual(await response.json(), {
    error: {
      code: "invalid_redirect",
      message: "The URL redirected to an invalid target.",
    },
  })
  assert.deepEqual(acquisitionCalls, ["https://example.com/article"])
})

test("URL acquisition maps malformed redirect locations to a stable error", async () => {
  const { app } = dependencies(async () =>
    new Response(null, {
      status: 302,
      headers: { location: "http://[" },
    }),
  )
  const response = await app.fetch(
    request("/v1/acquire/url", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ url: "https://example.com/article" }),
    }),
  ).catch(() => null)

  assert.ok(response, "malformed redirects must become a response")
  assert.equal(response.status, 502)
  assert.deepEqual(await response.json(), {
    error: {
      code: "invalid_redirect",
      message: "The URL redirected to an invalid target.",
    },
  })
})

test("URL acquisition maps an interrupted bounded stream to a recoverable error", async () => {
  const { app } = dependencies(async () =>
    new Response(
      new ReadableStream<Uint8Array>({
        pull(controller) {
          controller.error(new Error("upstream disconnected"))
        },
      }),
      { headers: { "content-type": "text/html" } },
    ),
  )
  const response = await app.fetch(
    request("/v1/acquire/url", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ url: "https://example.com/article" }),
    }),
  ).catch(() => null)

  assert.ok(response, "stream failure must become a response")
  assert.equal(response.status, 502)
  assert.deepEqual(await response.json(), {
    error: {
      code: "acquisition_failed",
      message: "The URL source stream was interrupted.",
    },
  })
})

test("cloud TTS audio is not a public R2 URL", async () => {
  const { app } = dependencies()
  const response = await app.fetch(request("/v1/tts/jobs/tts-1/audio"))

  assert.equal(response.status, 401)
  assert.deepEqual(await response.json(), {
    error: {
      code: "auth_required",
      message: "Sign in to use cloud sync or cloud TTS.",
    },
  })
})

test("anonymous clients can discover that local use does not require an account", async () => {
  const { app } = dependencies()
  const response = await app.fetch(request("/v1/capabilities"))

  assert.equal(response.status, 200)
  assert.deepEqual(await response.json(), {
    apiVersion: 1,
    localUseRequiresAuth: false,
    urlAcquisitionRequiresAuth: false,
    syncRequiresAuth: true,
    contentSyncRequiresAuth: true,
    cloudTTSRequiresAuth: true,
  })
})

test("anonymous library content upload uses the stable cloud auth boundary", async () => {
  const { app } = dependencies()
  const response = await app.fetch(
    request("/v1/library/content/book-a/reader", {
      method: "PUT",
      headers: {
        "content-type": "text/plain",
        "content-length": "4",
        "x-readio-content-sha256": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
      },
      body: "book",
    }),
  )

  assert.equal(response.status, 401)
  assert.deepEqual(await response.json(), {
    error: {
      code: "auth_required",
      message: "Sign in to use cloud sync or cloud TTS.",
    },
  })
})

test("authenticated library content streams through the private user scope", async () => {
  const { app, contentCalls } = dependencies()
  const headers = {
    authorization: "Bearer reader-a",
    "content-type": "text/plain; charset=utf-8",
    "content-length": "4",
    "x-readio-content-sha256": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
  }
  const uploaded = await app.fetch(
    request("/v1/library/content/book-a/reader", {
      method: "PUT",
      headers,
      body: "book",
    }),
  )

  assert.equal(uploaded.status, 201)
  assert.deepEqual(await uploaded.json(), {
    itemId: "book-a",
    variant: "reader",
    byteLength: 4,
    contentType: "text/plain; charset=utf-8",
    sha256Base64: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    etag: "content-etag",
  })
  assert.deepEqual(contentCalls, [
    {
      userId: "reader-a",
      itemId: "book-a",
      variant: "reader",
      bytes: [98, 111, 111, 107],
    },
  ])

  const downloaded = await app.fetch(
    request("/v1/library/content/book-a/reader", {
      headers: { authorization: "Bearer reader-a" },
    }),
  )
  assert.equal(downloaded.status, 200)
  assert.equal(downloaded.headers.get("content-length"), "4")
  assert.equal(
    downloaded.headers.get("x-readio-content-sha256"),
    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
  )
  assert.equal(await downloaded.text(), "book")

  const metadata = await app.fetch(
    request("/v1/library/content/book-a/reader", {
      method: "HEAD",
      headers: { authorization: "Bearer reader-a" },
    }),
  )
  assert.equal(metadata.status, 200)
  assert.equal(metadata.headers.get("content-length"), "4")
  assert.equal(
    metadata.headers.get("x-readio-content-sha256"),
    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
  )
  assert.equal(await metadata.text(), "")
})

for (const endpoint of ["/v1/sync/push", "/v1/tts/jobs"]) {
  test(`anonymous POST ${endpoint} returns the stable auth boundary`, async () => {
    const { app } = dependencies()
    const response = await app.fetch(
      request(endpoint, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: "{}",
      }),
    )

    assert.equal(response.status, 401)
    assert.deepEqual(await response.json(), {
      error: {
        code: "auth_required",
        message: "Sign in to use cloud sync or cloud TTS.",
      },
    })
  })
}

test("sync scope comes from the authenticated session, never the request body", async () => {
  const { app, syncCalls } = dependencies()
  const response = await app.fetch(
    request("/v1/sync/push", {
      method: "POST",
      headers: {
        authorization: "Bearer reader-a",
        "content-type": "application/json",
      },
      body: JSON.stringify({
        userId: "reader-b",
        deviceId: "mac-a",
        mutations: [],
      }),
    }),
  )

  assert.equal(response.status, 200)
  assert.deepEqual(syncCalls, [{ operation: "push", userId: "reader-a" }])
})

test("replica push stores CRDT changes in the authenticated user's scope without conflicts", async () => {
  const { app, replicaCalls } = dependencies()
  const changeId = "a".repeat(64)
  const response = await app.fetch(
    request("/v1/replica/push", {
      method: "POST",
      headers: {
        authorization: "Bearer reader-a",
        "content-type": "application/json",
      },
      body: JSON.stringify({
        userId: "reader-b",
        deviceId: "mac-a",
        ackCursor: 4,
        headsBase64: "BAUG",
        changes: [{ changeId, changeBase64: "AQID" }],
      }),
    }),
  )

  assert.equal(response.status, 200)
  assert.deepEqual(await response.json(), {
    cursor: 9,
    acceptedChangeIds: [changeId],
  })
  assert.deepEqual(replicaCalls, [
    {
      userId: "reader-a",
      batch: {
        deviceId: "mac-a",
        ackCursor: 4,
        headsBase64: "BAUG",
        changes: [{ changeId, changeBase64: "AQID" }],
      },
    },
  ])
})

test("replica push rejects an oversized JSON body before it reaches storage", async () => {
  const { app, replicaCalls } = dependencies()
  const response = await app.fetch(
    request("/v1/replica/push", {
      method: "POST",
      headers: {
        authorization: "Bearer reader-a",
        "content-type": "application/json",
      },
      body: JSON.stringify({ padding: "x".repeat(5 * 1024 * 1024) }),
    }),
  )

  assert.equal(response.status, 413)
  assert.deepEqual(replicaCalls, [])
})

test("replica snapshot upload streams private bytes into the authenticated user's scope", async () => {
  const { app, snapshotCalls } = dependencies()
  const response = await app.fetch(
    request("/v1/replica/snapshot", {
      method: "PUT",
      headers: {
        authorization: "Bearer reader-a",
        "content-type": "application/octet-stream",
        "content-length": "3",
        "x-readio-device-id": "mac-a",
        "x-readio-replica-cursor": "9",
      },
      body: new Uint8Array([1, 2, 3]),
    }),
  )

  assert.equal(response.status, 201)
  assert.deepEqual(await response.json(), {
    cursor: 9,
    etag: "snapshot-etag",
    byteLength: 3,
  })
  assert.deepEqual(snapshotCalls, [
    {
      userId: "reader-a",
      deviceId: "mac-a",
      cursor: 9,
      bytes: [1, 2, 3],
    },
  ])
})

test("cloud TTS is queued in the authenticated user's scope", async () => {
  const { app, ttsCalls } = dependencies()
  const response = await app.fetch(
    request("/v1/tts/jobs", {
      method: "POST",
      headers: {
        authorization: "Bearer reader-a",
        "content-type": "application/json",
        "x-readio-operation-id": "op-web-tts-1",
      },
      body: JSON.stringify({
        modelId: "workers-ai/aura-2-en",
        language: "en-US",
        text: "Read this.",
        voiceId: "luna",
        speed: 1,
      }),
    }),
  )

  assert.equal(response.status, 202)
  assert.deepEqual(ttsCalls, [
    {
      userId: "reader-a",
      input: {
        modelId: "workers-ai/aura-2-en",
        language: "en-US",
        text: "Read this.",
        voiceId: "luna",
        speed: 1,
      },
      operationId: "op-web-tts-1",
    },
  ])
  const job = (await response.json()) as TTSJobRecord
  assert.equal(job.operationId, "op-web-tts-1")
})

test("cloud TTS model availability comes from the server without requiring a session", async () => {
  const { app } = dependencies()
  const response = await app.fetch(request("/v1/tts/models"))

  assert.equal(response.status, 200)
  assert.deepEqual(await response.json(), {
    schemaVersion: 1,
    defaultModelId: "readio/auto",
    models: [
      {
        id: "readio/auto",
        provider: "readio",
        label: "Readio Auto",
        languages: ["*"],
        voices: [{ id: "default", label: "Automatic" }],
        defaultVoiceId: "default",
      },
    ],
  })
})

test("Better Auth owns only the auth route namespace", async () => {
  const { app } = dependencies()
  const response = await app.fetch(request("/api/auth/session"))

  assert.equal(response.status, 200)
  assert.equal(await response.text(), "auth endpoint")
})
