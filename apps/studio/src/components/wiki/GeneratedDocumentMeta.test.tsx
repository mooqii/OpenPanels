import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import type { WikiGeneratedDocument } from "../../types"
import { GeneratedDocumentMeta } from "./GeneratedDocumentMeta"

function importedDocument(): WikiGeneratedDocument {
  return {
    contentRef: "generated/document.md",
    contentVersion: 1,
    conversion: {
      error: null,
      status: "ready",
      taskId: "task:conversion",
      updatedAt: "2026-07-22T00:00:00Z",
    },
    createdAt: "2026-07-22T00:00:00Z",
    format: "markdown",
    id: "document:imported",
    importSource: {
      fileName: "reference.png",
      mimeType: "image/png",
      originalRef: "generated/document/original/reference.png",
      sha256: "fixture",
      sizeBytes: 42,
    },
    mimeType: "text/markdown",
    originalFileName: "reference.md",
    publishHistory: [],
    taskId: null,
    threadId: null,
    title: "Reference",
    updatedAt: "2026-07-22T00:00:00Z",
    wordCount: 12,
  }
}

describe("GeneratedDocumentMeta", () => {
  it("keeps the original format visible without an original-file action", () => {
    const markup = renderToStaticMarkup(
      <GeneratedDocumentMeta
        apiBase="http://127.0.0.1:43217"
        document={importedDocument()}
      />
    )

    expect(markup).toContain("<span>PNG</span>")
    expect(markup).toContain("<span>MD</span>")
    expect(markup).not.toContain("op-generated-document-meta__original")
  })

  it("makes the original format actionable when an opener is provided", () => {
    const markup = renderToStaticMarkup(
      <GeneratedDocumentMeta
        apiBase="http://127.0.0.1:43217"
        document={importedDocument()}
        onOpenOriginal={() => undefined}
      />
    )

    expect(markup).toContain("op-generated-document-meta__original")
    expect(markup).toContain(">PNG</button>")
    expect(markup).toContain("<span>MD</span>")
  })
})
