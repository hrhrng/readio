export type SearchWalkDirection = -1 | 1
export type ReaderTool = "search" | "bookmarks"

export interface ReaderOverlayState {
  tocOpen: boolean
  activeTool: ReaderTool | null
}

export type ReaderOverlayAction =
  | { type: "open-tool"; tool: ReaderTool }
  | { type: "toggle-toc" }
  | { type: "close-tools" }
  | { type: "close-all" }

export function reduceReaderOverlay(
  state: ReaderOverlayState,
  action: ReaderOverlayAction
): ReaderOverlayState {
  switch (action.type) {
    case "open-tool":
      return { tocOpen: false, activeTool: action.tool }
    case "toggle-toc":
      return { tocOpen: !state.tocOpen, activeTool: null }
    case "close-tools":
      return { ...state, activeTool: null }
    case "close-all":
      return { tocOpen: false, activeTool: null }
  }
}

export type SearchWalkResolution =
  | { kind: "hit"; targetIndex: number }
  | { kind: "load"; cursor: string; targetIndex: number }

export function resolveSearchWalk({
  hitCount,
  activeIndex,
  direction,
  nextCursor,
}: {
  hitCount: number
  activeIndex: number | null
  direction: SearchWalkDirection
  nextCursor: string | null
}): SearchWalkResolution {
  const current = activeIndex ?? (direction > 0 ? -1 : hitCount)
  const targetIndex = current + direction

  if (direction > 0 && targetIndex >= hitCount && nextCursor) {
    return { kind: "load", cursor: nextCursor, targetIndex: hitCount }
  }
  if (targetIndex < 0) {
    return { kind: "hit", targetIndex: hitCount - 1 }
  }
  if (targetIndex >= hitCount) {
    return { kind: "hit", targetIndex: 0 }
  }
  return { kind: "hit", targetIndex }
}

export function shouldOpenReaderSearch({
  key,
  metaKey,
  ctrlKey,
  isEditable,
}: {
  key: string
  metaKey: boolean
  ctrlKey: boolean
  isEditable: boolean
}): boolean {
  return (
    !isEditable &&
    key.toLowerCase() === "f" &&
    (metaKey || ctrlKey)
  )
}
