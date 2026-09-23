import { env } from "cloudflare:workers"
import { beforeEach, describe, expect, it } from "vitest"

import { processTTSJob } from "../src/tts-consumer"

async function insertFixture() {
  await env.DB.prepare(
    `INSERT INTO user
       (id, name, email, email_verified, created_at, updated_at)
     VALUES ('reader-a', 'A', 'a@example.com', 1, 1, 1)`,
  ).run()
  await env.DB.prepare(
    `INSERT INTO tts_jobs
       (id, user_id, status, input_text, requested_model_id, language,
        input_characters, voice_id, speed, operation_id,
        created_at_unix_ms, updated_at_unix_ms)
     VALUES ('tts-1', 'reader-a', 'queued', 'Private sentence.',
             'minimax/speech-2.6-hd', 'en', 17, 'voice-1', 1.25,
             'op-web-tts-1', 1, 1)`,
  ).run()
}

describe("TTS queue consumer", () => {
  beforeEach(async () => {
    await env.DB.exec("DELETE FROM tts_jobs; DELETE FROM user;")
    await env.MEDIA.delete("tts/reader-a/tts-1.mp3")
    await insertFixture()
  })

  it("stores generated audio in R2 and erases private input after completion", async () => {
    const requests: Array<{ url: string; body: unknown }> = []
    const fetcher: typeof fetch = async (input, init) => {
      requests.push({
        url: String(input),
        body: JSON.parse(String(init?.body)) as unknown,
      })
      return Response.json({
        data: { audio: "494433" },
        base_resp: { status_code: 0 },
      })
    }

    await processTTSJob(
      {
        DB: env.DB,
        MEDIA: env.MEDIA,
        MINIMAX_API_KEY: "secret",
        MINIMAX_GROUP_ID: "group-a",
        MINIMAX_BASE_URL: "https://api.minimax.io",
        MINIMAX_MODEL_ID: "speech-2.6-hd",
        AI_GATEWAY_ID: "readio",
      },
      { jobId: "tts-1", userId: "reader-a", operationId: "op-web-tts-1" },
      fetcher,
      () => 2_000,
    )

    expect(requests[0].url).toContain("/v1/t2a_v2")
    expect(requests[0].body).toMatchObject({
      text: "Private sentence.",
      voice_setting: { voice_id: "voice-1", speed: 1.25 },
    })
    const row = await env.DB.prepare(
      "SELECT status, input_text, audio_key FROM tts_jobs WHERE id = 'tts-1'",
    ).first<{ status: string; input_text: string | null; audio_key: string | null }>()
    expect(row).toEqual({
      status: "completed",
      input_text: null,
      audio_key: "tts/reader-a/tts-1.mp3",
    })
    expect(await (await env.MEDIA.get("tts/reader-a/tts-1.mp3"))?.text()).toBe("ID3")
  })

  it("executes automatic jobs through Workers AI and records billable provider facts", async () => {
    await env.DB.prepare(
      `UPDATE tts_jobs
          SET requested_model_id = 'readio/auto', language = 'en-US',
              voice_id = 'luna'
        WHERE id = 'tts-1'`,
    ).run()
    const workersAIRequests: Array<{
      model: string
      input: Record<string, unknown>
      options: Record<string, unknown>
    }> = []
    await Reflect.apply(processTTSJob, null, [
      {
        DB: env.DB,
        MEDIA: env.MEDIA,
        MINIMAX_API_KEY: "secret",
        MINIMAX_GROUP_ID: "",
        MINIMAX_BASE_URL: "https://api.minimax.io",
        MINIMAX_MODEL_ID: "speech-2.6-hd",
        AI_GATEWAY_ID: "readio",
      },
      { jobId: "tts-1", userId: "reader-a", operationId: "op-web-tts-1" },
      async () =>
        Response.json({
          data: { audio: "4d494e494d4158" },
          base_resp: { status_code: 0 },
        }),
      () => 3_000,
      async (
        model: string,
        input: Record<string, unknown>,
        options: Record<string, unknown>,
      ) => {
        workersAIRequests.push({ model, input, options })
        return new Response(new Uint8Array([0x49, 0x44, 0x33]))
      },
    ])

    expect(workersAIRequests).toHaveLength(1)
    expect(await (await env.MEDIA.get("tts/reader-a/tts-1.mp3"))?.text()).toBe("ID3")
    expect(await env.MEDIA.head("tts/reader-a/tts-1.mp3")).toMatchObject({
      customMetadata: {
        provider: "workers-ai",
        model: "workers-ai/aura-2-en",
      },
    })
    expect(
      await env.DB.prepare(
        `SELECT status, input_text, provider, resolved_model_id
           FROM tts_jobs
          WHERE id = 'tts-1'`,
      ).first(),
    ).toEqual({
      status: "completed",
      input_text: null,
      provider: "workers-ai",
      resolved_model_id: "workers-ai/aura-2-en",
    })
  })
})
