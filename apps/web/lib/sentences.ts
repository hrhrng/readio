/**
 * Restore only the whitespace that an English sentence boundary consumes.
 *
 * Sentence text is trimmed deliberately because it is also the stable input
 * for search, bookmarks, and TTS. Renderers therefore add this separator
 * outside the interactive sentence span instead of mutating that shared text.
 */
export function visualSentenceSeparator(
  previous: string | null,
  next: string
): "" | " " {
  const previousText = previous?.trimEnd();
  if (!previousText || !next.trimStart()) return "";

  return /[.!?]$/.test(previousText) ? " " : "";
}
