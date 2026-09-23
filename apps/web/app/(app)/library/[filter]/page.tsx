"use client"

import { use } from "react"
import { BookOpen, CheckCircle, Clock } from "lucide-react"
import { LibraryListingPage } from "@/components/library-listing-page"
import { useLocale } from "@/components/locale-provider"

const filterConfig = {
  all: {
    titleKey: "nav.library",
    descriptionKey: "library.allDescription",
    icon: BookOpen,
  },
  "want-to-read": {
    titleKey: "nav.wantToRead",
    descriptionKey: "library.wantToReadDescription",
    icon: Clock,
    progressMin: 0,
    progressMax: 0,
  },
  finished: {
    titleKey: "nav.finished",
    descriptionKey: "library.finishedDescription",
    icon: CheckCircle,
    progressMin: 100,
    progressMax: 100,
  },
} as const

export default function LibraryFilterPage({
  params,
}: {
  params: Promise<{ filter: string }>
}) {
  const { t } = useLocale()
  const { filter } = use(params)
  const config =
    filterConfig[filter as keyof typeof filterConfig] ?? filterConfig.all
  const { titleKey, descriptionKey, ...listing } = config

  return (
    <LibraryListingPage
      {...listing}
      title={t(titleKey)}
      description={t(descriptionKey)}
    />
  )
}
