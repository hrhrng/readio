export interface CloudTTSInput {
  modelId: string
  language: string
  text: string
  voiceId: string
  speed: number
}

export interface CloudTTSJob {
  id: string
  status: "queued" | "running" | "completed" | "failed"
  createdAtUnixMs: number
  audioURL?: string
  errorCode?: string
}

export interface CloudSpeechResult {
  job: CloudTTSJob
  audio: Blob
}

interface CloudErrorBody {
  error?: {
    code?: string
    message?: string
  }
}

export interface CloudTTSOptions {
  baseURL?: string
  fetcher?: typeof fetch
  sleep?: (milliseconds: number) => Promise<void>
  maxWaitMilliseconds?: number
  core?: Pick<RustCoreClient, "normalizeTTSModelCatalog">
  diagnostics?: Pick<WebDiagnosticLog, "beginOperation" | "record">
}

function endpoint(baseURL: string, path: string): string {
  return `${baseURL.replace(/\/+$/, "")}/${path.replace(/^\/+/, "")}`
}

async function cloudError(response: Response): Promise<Error> {
  const body = (await response.json().catch(() => null)) as CloudErrorBody | null
  return new Error(
    body?.error?.message ??
      (response.status === 401
        ? "Sign in to use cloud TTS."
        : `Cloud TTS request failed (${response.status})`)
  )
}

async function jobResponse(response: Response): Promise<CloudTTSJob> {
  if (!response.ok) throw await cloudError(response)
  return (await response.json()) as CloudTTSJob
}

export async function fetchCloudTTSModels(
  options: CloudTTSOptions = {}
): Promise<TTSModelCatalog> {
  const baseURL = options.baseURL ?? "/api/cloud/"
  const fetcher = options.fetcher ?? fetch
  const response = await fetcher(endpoint(baseURL, "/v1/tts/models"), {
    method: "GET",
    credentials: "include",
  })
  if (!response.ok) throw await cloudError(response)
  const raw: unknown = await response.json()
  return (options.core ?? webRustCore).normalizeTTSModelCatalog(raw)
}

export async function synthesizeCloudSpeech(
  input: CloudTTSInput,
  options: CloudTTSOptions = {}
): Promise<CloudSpeechResult> {
  const baseURL = options.baseURL ?? "/api/cloud/"
  const fetcher = options.fetcher ?? fetch
  const sleep =
    options.sleep ??
    ((milliseconds: number) =>
      new Promise<void>((resolve) => setTimeout(resolve, milliseconds)))
  const maxWait = options.maxWaitMilliseconds ?? 30_000
  const startedAt = Date.now()
  const diagnostics = options.diagnostics ?? webDiagnostics
  const operationId = diagnostics.beginOperation()
  const operationHeaders = { "x-readio-operation-id": operationId }

  diagnostics.record({
    operationId,
    component: "web-cloud-tts",
    category: "tts",
    name: "job_requested",
    fields: { provider: "cloud", model_id: input.modelId },
  })

  let job = await jobResponse(
    await fetcher(endpoint(baseURL, "/v1/tts/jobs"), {
      method: "POST",
      credentials: "include",
      headers: {
        "content-type": "application/json",
        ...operationHeaders,
      },
      body: JSON.stringify(input),
    })
  )
  diagnostics.record({
    operationId,
    component: "web-cloud-tts",
    category: "tts",
    name: `job_${job.status}`,
    fields: { provider: "cloud", model_id: input.modelId, job_id: job.id },
  })

  let delay = 200
  while (job.status === "queued" || job.status === "running") {
    if (Date.now() - startedAt >= maxWait) {
      throw new Error("Cloud TTS timed out. Please retry.")
    }
    await sleep(delay)
    delay = Math.min(2_000, Math.round(delay * 1.5))
    job = await jobResponse(
      await fetcher(endpoint(baseURL, `/v1/tts/jobs/${encodeURIComponent(job.id)}`), {
        method: "GET",
        credentials: "include",
        headers: operationHeaders,
      })
    )
    diagnostics.record({
      operationId,
      component: "web-cloud-tts",
      category: "tts",
      name: `job_${job.status}`,
      fields: { provider: "cloud", model_id: input.modelId, job_id: job.id },
    })
  }

  if (job.status === "failed") {
    throw new Error(job.errorCode ?? "Cloud TTS synthesis failed")
  }
  if (!job.audioURL) throw new Error("Cloud TTS completed without audio")

  const audioResponse = await fetcher(endpoint(baseURL, job.audioURL), {
    method: "GET",
    credentials: "include",
    headers: operationHeaders,
  })
  if (!audioResponse.ok) throw await cloudError(audioResponse)
  const audio = await audioResponse.blob()
  diagnostics.record({
    operationId,
    component: "web-cloud-tts",
    category: "tts",
    name: "audio_downloaded",
    fields: {
      provider: "cloud",
      model_id: input.modelId,
      job_id: job.id,
      duration_ms: Date.now() - startedAt,
    },
  })
  return { job, audio }
}
import { webRustCore } from "./readio-core-runtime"
import { webDiagnostics, type WebDiagnosticLog } from "./diagnostics"
import type { RustCoreClient, TTSModelCatalog } from "./rust-core"
