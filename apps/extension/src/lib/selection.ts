const DEFAULT_MAX_TTS_CHARS = 5000

export function trimForTts(text: string, maxChars: number = DEFAULT_MAX_TTS_CHARS): string {
  const cleaned = text.trim()
  if (cleaned.length <= maxChars) {
    return cleaned
  }
  return cleaned.slice(0, maxChars)
}

export function pickPlayableText(selectedText: string, articleText: string): string {
  const selected = selectedText.trim()
  if (selected.length > 0) {
    return selected
  }
  return articleText.trim()
}
