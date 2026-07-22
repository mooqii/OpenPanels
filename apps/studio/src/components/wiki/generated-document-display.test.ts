import { describe, expect, it } from "vitest"
import {
  countDocumentCharacters,
  generatedDocumentConversionDisplay,
  generatedDocumentFormats,
} from "./generated-document-display"

describe("countDocumentCharacters", () => {
  it("counts non-whitespace Unicode characters", () => {
    expect(countDocumentCharacters("Hello 世界\n")).toBe(7)
  })
})

describe("generatedDocumentConversionDisplay", () => {
  it("locks queued and converting documents until content is ready", () => {
    expect(
      generatedDocumentConversionDisplay({
        conversion: {
          error: null,
          status: "queued",
          taskId: "task:1",
          updatedAt: "2026-07-22T00:00:00Z",
        },
      })
    ).toEqual({ isFailed: false, isLocked: true, label: "pending" })
    expect(
      generatedDocumentConversionDisplay({
        conversion: {
          error: null,
          status: "converting",
          taskId: "task:1",
          updatedAt: "2026-07-22T00:00:01Z",
        },
      })
    ).toEqual({ isFailed: false, isLocked: true, label: "converting" })
  })

  it("exposes failed conversions for retry without locking document actions", () => {
    expect(
      generatedDocumentConversionDisplay({
        conversion: {
          error: "conversion failed",
          status: "failed",
          taskId: "task:1",
          updatedAt: "2026-07-22T00:00:02Z",
        },
      })
    ).toEqual({ isFailed: true, isLocked: false, label: null })
  })
})

describe("generatedDocumentFormats", () => {
  const importSource = {
    fileName: "moodbook.png",
    mimeType: "image/png",
    originalRef: "generated/document/original/moodbook.png",
    sha256: "fixture",
    sizeBytes: 42,
  }

  it("shows the imported and current formats after conversion", () => {
    expect(
      generatedDocumentFormats({
        conversion: {
          error: null,
          status: "ready",
          taskId: "task:1",
          updatedAt: "2026-07-22T00:00:00Z",
        },
        format: "markdown",
        importSource,
      })
    ).toEqual({ converted: "MD", original: "PNG" })
  })

  it("shows only the imported format while conversion is pending", () => {
    expect(
      generatedDocumentFormats({
        conversion: {
          error: null,
          status: "queued",
          taskId: "task:1",
          updatedAt: "2026-07-22T00:00:00Z",
        },
        format: "markdown",
        importSource,
      })
    ).toEqual({ converted: null, original: "PNG" })
  })

  it("does not add source-format metadata to native documents", () => {
    expect(
      generatedDocumentFormats({
        conversion: undefined,
        format: "markdown",
        importSource: undefined,
      })
    ).toEqual({ converted: null, original: null })
  })
})
