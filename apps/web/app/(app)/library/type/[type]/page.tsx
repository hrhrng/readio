"use client"

import { use } from "react"
import { Book, FileText, Globe, Type } from "lucide-react"
import { LibraryListingPage } from "@/components/library-listing-page"
import { useLocale } from "@/components/locale-provider"
import type { LibraryItemType } from "@/lib/types"

const typeConfig = {
  web: {
    titleKey: "library.webTitle",
    descriptionKey: "library.webDescription",
    icon: Globe,
  },
  pdf: {
    titleKey: "library.pdfTitle",
    descriptionKey: "library.pdfDescription",
    icon: FileText,
  },
  epub: {
    titleKey: "library.epubTitle",
    descriptionKey: "library.epubDescription",
    icon: Book,
  },
  txt: {
    titleKey: "library.textTitle",
    descriptionKey: "library.textDescription",
    icon: Type,
  },
} as const

export default function LibraryTypePage({
  params,
}: {
  params: Promise<{ type: string }>
}) {
  const { t } = useLocale()
  const { type } = use(params)
  const itemType = type as LibraryItemType
  const config = typeConfig[itemType]
  const normalizedType = type.toUpperCase()

  return (
    <LibraryListingPage
      icon={config?.icon ?? FileText}
      title={
        config
          ? t(config.titleKey)
          : t("library.genericTypeTitle", { type: normalizedType })
      }
      description={
        config
          ? t(config.descriptionKey)
          : t("library.genericTypeDescription", { type: normalizedType })
      }
      type={itemType}
    />
  )
}
