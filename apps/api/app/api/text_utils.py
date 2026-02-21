"""
CJK-aware text splitting utilities for TTS processing.

Handles Chinese/Japanese/Korean text where:
- Sentences are not separated by whitespace after punctuation marks (。！？)
- Words are not separated by spaces, so `str.split()` returns a single element
"""

import re

# CJK Unified Ideographs, Extension A/B, Compatibility Ideographs,
# Hangul Syllables, Katakana, Hiragana
_CJK_RANGE = r'\u2E80-\u2FFF\u3040-\u309F\u30A0-\u30FF\u3400-\u4DBF\u4E00-\u9FFF\uAC00-\uD7AF\uF900-\uFAFF'
_CJK_RE = re.compile(f'[{_CJK_RANGE}]')

# Sentence boundary: two branches united by alternation.
#   Branch 1 (English): lookbehind for .!? followed by mandatory whitespace.
#   Branch 2 (CJK): lookbehind for 。！？ followed by lookahead for a CJK char
#     or word char. Zero-width split — no whitespace consumed. The positive
#     lookahead prevents splitting before closing quotes like "你好。"她说。
_SENTENCE_BOUNDARY = re.compile(
    r'(?<=[.!?])\s+'
    r'|'
    rf'(?<=[。！？])(?=[{_CJK_RANGE}\w])'
)

# CJK clause-level punctuation for sub-sentence splitting
_CJK_CLAUSE_PUNCT = re.compile(r'(?<=[，、；：])')


def contains_cjk(text: str) -> bool:
    """Return True if text contains any CJK character."""
    return bool(_CJK_RE.search(text))


def split_sentences(text: str) -> list[str]:
    """
    Split text into sentences, aware of both English and CJK boundaries.

    Splits on newlines first, then on sentence-ending punctuation.
    """
    # Split on newlines first
    paragraphs = re.split(r'\n+', text.strip())
    sentences: list[str] = []
    for para in paragraphs:
        para = para.strip()
        if not para:
            continue
        parts = _SENTENCE_BOUNDARY.split(para)
        for part in parts:
            part = part.strip()
            if part:
                sentences.append(part)
    return sentences


def split_cjk_clauses(text: str, max_chars: int) -> list[str]:
    """
    Split a CJK text chunk on clause-level punctuation (，、；：),
    accumulating clauses up to max_chars. Only hard-slices individual
    clauses that are still too long as a last resort.
    """
    # Split on clause punctuation while keeping the delimiter attached
    # to the preceding clause
    clauses = _CJK_CLAUSE_PUNCT.split(text)

    # The split above separates after the punctuation, but the punctuation
    # stays at the end of each preceding segment. However re.split with a
    # zero-width assertion keeps segments together, so clauses already have
    # their trailing punctuation attached.

    result: list[str] = []
    current = ''

    for clause in clauses:
        if not clause:
            continue
        # If adding this clause would exceed the limit, flush current
        if current and len(current) + len(clause) > max_chars:
            result.append(current)
            current = ''
        # If a single clause exceeds max_chars, hard-slice it
        if len(clause) > max_chars:
            if current:
                result.append(current)
                current = ''
            for i in range(0, len(clause), max_chars):
                result.append(clause[i:i + max_chars])
        else:
            current += clause

    if current:
        result.append(current)

    return result
