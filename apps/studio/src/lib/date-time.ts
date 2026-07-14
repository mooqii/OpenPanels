const RECENT_TIME_WINDOW_MS = 24 * 60 * 60 * 1000

export function formatRelativeOrDate(
  value: string,
  locale: string,
  now = Date.now()
): string {
  const timestamp = Date.parse(value)
  if (Number.isNaN(timestamp)) return value

  const difference = timestamp - now
  const absoluteDifference = Math.abs(difference)
  const relativeTime = new Intl.RelativeTimeFormat(locale, { numeric: "auto" })

  if (absoluteDifference < 60_000) {
    return relativeTime.format(0, "second")
  }
  if (absoluteDifference < 60 * 60_000) {
    return relativeTime.format(Math.round(difference / 60_000), "minute")
  }
  if (absoluteDifference < RECENT_TIME_WINDOW_MS) {
    return relativeTime.format(Math.round(difference / 3_600_000), "hour")
  }

  return new Intl.DateTimeFormat(locale, {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
  }).format(timestamp)
}
