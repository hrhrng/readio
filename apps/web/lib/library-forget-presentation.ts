import type { ImportOwnership } from "./rust-core"
import { createTranslator, type SupportedLocale } from "./localization"

export interface LibraryForgetPresentation {
  title: string
  description: string
  actionLabel: string
  completion: string
}

/** Keep the destructive copy honest about whether Readio owns any bytes. */
export function resolveLibraryForgetPresentation(
  itemTitle: string,
  ownership: ImportOwnership | null | undefined,
  locale: SupportedLocale = "en"
): LibraryForgetPresentation {
  const t = createTranslator(locale)
  if (ownership === "link") {
    return {
      title: t("library.removeLinkedTitle"),
      description: t("library.removeLinkedMessage", { title: itemTitle }),
      actionLabel: t("common.remove"),
      completion: t("library.removedLinkedCompletion", { title: itemTitle }),
    }
  }

  if (ownership === "copy" || ownership === "move") {
    return {
      title: t("library.deleteCopyTitle"),
      description: t("library.deleteCopyMessage", { title: itemTitle }),
      actionLabel: t("common.delete"),
      completion: t("library.deletedCopyCompletion", { title: itemTitle }),
    }
  }

  return {
    title: t("library.removeLinkedTitle"),
    description: t("library.removeLegacyMessage", { title: itemTitle }),
    actionLabel: t("common.remove"),
    completion: t("library.removedLegacyCompletion", { title: itemTitle }),
  }
}
