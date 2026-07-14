import { describe, expect, it } from "vitest"
import { formatRelativeOrDate } from "./date-time"

describe("formatRelativeOrDate", () => {
  const now = Date.parse("2026-07-14T12:00:00.000Z")

  it("uses relative minutes and hours for recent values", () => {
    expect(formatRelativeOrDate("2026-07-14T11:55:00.000Z", "en-US", now)).toBe(
      "5 minutes ago"
    )
    expect(formatRelativeOrDate("2026-07-14T11:00:00.000Z", "en-US", now)).toBe(
      "1 hour ago"
    )
  })

  it("uses a date without a time for older values", () => {
    expect(formatRelativeOrDate("2026-07-10T12:00:00.000Z", "en-US", now)).toBe(
      "07/10/2026"
    )
  })
})
