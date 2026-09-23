"use client"

import { useEffect } from "react"

import { useSession } from "@/lib/auth-client"
import { syncWebCloud } from "@/lib/browser-local-replica"

export function CloudSyncBridge() {
  const { data: session } = useSession()
  const userId = session?.user?.id

  useEffect(() => {
    if (!userId) return
    void syncWebCloud(userId).catch(() => {
      // Cloud is optional. A failed background sync must never interrupt the
      // local library; the next authenticated mount or online event retries.
    })
  }, [userId])

  return null
}
