import { env } from "cloudflare:workers"
import { beforeEach, describe, expect, it } from "vitest"

import {
  CloudTTSService,
  type TTSQueueMessage,
} from "../src/cloud-tts-service"

const messages: TTSQueueMessage[] = []
const queue = {
  async send(message: TTSQueueMessage) {
    messages.push(message)
    return { metadata: { metrics: {} } }
  },
} as unknown as Queue<TTSQueueMessage>

async function insertUser(id: string) {
  await env.DB.prepare(
    `INSERT OR IGNORE INTO user
       (id, name, email, email_verified, created_at, updated_at)
     VALUES (?, ?, ?, 1, ?, ?)`,
  )
    .bind(id, id, `${id}@example.com`, 1, 1)
    .run()
}

describe("cloud TTS jobs", () => {
  beforeEach(async () => {
    messages.length = 0
    await env.DB.exec("DELETE FROM tts_jobs; DELETE FROM user;")
    await insertUser("reader-a")
    await insertUser("reader-b")
  })

  it("persists private input, queues only an opaque scoped job, and never crosses users", async () => {
    const service = new CloudTTSService(
      env.DB,
      queue,
      env.MEDIA,
      () => 1_000,
      () => "tts-1",
    )
    const created = await service.create(
      "reader-a",
      {
        modelId: "workers-ai/aura-2-en",
        language: "en-US",
        text: "Private sentence.",
        voiceId: "luna",
        speed: 1,
      },
      "op-web-tts-1",
    )

    expect(created).toEqual({
      id: "tts-1",
      status: "queued",
      createdAtUnixMs: 1_000,
      operationId: "op-web-tts-1",
    })
    expect(messages).toEqual([
      { jobId: "tts-1", userId: "reader-a", operationId: "op-web-tts-1" },
    ])
    await expect(service.get("reader-b", "tts-1")).resolves.toBeNull()
    await expect(service.get("reader-a", "tts-1")).resolves.toEqual(created)

    const row = await env.DB.prepare(
      `SELECT input_text, requested_model_id, language, input_characters, operation_id
         FROM tts_jobs
        WHERE id = ?`,
    )
      .bind("tts-1")
      .first<{
        input_text: string
        requested_model_id: string
        language: string
        input_characters: number
        operation_id: string
      }>()
    expect(row).toEqual({
      input_text: "Private sentence.",
      requested_model_id: "workers-ai/aura-2-en",
      language: "en-US",
      input_characters: 17,
      operation_id: "op-web-tts-1",
    })
  })

  it("publishes the ordered model catalog that clients normalize", async () => {
    const service = new CloudTTSService(env.DB, queue, env.MEDIA)
    const catalog = await service.models()

    expect(catalog.schemaVersion).toBe(1)
    expect(catalog.defaultModelId).toBe("readio/auto")
    expect(catalog.models.map((model) => model.id)).toEqual([
      "readio/auto",
      "workers-ai/aura-1",
      "workers-ai/aura-2-en",
      "workers-ai/aura-2-es",
      "workers-ai/melotts",
      "minimax/speech-2.6-hd",
    ])
    expect(catalog.models.find((model) => model.id === "workers-ai/aura-2-en"))
      .toMatchObject({
        provider: "workers-ai",
        languages: ["en"],
        defaultVoiceId: "luna",
      })
  })
})
