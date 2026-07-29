import { describe, expect, it } from "vitest"
import type { MyDocument } from "../../types"
import { myDocumentPublishActionText } from "./MyDocumentsModule"

function document(publishHistory: MyDocument["publishHistory"]): MyDocument {
  return {
    contentRef: "documents/article.md",
    contentVersion: 1,
    createdAt: "2026-07-29T00:00:00Z",
    format: "markdown",
    id: "document:article",
    mimeType: "text/markdown",
    originalFileName: "article.md",
    publishHistory,
    taskId: null,
    threadId: null,
    title: "Article",
    updatedAt: "2026-07-29T00:00:00Z",
    wordCount: 12,
  }
}

describe("MyDocumentActions", () => {
  it("offers adding an unpublished document to raw documents", () => {
    expect(myDocumentPublishActionText(document([]))).toBe(
      "Add to raw documents"
    )
  })

  it("offers adding the latest version after a document was published", () => {
    const published = document([
      {
        documentVersion: 1,
        publishedAt: "2026-07-29T00:00:00Z",
        rawDocumentId: "raw:article",
      },
    ])

    expect(myDocumentPublishActionText(published)).toBe(
      "Add latest version to raw documents"
    )
  })
})
