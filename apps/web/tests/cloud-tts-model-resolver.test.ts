import assert from "node:assert/strict"
import test from "node:test"

import type { TTSModelCatalog } from "../lib/rust-core.ts"

const catalog: TTSModelCatalog = {
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

test("cloud TTS model resolution stays lazy and reuses the Rust-normalized catalog", async () => {
  const modulePath = `../lib/${"cloud-tts-model-resolver"}.ts`
  const resolverModule = await import(modulePath).catch(() => ({}))
  const createCloudTTSModelResolver = Reflect.get(
    resolverModule,
    "createCloudTTSModelResolver"
  ) as
    | ((loadCatalog: () => Promise<TTSModelCatalog>) => {
        resolve(input: {
          language: string
          voiceId: string | null
        }): Promise<{ modelId: string; language: string; voiceId: string }>
      })
    | undefined
  assert.equal(typeof createCloudTTSModelResolver, "function")

  let loadCount = 0
  const resolver = createCloudTTSModelResolver!(async () => {
    loadCount += 1
    return catalog
  })

  assert.equal(loadCount, 0)
  assert.deepEqual(
    await resolver.resolve({ language: "zh-Hans-CN", voiceId: null }),
    {
      modelId: "readio/auto",
      language: "zh-Hans-CN",
      voiceId: "default",
    }
  )
  assert.deepEqual(
    await resolver.resolve({ language: "es", voiceId: null }),
    {
      modelId: "readio/auto",
      language: "es",
      voiceId: "default",
    }
  )
  assert.equal(loadCount, 1)
})

test("cloud TTS model resolution chooses the server model that owns a saved voice", async () => {
  const { createCloudTTSModelResolver } = await import(
    "../lib/cloud-tts-model-resolver.ts"
  )
  const resolver = createCloudTTSModelResolver(async () => catalog)

  assert.deepEqual(
    await resolver.resolve({
      language: "en",
      voiceId: "English_expressive_narrator",
    }),
    {
      modelId: "minimax/speech-2.6-hd",
      language: "en",
      voiceId: "English_expressive_narrator",
    }
  )
})

test("cloud TTS model resolution retries the catalog after a transient load failure", async () => {
  const { createCloudTTSModelResolver } = await import(
    "../lib/cloud-tts-model-resolver.ts"
  )
  let loadCount = 0
  const resolver = createCloudTTSModelResolver(async () => {
    loadCount += 1
    if (loadCount === 1) throw new Error("catalog temporarily unavailable")
    return catalog
  })

  await assert.rejects(
    resolver.resolve({ language: "en", voiceId: null }),
    /catalog temporarily unavailable/
  )
  assert.deepEqual(
    await resolver.resolve({ language: "en", voiceId: null }),
    {
      modelId: "readio/auto",
      language: "en",
      voiceId: "default",
    }
  )
  assert.equal(loadCount, 2)
})
