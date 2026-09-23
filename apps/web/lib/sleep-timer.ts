export const SLEEP_TIMER_PRESET_MINUTES = [15, 30, 60] as const

export function sleepDeadlineFromMinutes(
  minutes: number,
  nowUnixMilliseconds = Date.now()
): number {
  return Math.round(nowUnixMilliseconds) + Math.max(0, minutes) * 60_000
}

export function formatSleepTimerRemaining(
  deadlineUnixMilliseconds: number | null,
  nowUnixMilliseconds = Date.now()
): string {
  if (deadlineUnixMilliseconds === null) return "Off"
  const remaining = Math.max(0, deadlineUnixMilliseconds - nowUnixMilliseconds)
  return `${Math.ceil(remaining / 60_000)}m left`
}
