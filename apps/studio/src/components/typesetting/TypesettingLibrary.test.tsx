import { createRef } from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { createTypesettingPublication } from "../../lib/typesetting"
import type { MyDocument } from "../../types"
import { TypesettingLibrary } from "./TypesettingLibrary"

const document: MyDocument = {
  contentRef: "documents/article.md",
  contentVersion: 1,
  createdAt: "2026-07-23T00:00:00Z",
  format: "markdown",
  id: "document:article",
  mimeType: "text/markdown",
  originalFileName: "article.md",
  publishHistory: [],
  taskId: null,
  threadId: null,
  title: "Article",
  updatedAt: "2026-07-23T00:00:00Z",
  wordCount: 12,
}

function renderLibrary(activePublicationId: string | null) {
  const publication = createTypesettingPublication(
    "publication:article",
    "2026-07-23T00:00:00Z"
  )
  return renderToStaticMarkup(
    <TypesettingLibrary
      activePublicationId={activePublicationId}
      addMyDocumentFiles={async () => undefined}
      className=""
      createMyDocument={async () => undefined}
      handleMyDocumentDragEnter={() => undefined}
      handleMyDocumentDragLeave={() => undefined}
      handleMyDocumentDragOver={() => undefined}
      handleMyDocumentDrop={() => undefined}
      insertingDocumentId={null}
      isInsertDisabled={false}
      isMyDocumentBusy={false}
      isMyDocumentDragActive={false}
      myDocumentFileInputRef={createRef<HTMLInputElement>()}
      myDocuments={[document]}
      onClose={() => undefined}
      onCreatePublication={() => undefined}
      onGeneratePublicationFromMyDocument={() => undefined}
      onInsertMyDocument={() => undefined}
      onOpenMyDocument={() => undefined}
      onOpenMyDocumentOriginal={() => undefined}
      onOpenPublication={() => undefined}
      projectId="project:test"
      publications={[publication]}
      transport={{ apiBase: "http://127.0.0.1:43217", kind: "http" }}
    />
  )
}

describe("TypesettingLibrary My Documents actions", () => {
  it("offers generation when no publication content is selected", () => {
    const markup = renderLibrary(null)

    expect(markup).toContain(
      'aria-label="Generate publication content from this document"'
    )
    expect(markup).not.toContain(
      'aria-label="Insert document content into content details"'
    )
  })

  it("offers insertion when publication content is selected", () => {
    const markup = renderLibrary("publication:article")

    expect(markup).toContain(
      'aria-label="Insert document content into content details"'
    )
    expect(markup).not.toContain(
      'aria-label="Generate publication content from this document"'
    )
  })
})
