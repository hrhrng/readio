export interface TTSSynthesisInput {
  modelId: string
  language: string
  text: string
  voiceId: string
  speed: number
}

export interface MiniMaxProviderConfig {
  apiKey: string
  groupId: string
  baseURL: string
  modelId: string
}

export interface WorkersAIRequestOptions {
  gateway: {
    id: string
    skipCache: true
  }
  returnRawResponse: true
}

export interface TTSSynthesisOptions {
  gatewayId: string
  minimax: MiniMaxProviderConfig
  runWorkersAI(
    model: string,
    input: Record<string, unknown>,
    options: WorkersAIRequestOptions,
  ): Promise<Response>
  fetcher: typeof fetch
}

export interface SynthesizedAudio {
  audio: ReadableStream<Uint8Array> | Uint8Array
  contentType: string
  provider: string
  modelId: string
}

interface MiniMaxResponse {
  data?: { audio?: unknown }
  audio?: unknown
  base_resp?: { status_code?: unknown }
}

function decodeAudioPayload(value: string): Uint8Array {
  const normalized = value.trim()
  if (!normalized) throw new Error("minimax_audio_empty")

  const hex = normalized.startsWith("0x") ? normalized.slice(2) : normalized
  if (hex.length % 2 === 0 && /^[0-9a-f]+$/i.test(hex)) {
    const bytes = new Uint8Array(hex.length / 2)
    for (let index = 0; index < hex.length; index += 2) {
      bytes[index / 2] = Number.parseInt(hex.slice(index, index + 2), 16)
    }
    return bytes
  }

  try {
    const binary = atob(normalized)
    return Uint8Array.from(binary, (character) => character.charCodeAt(0))
  } catch {
    throw new Error("minimax_audio_encoding_invalid")
  }
}

async function synthesizeWithMiniMax(
  input: TTSSynthesisInput,
  options: TTSSynthesisOptions,
): Promise<SynthesizedAudio> {
  const base = options.minimax.baseURL.replace(/\/+$/, "")
  const endpoint = new URL(
    base.endsWith("/v1/t2a_v2") ? base : `${base}/v1/t2a_v2`,
  )
  if (options.minimax.groupId) {
    endpoint.searchParams.set("GroupId", options.minimax.groupId)
  }
  const response = await options.fetcher(endpoint, {
    method: "POST",
    headers: {
      authorization: `Bearer ${options.minimax.apiKey}`,
      "content-type": "application/json",
    },
    body: JSON.stringify({
      model: options.minimax.modelId,
      text: input.text,
      stream: false,
      voice_setting: {
        voice_id:
          input.modelId.startsWith("minimax/")
            ? input.voiceId
            : input.language.toLowerCase().startsWith("zh")
              ? "Chinese (Mandarin)_Gentleman"
              : "English_expressive_narrator",
        speed: input.speed,
        vol: 1,
        pitch: 0,
      },
      audio_setting: {
        sample_rate: 32_000,
        bitrate: 128_000,
        format: "mp3",
        channel: 1,
      },
      output_format: "hex",
      ...(options.minimax.groupId ? { group_id: options.minimax.groupId } : {}),
    }),
  })
  if (!response.ok) throw new Error("minimax_http_error")
  const body = (await response.json()) as MiniMaxResponse
  const status = body.base_resp?.status_code
  if (status !== undefined && status !== 0 && status !== "0") {
    throw new Error("minimax_provider_rejected")
  }
  const audio = body.data?.audio ?? body.audio
  if (typeof audio !== "string") throw new Error("minimax_audio_missing")
  return {
    audio: decodeAudioPayload(audio),
    contentType: "audio/mpeg",
    provider: "minimax",
    modelId: `minimax/${options.minimax.modelId}`,
  }
}

interface WorkersAIRequest {
  model: string
  input: Record<string, unknown>
  resolvedModelId: string
}

function workersAIRequest(
  input: TTSSynthesisInput,
  language: string,
): WorkersAIRequest {
  const selected =
    input.modelId === "readio/auto"
      ? language === "en"
        ? "workers-ai/aura-2-en"
        : language === "es"
          ? "workers-ai/aura-2-es"
          : "workers-ai/melotts"
      : input.modelId

  if (selected === "workers-ai/melotts") {
    return {
      model: "@cf/myshell-ai/melotts",
      input: { prompt: input.text, lang: language },
      resolvedModelId: selected,
    }
  }

  const upstream = {
    "workers-ai/aura-1": "@cf/deepgram/aura-1",
    "workers-ai/aura-2-en": "@cf/deepgram/aura-2-en",
    "workers-ai/aura-2-es": "@cf/deepgram/aura-2-es",
  }[selected]
  if (!upstream) throw new Error("tts_model_unavailable")
  const defaultSpeaker = {
    "workers-ai/aura-1": "angus",
    "workers-ai/aura-2-en": "luna",
    "workers-ai/aura-2-es": "sirio",
  }[selected]
  return {
    model: upstream,
    input: {
      text: input.text,
      speaker:
        input.voiceId === "default" ? defaultSpeaker : input.voiceId,
      encoding: "mp3",
    },
    resolvedModelId: selected,
  }
}

export async function synthesizeTTS(
  input: TTSSynthesisInput,
  options: TTSSynthesisOptions,
): Promise<SynthesizedAudio> {
  const language = input.language.trim().split(/[-_]/, 1)[0].toLowerCase() || "en"
  if (input.modelId.startsWith("minimax/")) {
    return synthesizeWithMiniMax(input, options)
  }
  const request = workersAIRequest(input, language)

  try {
    const response = await options.runWorkersAI(
      request.model,
      request.input,
      {
        gateway: { id: options.gatewayId, skipCache: true },
        returnRawResponse: true,
      },
    )
    if (response.ok && response.body) {
      return {
        audio: response.body,
        contentType: response.headers.get("content-type") ?? "audio/mpeg",
        provider: "workers-ai",
        modelId: request.resolvedModelId,
      }
    }
  } catch {
    // The ordered provider chain below owns the stable fallback behavior.
  }
  return synthesizeWithMiniMax(input, options)
}
