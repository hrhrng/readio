import { Sentence } from "./types";

// Two-branch sentence boundary regex:
//   Branch 1 (English): lookbehind .!? + consume whitespace
//   Branch 2 (CJK): lookbehind 。！？ + zero-width lookahead for CJK/word char
//     (positive lookahead avoids splitting before closing quotes like "你好。"她说)
export const BOUNDARY =
  /(?<=[.!?])\s+|(?<=[\u3002\uff01\uff1f])(?=[\u2E80-\u9FFF\uAC00-\uD7AF\uF900-\uFAFF\w])/;

export function extractSentencesFromText(text: string): Sentence[] {
  const sentences: Sentence[] = [];
  const parts = text.split(BOUNDARY).filter((s) => s.trim().length > 0);
  for (const part of parts) {
    sentences.push({ index: sentences.length, text: part.trim() });
  }
  return sentences;
}

export function wrapSentencesInDOM(
  container: HTMLElement,
  startIndex: number
): Sentence[] {
  const sentences: Sentence[] = [];
  let currentIndex = startIndex;

  const walker = document.createTreeWalker(
    container,
    NodeFilter.SHOW_TEXT,
    null
  );

  const textNodes: Text[] = [];
  let node: Text | null;
  while ((node = walker.nextNode() as Text | null)) {
    if (node.textContent && node.textContent.trim()) {
      textNodes.push(node);
    }
  }

  for (const textNode of textNodes) {
    const text = textNode.textContent || "";
    const parts = text.split(BOUNDARY).filter((s) => s.length > 0);

    if (parts.length <= 1 && text.trim()) {
      const span = document.createElement("span");
      span.setAttribute("data-sentence-id", String(currentIndex));
      span.textContent = text;
      textNode.parentNode?.replaceChild(span, textNode);
      sentences.push({ index: currentIndex, text: text.trim() });
      currentIndex++;
      continue;
    }

    const fragment = document.createDocumentFragment();
    for (const part of parts) {
      if (!part.trim()) {
        fragment.appendChild(document.createTextNode(part));
        continue;
      }
      const span = document.createElement("span");
      span.setAttribute("data-sentence-id", String(currentIndex));
      span.textContent = part;
      fragment.appendChild(span);
      sentences.push({ index: currentIndex, text: part.trim() });
      currentIndex++;
    }
    textNode.parentNode?.replaceChild(fragment, textNode);
  }

  return sentences;
}
