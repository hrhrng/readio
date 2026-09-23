import { describe, expect, it } from "vitest"
import type { WorkersAIRequestOptions } from "../src/tts-providers"

interface SynthesisInput {
  modelId: string
  language: string
  text: string
  voiceId: string
  speed: number
}

interface SynthesisOptions {
  gatewayId: string
  minimax: {
    apiKey: string
    groupId: string
    baseURL: string
    modelId: string
  }
  runWorkersAI(
    model: string,
    input: Record<string, unknown>,
    options: WorkersAIRequestOptions,
  ): Promise<Response>
  fetcher: typeof fetch
}

interface SynthesisResult {
  audio: BodyInit
  contentType: string
  provider: string
  modelId: string
}

type SynthesizeTTS = (
  input: SynthesisInput,
  options: SynthesisOptions,
) => Promise<SynthesisResult>

describe("cloud TTS providers", () => {
  it("routes English automatic synthesis through Aura 2 and disables gateway caching", async () => {
    const modulePath = `../src/${"tts-providers"}.ts`
    const providerModule = await import(modulePath).catch(() => ({}))
    const synthesizeTTS = Reflect.get(providerModule, "synthesizeTTS") as
      | SynthesizeTTS
      | undefined
    expect(typeof synthesizeTTS).toBe("function")

    const requests: Array<{
      model: string
      input: Record<string, unknown>
      options: WorkersAIRequestOptions
    }> = []
    const result = await synthesizeTTS!(
      {
        modelId: "readio/auto",
        language: "en-US",
        text: "A private sentence.",
        voiceId: "default",
        speed: 1,
      },
      {
        gatewayId: "readio",
        minimax: {
          apiKey: "secret",
          groupId: "",
          baseURL: "https://api.minimax.io",
          modelId: "speech-2.6-hd",
        },
        async runWorkersAI(model, input, options) {
          requests.push({ model, input, options })
          return new Response(new Uint8Array([0x49, 0x44, 0x33]), {
            headers: { "content-type": "audio/mpeg" },
          })
        },
        async fetcher() {
          throw new Error("MiniMax must not run when Aura succeeds")
        },
      },
    )

    expect(requests).toEqual([
      {
        model: "@cf/deepgram/aura-2-en",
        input: {
          text: "A private sentence.",
          speaker: "luna",
          encoding: "mp3",
        },
        options: {
          gateway: { id: "readio", skipCache: true },
          returnRawResponse: true,
        },
      },
    ])
    expect(result).toMatchObject({
      contentType: "audio/mpeg",
      provider: "workers-ai",
      modelId: "workers-ai/aura-2-en",
    })
    expect([...new Uint8Array(await new Response(result.audio).arrayBuffer())]).toEqual([
      0x49,
      0x44,
      0x33,
    ])
  })

  it("adapts multilingual requests to the MeloTTS contract", async () => {
    const providerModule = await import("../src/tts-providers")
    const synthesizeTTS = Reflect.get(providerModule, "synthesizeTTS") as SynthesizeTTS
    const requests: Array<{
      model: string
      input: Record<string, unknown>
      options: WorkersAIRequestOptions
    }> = []

    const result = await synthesizeTTS(
      {
        modelId: "workers-ai/melotts",
        language: "zh-Hans-CN",
        text: "你好，世界。",
        voiceId: "default",
        speed: 1,
      },
      {
        gatewayId: "readio",
        minimax: {
          apiKey: "secret",
          groupId: "",
          baseURL: "https://api.minimax.io",
          modelId: "speech-2.6-hd",
        },
        async runWorkersAI(model, input, options) {
          requests.push({ model, input, options })
          return new Response(new Uint8Array([1, 2, 3]), {
            headers: { "content-type": "audio/mpeg" },
          })
        },
        async fetcher() {
          throw new Error("MiniMax must not run when MeloTTS succeeds")
        },
      },
    )

    expect(requests[0]).toEqual({
      model: "@cf/myshell-ai/melotts",
      input: { prompt: "你好，世界。", lang: "zh" },
      options: {
        gateway: { id: "readio", skipCache: true },
        returnRawResponse: true,
      },
    })
    expect(result).toMatchObject({
      provider: "workers-ai",
      modelId: "workers-ai/melotts",
      contentType: "audio/mpeg",
    })
  })

  it("falls back to MiniMax when the preferred Workers AI model rejects the request", async () => {
    const providerModule = await import("../src/tts-providers")
    const synthesizeTTS = Reflect.get(providerModule, "synthesizeTTS") as SynthesizeTTS
    const minimaxRequests: Array<{ url: string; body: unknown }> = []

    const result = await synthesizeTTS(
      {
        modelId: "readio/auto",
        language: "en",
        text: "Fallback sentence.",
        voiceId: "English_expressive_narrator",
        speed: 1.25,
      },
      {
        gatewayId: "readio",
        minimax: {
          apiKey: "secret",
          groupId: "group-a",
          baseURL: "https://api.minimax.io",
          modelId: "speech-2.6-hd",
        },
        async runWorkersAI() {
          return new Response("unavailable", { status: 503 })
        },
        async fetcher(input, init) {
          minimaxRequests.push({
            url: String(input),
            body: JSON.parse(String(init?.body)) as unknown,
          })
          return Response.json({
            data: { audio: "494433" },
            base_resp: { status_code: 0 },
          })
        },
      },
    )

    expect(minimaxRequests[0]).toMatchObject({
      url: "https://api.minimax.io/v1/t2a_v2?GroupId=group-a",
      body: {
        model: "speech-2.6-hd",
        text: "Fallback sentence.",
        voice_setting: {
          voice_id: "English_expressive_narrator",
          speed: 1.25,
        },
        audio_setting: { format: "mp3" },
      },
    })
    expect(result).toMatchObject({
      provider: "minimax",
      modelId: "minimax/speech-2.6-hd",
      contentType: "audio/mpeg",
    })
    expect([...new Uint8Array(await new Response(result.audio).arrayBuffer())]).toEqual([
      0x49,
      0x44,
      0x33,
    ])
  })

  it("runs an explicitly selected MiniMax model without probing Workers AI", async () => {
    const providerModule = await import("../src/tts-providers")
    const synthesizeTTS = Reflect.get(providerModule, "synthesizeTTS") as SynthesizeTTS
    let workersAICalls = 0

    const result = await synthesizeTTS(
      {
        modelId: "minimax/speech-2.6-hd",
        language: "zh-CN",
        text: "直接使用已选模型。",
        voiceId: "Chinese (Mandarin)_Gentleman",
        speed: 1,
      },
      {
        gatewayId: "readio",
        minimax: {
          apiKey: "secret",
          groupId: "",
          baseURL: "https://api.minimax.io",
          modelId: "speech-2.6-hd",
        },
        async runWorkersAI() {
          workersAICalls += 1
          throw new Error("Workers AI should not be probed")
        },
        async fetcher() {
          return Response.json({
            data: { audio: "494433" },
            base_resp: { status_code: 0 },
          })
        },
      },
    )

    expect(workersAICalls).toBe(0)
    expect(result).toMatchObject({
      provider: "minimax",
      modelId: "minimax/speech-2.6-hd",
    })
  })

  it.each([
    {
      modelId: "workers-ai/aura-1",
      language: "en",
      voiceId: "angus",
      upstream: "@cf/deepgram/aura-1",
    },
    {
      modelId: "workers-ai/aura-2-es",
      language: "es-ES",
      voiceId: "sirio",
      upstream: "@cf/deepgram/aura-2-es",
    },
  ])("executes the advertised $modelId model", async (fixture) => {
    const providerModule = await import("../src/tts-providers")
    const synthesizeTTS = Reflect.get(providerModule, "synthesizeTTS") as SynthesizeTTS
    const requests: Array<{ model: string; input: Record<string, unknown> }> = []

    const result = await synthesizeTTS(
      {
        modelId: fixture.modelId,
        language: fixture.language,
        text: "Catalog models must be executable.",
        voiceId: fixture.voiceId,
        speed: 1,
      },
      {
        gatewayId: "readio",
        minimax: {
          apiKey: "secret",
          groupId: "",
          baseURL: "https://api.minimax.io",
          modelId: "speech-2.6-hd",
        },
        async runWorkersAI(model, input) {
          requests.push({ model, input })
          return new Response(new Uint8Array([1]))
        },
        async fetcher() {
          throw new Error("MiniMax must not run")
        },
      },
    )

    expect(requests).toEqual([
      {
        model: fixture.upstream,
        input: {
          text: "Catalog models must be executable.",
          speaker: fixture.voiceId,
          encoding: "mp3",
        },
      },
    ])
    expect(result.modelId).toBe(fixture.modelId)
  })
})
