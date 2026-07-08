import { describe, expect, it } from "vitest"
import {
  artifactSchema,
  createSessionInputSchema,
  openPanelInputSchema,
} from "./index"

describe("@openpanels/protocol", () => {
  it("parses image artifacts", () => {
    const parsed = artifactSchema.parse({
      id: "artifact:1",
      kind: "image",
      mimeType: "image/png",
      assetRef: "assets/image.png",
      createdAt: new Date().toISOString(),
    })

    expect(parsed.kind).toBe("image")
  })

  it("defaults session titles", () => {
    expect(createSessionInputSchema.parse({}).title).toBe("OpenPanels Session")
  })

  it("accepts canvas panels", () => {
    expect(
      openPanelInputSchema.parse({
        sessionId: "session:1",
        kind: "canvas",
      }).kind
    ).toBe("canvas")
  })

  it("accepts wiki panels", () => {
    expect(
      openPanelInputSchema.parse({
        sessionId: "session:1",
        kind: "wiki",
      }).kind
    ).toBe("wiki")
  })
})
