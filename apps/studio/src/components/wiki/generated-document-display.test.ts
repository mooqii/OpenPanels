import { describe, expect, it } from "vitest"
import { countDocumentCharacters } from "./generated-document-display"

describe("countDocumentCharacters", () => {
  it("counts non-whitespace Unicode characters", () => {
    expect(countDocumentCharacters("Hello 世界\n")).toBe(7)
  })
})
