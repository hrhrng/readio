import { fetchCloudTTSModels } from "./cloud-tts-client"
import type { TTSModel, TTSModelCatalog } from "./rust-core"

export interface CloudTTSModelSelection {
  modelId: string
  language: string
  voiceId: string
}

export interface CloudTTSModelResolver {
  resolve(input: {
    language: string
    voiceId: string | null
  }): Promise<CloudTTSModelSelection>
}

export function createCloudTTSModelResolver(
  loadCatalog: () => Promise<TTSModelCatalog> = fetchCloudTTSModels
): CloudTTSModelResolver {
  let catalogPromise: Promise<TTSModelCatalog> | null = null

  const catalog = () => {
    catalogPromise ??= loadCatalog().catch((error: unknown) => {
      catalogPromise = null
      throw error
    })
    return catalogPromise
  }

  return {
    async resolve({ language, voiceId }) {
      const available = await catalog()
      const requestedVoice = voiceId?.trim() || null
      const voiceModel = requestedVoice
        ? available.models.find((model) =>
            model.voices.some((voice) => voice.id === requestedVoice)
          )
        : undefined
      const model: TTSModel | undefined =
        voiceModel ??
        available.models.find(
          (candidate) => candidate.id === available.defaultModelId
        )

      if (!model) throw new Error("No cloud TTS model is currently available")

      return {
        modelId: model.id,
        language: language.trim() || "en",
        voiceId: requestedVoice && voiceModel ? requestedVoice : model.defaultVoiceId,
      }
    },
  }
}

export const cloudTTSModelResolver = createCloudTTSModelResolver()
