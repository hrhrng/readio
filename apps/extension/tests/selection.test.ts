import { describe, expect, it } from 'vitest'

import { pickPlayableText, trimForTts } from '../src/lib/selection'

describe('selection helpers', () => {
  it('prefers selected text when non-empty', () => {
    const result = pickPlayableText('  hello world  ', 'article fallback')
    expect(result).toBe('hello world')
  })

  it('falls back to article body when selection is empty', () => {
    const result = pickPlayableText('   ', '  article text   ')
    expect(result).toBe('article text')
  })

  it('trims long text to max size for tts calls', () => {
    const text = 'a'.repeat(100)
    expect(trimForTts(text, 12)).toBe('a'.repeat(12))
  })
})
