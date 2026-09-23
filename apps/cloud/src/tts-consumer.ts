import type { TTSQueueMessage } from "./cloud-tts-service"
import {
  synthesizeTTS,
  type TTSSynthesisOptions,
} from "./tts-providers"

export interface TTSConsumerEnv {
  DB: D1Database
  MEDIA: R2Bucket
  MINIMAX_API_KEY: string
  MINIMAX_GROUP_ID: string
  MINIMAX_BASE_URL: string
  MINIMAX_MODEL_ID: string
  AI_GATEWAY_ID: string
}

interface TTSInputRow {
  status: "queued" | "running" | "completed" | "failed"
  input_text: string | null
  requested_model_id: string
  language: string
  voice_id: string
  speed: number
}

export async function processTTSJob(
  env: TTSConsumerEnv,
  message: TTSQueueMessage,
  fetcher: typeof fetch = fetch,
  now: () => number = Date.now,
  runWorkersAI: TTSSynthesisOptions["runWorkersAI"] = async () => {
    throw new Error("workers_ai_binding_missing")
  },
): Promise<void> {
  const row = await env.DB.prepare(
    `SELECT status, input_text, requested_model_id, language, voice_id, speed
       FROM tts_jobs
      WHERE id = ? AND user_id = ?`,
  )
    .bind(message.jobId, message.userId)
    .first<TTSInputRow>()
  if (!row || row.status === "completed") return
  if (!row.input_text) throw new Error("tts_input_missing")

  await env.DB.prepare(
    `UPDATE tts_jobs
        SET status = 'running', error_code = NULL, updated_at_unix_ms = ?
      WHERE id = ? AND user_id = ?`,
  )
    .bind(now(), message.jobId, message.userId)
    .run()

  try {
    const synthesis = await synthesizeTTS(
      {
        modelId: row.requested_model_id,
        language: row.language,
        text: row.input_text,
        voiceId: row.voice_id,
        speed: row.speed,
      },
      {
        gatewayId: env.AI_GATEWAY_ID,
        minimax: {
          apiKey: env.MINIMAX_API_KEY,
          groupId: env.MINIMAX_GROUP_ID,
          baseURL: env.MINIMAX_BASE_URL,
          modelId: env.MINIMAX_MODEL_ID,
        },
        runWorkersAI,
        fetcher,
      },
    )
    const audioKey = `tts/${message.userId}/${message.jobId}.mp3`

    await env.MEDIA.put(audioKey, synthesis.audio, {
      httpMetadata: { contentType: synthesis.contentType },
      customMetadata: {
        provider: synthesis.provider,
        model: synthesis.modelId,
      },
    })
    await env.DB.prepare(
      `UPDATE tts_jobs
          SET status = 'completed', input_text = NULL, audio_key = ?,
              provider = ?, resolved_model_id = ?, error_code = NULL,
              updated_at_unix_ms = ?
        WHERE id = ? AND user_id = ?`,
    )
      .bind(
        audioKey,
        synthesis.provider,
        synthesis.modelId,
        now(),
        message.jobId,
        message.userId,
      )
      .run()
  } catch (caught) {
    await env.DB.prepare(
      `UPDATE tts_jobs
          SET status = 'failed', error_code = 'provider_failed',
              updated_at_unix_ms = ?
        WHERE id = ? AND user_id = ?`,
    )
      .bind(now(), message.jobId, message.userId)
      .run()
    throw caught
  }
}

export async function consumeTTSBatch(
  batch: MessageBatch<TTSQueueMessage>,
  env: TTSConsumerEnv,
  runWorkersAI?: TTSSynthesisOptions["runWorkersAI"],
): Promise<void> {
  for (const message of batch.messages) {
    try {
      await processTTSJob(env, message.body, fetch, Date.now, runWorkersAI)
      message.ack()
    } catch {
      if (message.attempts < 3) {
        message.retry({ delaySeconds: Math.min(60, 2 ** message.attempts) })
      } else {
        message.ack()
      }
    }
  }
}
