"use client"

import Link from "next/link"
import { useLocale } from "./locale-provider"

const BUILTIN_SAMPLE_ITEM_ID = "readio:sample:v1"

export function BuiltinSampleLink() {
  const { t } = useLocale()
  return (
    <Link
      className="ui-button ui-button--primary"
      href={`/player/${encodeURIComponent(BUILTIN_SAMPLE_ITEM_ID)}`}
    >
      {t("library.openSample")}
    </Link>
  )
}
