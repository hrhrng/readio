/**
 * CJK (Chinese/Japanese/Korean) text detection and tokenization utilities.
 *
 * Used by sentence splitting and word-level highlighting to handle CJK text
 * where words are not separated by whitespace. Each CJK character becomes
 * an individual token for highlighting, while Latin/non-CJK runs remain
 * grouped as whole words. Whitespace is preserved as separate tokens.
 */

// CJK Unified Ideographs + Extension A/B, CJK Compatibility Ideographs,
// Hangul Syllables, Katakana, Hiragana
const CJK_CHAR =
  /[\u2E80-\u2FFF\u3040-\u309F\u30A0-\u30FF\u3400-\u4DBF\u4E00-\u9FFF\uAC00-\uD7AF\uF900-\uFAFF]/;

/**
 * Returns true if the text contains any CJK ideograph/syllable character.
 */
export function containsCJK(text: string): boolean {
  return CJK_CHAR.test(text);
}

// Tokenizer regex — matches one of three alternatives per token:
//   1. whitespace run  →  preserved as-is (for rendering spacing)
//   2. single CJK char →  each ideograph is its own token
//   3. non-CJK, non-whitespace run  →  grouped as a Latin "word"
const TOKEN_RE =
  /(\s+)|([\u2E80-\u2FFF\u3040-\u309F\u30A0-\u30FF\u3400-\u4DBF\u4E00-\u9FFF\uAC00-\uD7AF\uF900-\uFAFF])|([^\s\u2E80-\u2FFF\u3040-\u309F\u30A0-\u30FF\u3400-\u4DBF\u4E00-\u9FFF\uAC00-\uD7AF\uF900-\uFAFF]+)/g;

/**
 * Split text into tokens suitable for word-level TTS highlighting.
 *
 * - CJK characters → one token per character (character-by-character sweep)
 * - Latin/other words → one token per whitespace-delimited word
 * - Whitespace → preserved as separate tokens (for rendering spacing)
 *
 * For Chinese "这是句子" → ["这", "是", "句", "子"]
 * For English "Hello world" → ["Hello", " ", "world"]
 * For mixed "Hello 你好 world" → ["Hello", " ", "你", "好", " ", "world"]
 */
export function tokenize(text: string): string[] {
  const tokens: string[] = [];
  let match: RegExpExecArray | null;
  TOKEN_RE.lastIndex = 0;
  while ((match = TOKEN_RE.exec(text)) !== null) {
    tokens.push(match[0]);
  }
  return tokens;
}

/**
 * Count the number of non-whitespace tokens in text.
 * Used to compute word progress ratio for TTS highlighting.
 */
export function countTokens(text: string): number {
  let count = 0;
  let match: RegExpExecArray | null;
  TOKEN_RE.lastIndex = 0;
  while ((match = TOKEN_RE.exec(text)) !== null) {
    // match[1] is the whitespace group — skip it
    if (!match[1]) count++;
  }
  return count;
}
