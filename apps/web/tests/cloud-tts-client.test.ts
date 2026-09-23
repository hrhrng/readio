import assert from "node:assert/strict"
import test from "node:test"

test("web cloud TTS gets server availability and delegates normalization to Rust", async () => {
  const modulePath = `../lib/${"cloud-tts-client"}.ts`
  const clientModule = await import(modulePath).catch(() => ({}))
  const fetchCloudTTSModels = Reflect.get(clientModule, "fetchCloudTTSModels") as
    | ((options: Record<string, unknown>) => Promise<Record<string, unknown>>)
    | undefined
  assert.equal(typeof fetchCloudTTSModels, "function")

  const raw = {
    schemaVersion: 1,
    defaultModelId: " readio/auto ",
    models: [],
  }
  const normalized = {
    schemaVersion: 1,
    defaultModelId: "readio/auto",
    models: [{ id: "readio/auto" }],
  }
  const calls: string[] = []
  const result = await fetchCloudTTSModels!({
    async fetcher(input: RequestInfo | URL, init?: RequestInit) {
      calls.push(`${init?.method ?? "GET"} ${String(input)}`)
      return Response.json(raw)
    },
    core: {
      async normalizeTTSModelCatalog(catalog: unknown) {
        assert.deepEqual(catalog, raw)
        return normalized
      },
    },
  })

  assert.deepEqual(calls, ["GET /api/cloud/v1/tts/models"])
  assert.deepEqual(result, normalized)
})

test("web cloud TTS creates, polls, and streams private audio through the same-origin proxy", async () => {
  const modulePath = `../lib/${"cloud-tts-client"}.ts`
  const clientModule = await import(modulePath).catch(() => ({}))
  const synthesizeCloudSpeech = Reflect.get(
    clientModule,
    "synthesizeCloudSpeech"
  ) as
    | ((
        input: {
          modelId: string
          language: string
          text: string
          voiceId: string
          speed: number
        },
        options: {
          fetcher: typeof fetch
          sleep: () => Promise<void>
          diagnostics: {
            beginOperation(): string
            record(event: Record<string, unknown>): void
          }
        }
      ) => Promise<{ audio: Blob }>)
    | undefined
  assert.equal(typeof synthesizeCloudSpeech, "function")

  const calls: Array<{
    url: string
    method: string
    body: string | null
    operationId: string | null
  }> = []
  const diagnosticEvents: Record<string, unknown>[] = []
  const fetcher: typeof fetch = async (input, init) => {
    const url = String(input)
    const headers = new Headers(init?.headers)
    calls.push({
      url,
      method: init?.method ?? "GET",
      body: typeof init?.body === "string" ? init.body : null,
      operationId: headers.get("x-readio-operation-id"),
    })
    if (url.endsWith("/v1/tts/jobs") && init?.method === "POST") {
      return Response.json({
        id: "job-1",
        status: "queued",
        createdAtUnixMs: 10,
      })
    }
    if (url.endsWith("/v1/tts/jobs/job-1")) {
      return Response.json({
        id: "job-1",
        status: "completed",
        createdAtUnixMs: 10,
        audioURL: "/v1/tts/jobs/job-1/audio",
      })
    }
    if (url.endsWith("/v1/tts/jobs/job-1/audio")) {
      return new Response(new Uint8Array([1, 2, 3]), {
        headers: { "content-type": "audio/mpeg" },
      })
    }
    return new Response(null, { status: 404 })
  }

  const result = await synthesizeCloudSpeech!(
    {
      modelId: "readio/auto",
      language: "en-US",
      text: "One Rust playback core.",
      voiceId: "English_expressive_narrator",
      speed: 1,
    },
    {
      fetcher,
      sleep: async () => undefined,
      diagnostics: {
        beginOperation: () => "op-web-tts-1",
        record: (event) => diagnosticEvents.push(event),
      },
    }
  )

  assert.deepEqual(calls, [
    {
      url: "/api/cloud/v1/tts/jobs",
      method: "POST",
      body: JSON.stringify({
        modelId: "readio/auto",
        language: "en-US",
        text: "One Rust playback core.",
        voiceId: "English_expressive_narrator",
        speed: 1,
      }),
      operationId: "op-web-tts-1",
    },
    {
      url: "/api/cloud/v1/tts/jobs/job-1",
      method: "GET",
      body: null,
      operationId: "op-web-tts-1",
    },
    {
      url: "/api/cloud/v1/tts/jobs/job-1/audio",
      method: "GET",
      body: null,
      operationId: "op-web-tts-1",
    },
  ])
  assert.deepEqual(
    [...new Uint8Array(await result.audio.arrayBuffer())],
    [1, 2, 3]
  )
  assert.deepEqual(
    diagnosticEvents.map((event) => event.name),
    ["job_requested", "job_queued", "job_completed", "audio_downloaded"]
  )
  assert.ok(
    diagnosticEvents.every(
      (event) => event.operationId === "op-web-tts-1"
    )
  )
  assert.doesNotMatch(JSON.stringify(diagnosticEvents), /One Rust playback core/)
})
