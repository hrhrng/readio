"use client"

import { useEffect, useState } from "react"
import { Clock3 } from "lucide-react"
import { useLocale } from "../locale-provider"

import {
  formatSleepTimerRemaining,
  SLEEP_TIMER_PRESET_MINUTES,
  sleepDeadlineFromMinutes,
} from "@/lib/sleep-timer"

interface SleepTimerControlProps {
  deadlineUnixMilliseconds: number | null
  onChange(deadlineUnixMilliseconds: number | null): void
  nowUnixMilliseconds?: number
}

export function SleepTimerControl({
  deadlineUnixMilliseconds,
  onChange,
  nowUnixMilliseconds,
}: SleepTimerControlProps) {
  const { t } = useLocale()
  const [now, setNow] = useState(nowUnixMilliseconds ?? Date.now())

  useEffect(() => {
    if (deadlineUnixMilliseconds === null || nowUnixMilliseconds !== undefined) return
    const timer = window.setInterval(() => setNow(Date.now()), 30_000)
    return () => window.clearInterval(timer)
  }, [deadlineUnixMilliseconds, nowUnixMilliseconds])

  return (
    <label className="reader-text-control reader-sleep-timer">
      <Clock3 className="ui-icon-sm" aria-hidden="true" />
      <select
        aria-label={t("reader.sleepTimer")}
        value={deadlineUnixMilliseconds === null ? "off" : "active"}
        onChange={(event) => {
          const value = event.target.value
          if (value === "active") return
          if (value === "off") {
            onChange(null)
            return
          }
          onChange(sleepDeadlineFromMinutes(Number(value)))
        }}
      >
        {deadlineUnixMilliseconds !== null && (
          <option value="active" disabled>
            {formatSleepTimerRemaining(deadlineUnixMilliseconds, now)}
          </option>
        )}
        <option value="off">{t("reader.off")}</option>
        {SLEEP_TIMER_PRESET_MINUTES.map((minutes) => (
          <option key={minutes} value={minutes}>
            {t("reader.minutes", { minutes })}
          </option>
        ))}
      </select>
    </label>
  )
}
