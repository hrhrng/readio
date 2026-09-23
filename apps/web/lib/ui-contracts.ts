export type PageKind = "home" | "library" | "search" | "reader"
export type PageFrame = "overview" | "list" | "reader"

export interface PageLayoutContract {
  frame: PageFrame
  density: "compact" | "comfortable"
  header: "sticky" | "fixed"
  alignment: "edges" | "x-center"
}

const pageLayouts: Record<PageKind, PageLayoutContract> = {
  home: {
    frame: "overview",
    density: "comfortable",
    header: "sticky",
    alignment: "edges",
  },
  library: {
    frame: "overview",
    density: "comfortable",
    header: "sticky",
    alignment: "edges",
  },
  search: {
    frame: "list",
    density: "compact",
    header: "sticky",
    alignment: "edges",
  },
  reader: {
    frame: "reader",
    density: "comfortable",
    header: "fixed",
    alignment: "x-center",
  },
}

export function resolvePageLayout(page: PageKind): PageLayoutContract {
  return pageLayouts[page]
}

export type FeedbackEvent =
  | "library.imported"
  | "library.import_failed"
  | "library.deleted"
  | "library.delete_failed"
  | "auth.failed"
  | "settings.failed"
  | "tts.failed"
  | "bookmark.saved"
  | "bookmark.deleted"
  | "bookmark.delete_requested"
  | "bookmark.failed"

export interface FeedbackContract {
  tone: "neutral" | "success" | "danger"
  surface: "toast" | "inline" | "banner" | "alert-dialog"
  politeness: "polite" | "assertive"
}

const feedbackContracts: Record<FeedbackEvent, FeedbackContract> = {
  "library.imported": {
    tone: "success",
    surface: "toast",
    politeness: "polite",
  },
  "library.import_failed": {
    tone: "danger",
    surface: "inline",
    politeness: "assertive",
  },
  "library.deleted": {
    tone: "neutral",
    surface: "toast",
    politeness: "polite",
  },
  "library.delete_failed": {
    tone: "danger",
    surface: "toast",
    politeness: "assertive",
  },
  "auth.failed": {
    tone: "danger",
    surface: "inline",
    politeness: "assertive",
  },
  "settings.failed": {
    tone: "danger",
    surface: "toast",
    politeness: "assertive",
  },
  "tts.failed": {
    tone: "danger",
    surface: "banner",
    politeness: "assertive",
  },
  "bookmark.saved": {
    tone: "success",
    surface: "toast",
    politeness: "polite",
  },
  "bookmark.deleted": {
    tone: "neutral",
    surface: "toast",
    politeness: "polite",
  },
  "bookmark.delete_requested": {
    tone: "danger",
    surface: "alert-dialog",
    politeness: "assertive",
  },
  "bookmark.failed": {
    tone: "danger",
    surface: "inline",
    politeness: "assertive",
  },
}

export function resolveFeedback(event: FeedbackEvent): FeedbackContract {
  return feedbackContracts[event]
}
