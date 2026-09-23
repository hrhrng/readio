import type {
  TTSJobInput,
  TTSJobRecord,
  TTSModelCatalog,
  TTSService,
} from "./app"

export interface TTSQueueMessage {
  jobId: string
  userId: string
  operationId: string
}

interface TTSJobRow {
  id: string
  operation_id: string
  status: TTSJobRecord["status"]
  audio_key: string | null
  error_code: string | null
  created_at_unix_ms: number
}

function publicJob(row: TTSJobRow): TTSJobRecord {
  return {
    id: row.id,
    operationId: row.operation_id,
    status: row.status,
    createdAtUnixMs: row.created_at_unix_ms,
    audioURL: row.audio_key ? `/v1/tts/jobs/${encodeURIComponent(row.id)}/audio` : undefined,
    errorCode: row.error_code ?? undefined,
  }
}

const MODEL_CATALOG: TTSModelCatalog = {
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
    {
      id: "workers-ai/aura-1",
      provider: "workers-ai",
      label: "Aura 1",
      languages: ["en"],
      voices: [
        { id: "angus", label: "Angus" },
        { id: "asteria", label: "Asteria" },
        { id: "athena", label: "Athena" },
        { id: "orion", label: "Orion" },
      ],
      defaultVoiceId: "angus",
    },
    {
      id: "workers-ai/aura-2-en",
      provider: "workers-ai",
      label: "Aura 2 English",
      languages: ["en"],
      voices: [
        { id: "luna", label: "Luna" },
        { id: "athena", label: "Athena" },
        { id: "asteria", label: "Asteria" },
        { id: "orion", label: "Orion" },
      ],
      defaultVoiceId: "luna",
    },
    {
      id: "workers-ai/aura-2-es",
      provider: "workers-ai",
      label: "Aura 2 Spanish",
      languages: ["es"],
      voices: [
        { id: "sirio", label: "Sirio" },
        { id: "carina", label: "Carina" },
        { id: "celeste", label: "Celeste" },
      ],
      defaultVoiceId: "sirio",
    },
    {
      id: "workers-ai/melotts",
      provider: "workers-ai",
      label: "MeloTTS",
      languages: ["*"],
      voices: [{ id: "default", label: "Default" }],
      defaultVoiceId: "default",
    },
    {
      id: "minimax/speech-2.6-hd",
      provider: "minimax",
      label: "MiniMax Speech 2.6 HD",
      languages: ["*"],
      voices: [
        {
          id: "English_expressive_narrator",
          label: "Expressive Narrator",
        },
      ],
      defaultVoiceId: "English_expressive_narrator",
    },
  ],
}

export class CloudTTSService implements TTSService {
  constructor(
    private readonly database: D1Database,
    private readonly queue: Queue<TTSQueueMessage>,
    private readonly media: R2Bucket,
    private readonly now: () => number = Date.now,
    private readonly createId: () => string = crypto.randomUUID,
  ) {}

  async models(): Promise<TTSModelCatalog> {
    return MODEL_CATALOG
  }

  async create(
    userId: string,
    input: TTSJobInput,
    operationId: string,
  ): Promise<TTSJobRecord> {
    const id = this.createId()
    const timestamp = this.now()
    await this.database
      .prepare(
        `INSERT INTO tts_jobs
           (id, user_id, status, input_text, requested_model_id, language,
            input_characters, voice_id, speed, operation_id,
            created_at_unix_ms, updated_at_unix_ms)
         VALUES (?, ?, 'queued', ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .bind(
        id,
        userId,
        input.text,
        input.modelId,
        input.language,
        [...input.text].length,
        input.voiceId,
        input.speed,
        operationId,
        timestamp,
        timestamp,
      )
      .run()

    try {
      await this.queue.send({ jobId: id, userId, operationId })
    } catch (caught) {
      await this.database
        .prepare(
          `UPDATE tts_jobs
              SET status = 'failed', error_code = 'queue_unavailable',
                  updated_at_unix_ms = ?
            WHERE id = ? AND user_id = ?`,
        )
        .bind(this.now(), id, userId)
        .run()
      throw caught
    }

    return {
      id,
      operationId,
      status: "queued",
      createdAtUnixMs: timestamp,
    }
  }

  async get(userId: string, jobId: string): Promise<TTSJobRecord | null> {
    const row = await this.database
      .prepare(
        `SELECT id, operation_id, status, audio_key, error_code, created_at_unix_ms
           FROM tts_jobs
          WHERE id = ? AND user_id = ?`,
      )
      .bind(jobId, userId)
      .first<TTSJobRow>()
    return row ? publicJob(row) : null
  }

  async audio(userId: string, jobId: string): Promise<Response | null> {
    const row = await this.database
      .prepare(
        `SELECT audio_key
           FROM tts_jobs
          WHERE id = ? AND user_id = ? AND status = 'completed'`,
      )
      .bind(jobId, userId)
      .first<{ audio_key: string | null }>()
    if (!row?.audio_key) return null

    const object = await this.media.get(row.audio_key)
    if (!object || !("body" in object)) return null
    const headers = new Headers({
      "cache-control": "private, no-store",
      etag: object.httpEtag,
    })
    object.writeHttpMetadata(headers)
    return new Response(object.body, { headers })
  }
}
