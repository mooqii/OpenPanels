import { describe, expect, it } from "vitest"
import { STABLE_BACKDROP_VARIANT } from "./overlay-safety"

describe("overlay safety", () => {
  it("avoids full-viewport backdrop filters that can black out the WebView", () => {
    expect(STABLE_BACKDROP_VARIANT).toBe("opaque")
  })
})
