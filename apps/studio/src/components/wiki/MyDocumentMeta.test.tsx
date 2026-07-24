import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import type { MyDocument } from "../../types"
import { MyDocumentMeta } from "./MyDocumentMeta"

function importedDocument(): MyDocument {
  return {
    contentRef: "content.md",
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
      originalRef: "original/reference.png",
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

describe("MyDocumentMeta", () => {
  it("marks Agent-generated documents before the character count", () => {
    const document = importedDocument()
    document.id = "document:generated"
    document.importSource = undefined
    document.conversion = undefined
    document.taskId = "task:writing"

    const markup = renderToStaticMarkup(
      <MyDocumentMeta apiBase="http://127.0.0.1:43217" document={document} />
    )

    expect(markup).toMatch(
      /op-my-document-meta__generated.*lucide-sparkle.*12.*characters/
    )
  })

  it("does not mark manually created or imported documents as generated", () => {
    const imported = importedDocument()
    const manual = importedDocument()
    manual.id = "document:manual"
    manual.importSource = undefined
    manual.conversion = undefined

    for (const document of [manual, imported]) {
      const markup = renderToStaticMarkup(
        <MyDocumentMeta apiBase="http://127.0.0.1:43217" document={document} />
      )

      expect(markup).not.toContain("op-my-document-meta__generated")
      expect(markup).not.toContain("lucide-sparkle")
    }
  })

  it("keeps the original format visible without an original-file action", () => {
    const markup = renderToStaticMarkup(
      <MyDocumentMeta
        apiBase="http://127.0.0.1:43217"
        document={importedDocument()}
      />
    )

    expect(markup).toContain("<span>PNG</span>")
    expect(markup).toContain("<span>MD</span>")
    expect(markup).not.toContain("op-my-document-meta__original")
  })

  it("makes the original format actionable when an opener is provided", () => {
    const markup = renderToStaticMarkup(
      <MyDocumentMeta
        apiBase="http://127.0.0.1:43217"
        document={importedDocument()}
        onOpenOriginal={() => undefined}
      />
    )

    expect(markup).toContain("op-my-document-meta__original")
    expect(markup).toContain(">PNG</button>")
    expect(markup).toContain("<span>MD</span>")
  })
})
