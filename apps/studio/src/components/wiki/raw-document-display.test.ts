import { describe, expect, it } from "vitest"
import { rawDocumentFormats } from "./raw-document-display"

describe("rawDocumentFormats", () => {
  it("shows an original and converted format when conversion produced Markdown", () => {
    expect(
      rawDocumentFormats({
        conversion: { status: "ready" },
        markdownRef: "raw/document/source.md",
        originalFileName: "brief.docx",
      })
    ).toEqual({ converted: "MD", original: "DOCX" })
  })

  it("shows one format for an original Markdown document", () => {
    expect(
      rawDocumentFormats({
        conversion: { status: "not_required" },
        markdownRef: "raw/document/source.md",
        originalFileName: "brief.markdown",
      })
    ).toEqual({ converted: null, original: "MD" })
  })

  it("does not present MDX as converted when Markdown conversion is not required", () => {
    expect(
      rawDocumentFormats({
        conversion: { status: "not_required" },
        markdownRef: "raw/document/source.md",
        originalFileName: "component.mdx",
      })
    ).toEqual({ converted: null, original: "MDX" })
  })

  it("shows only the original format before conversion", () => {
    expect(
      rawDocumentFormats({
        conversion: { status: "queued" },
        markdownRef: null,
        originalFileName: "brief.pdf",
      })
    ).toEqual({ converted: null, original: "PDF" })
  })
})
