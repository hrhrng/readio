export type TtsProvider = 'auto' | 'edge' | 'elevenlabs' | 'minimax'

export type SynthesizePayload = {
  text: string
  speed: number
  voice?: string
  format: 'mp3'
  provider_options: Record<string, unknown>
}

export function buildSynthesizeUrl(apiBase: string, provider: TtsProvider): string {
  const normalizedBase = apiBase.replace(/\/$/, '')
  const endpoint = `${normalizedBase}/api/tts/synthesize`
  if (provider === 'auto') {
    return endpoint
  }
  return `${endpoint}?provider=${encodeURIComponent(provider)}`
}

export function buildStreamUrl(apiBase: string, provider: TtsProvider, maxChars: number = 220): string {
  const normalizedBase = apiBase.replace(/\/$/, '')
  const endpoint = `${normalizedBase}/api/tts/stream?max_chars=${encodeURIComponent(String(maxChars))}`
  if (provider === 'auto') {
    return endpoint
  }
  return `${endpoint}&provider=${encodeURIComponent(provider)}`
}

export function createSynthesizePayload(text: string, speed: number, voice?: string | null): SynthesizePayload {
  const normalizedVoice = typeof voice === 'string' ? voice.trim() : ''
  return {
    text,
    speed,
    ...(normalizedVoice ? { voice: normalizedVoice } : {}),
    format: 'mp3',
    provider_options: {},
  }
}

export type SynthesizeResponse = {
  provider: string
  audio_base64: string
  duration_ms: number | null
  sample_rate: number | null
  trace_id: string
}
