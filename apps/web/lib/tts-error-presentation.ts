/** Provider-aware recovery copy without leaking sentence text into the UI. */
export function presentCloudTTSFailure(
  sentenceIndex: number,
  reason: string
): string {
  const normalizedReason = reason.trim().replace(/[.!?]+$/u, "") || "unknown error"
  return `Cloud TTS failed for sentence ${sentenceIndex + 1}: ${normalizedReason}. Retry or choose another voice.`
}
