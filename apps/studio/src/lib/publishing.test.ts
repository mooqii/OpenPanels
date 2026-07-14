import { describe, expect, it } from "vitest"
import {
  emptyPublishingState,
  isPublishingState,
  normalizePanelState,
} from "./api"

describe("Publishing state", () => {
  it("uses an empty schema v1 state for the panel scaffold", () => {
    expect(emptyPublishingState()).toEqual({ schemaVersion: 1 })
    expect(isPublishingState({ schemaVersion: 1 })).toBe(true)
    expect(isPublishingState({ schemaVersion: 2 })).toBe(false)
    expect(normalizePanelState("publishing", null)).toEqual({
      schemaVersion: 1,
    })
  })
})
