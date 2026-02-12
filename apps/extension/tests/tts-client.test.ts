import { describe, expect, it } from 'vitest'

import { buildStreamUrl, buildSynthesizeUrl, createSynthesizePayload } from '../src/lib/tts-client'

describe('tts client helpers', () => {
  it('builds fallback url when provider is auto', () => {
    expect(buildSynthesizeUrl('http://localhost:8000', 'auto')).toBe(
      'http://localhost:8000/api/tts/synthesize'
    )
  })

  it('builds forced provider url when provider is specified', () => {
    expect(buildSynthesizeUrl('http://localhost:8000/', 'minimax')).toBe(
      'http://localhost:8000/api/tts/synthesize?provider=minimax'
    )
  })

  it('builds stream url for auto provider', () => {
    expect(buildStreamUrl('http://localhost:8000', 'auto', 200)).toBe(
      'http://localhost:8000/api/tts/stream?max_chars=200'
    )
  })

  it('creates payload with expected defaults', () => {
    expect(createSynthesizePayload('hello', 1.25)).toEqual({
      text: 'hello',
      speed: 1.25,
      format: 'mp3',
      provider_options: {},
    })
  })

  it('adds voice field when provided', () => {
    expect(createSynthesizePayload('hello', 1.25, 'English_expressive_narrator')).toEqual({
      text: 'hello',
      speed: 1.25,
      voice: 'English_expressive_narrator',
      format: 'mp3',
      provider_options: {},
    })
  })
})
