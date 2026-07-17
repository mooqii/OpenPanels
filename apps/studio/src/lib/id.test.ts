import { describe, expect, it } from "vitest"
import { randomBase64Url96, randomId } from "./id"

describe("random Base64URL IDs", () => {
  it("encodes 96 random bits as 16 URL-safe characters", () => {
    for (let index = 0; index < 100; index += 1) {
      expect(randomBase64Url96()).toMatch(/^[A-Za-z0-9_-]{16}$/)
    }
  })

  it("preserves the ID prefix", () => {
    expect(randomId("task")).toMatch(/^task:[A-Za-z0-9_-]{16}$/)
  })
})
