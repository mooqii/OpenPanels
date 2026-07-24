import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { createTypesettingPublication } from "../../lib/typesetting"
import { PublicationModeHeader } from "./TypesettingPublication"

const publication = createTypesettingPublication(
  "publication:test",
  "2026-07-24T00:00:00Z"
)

function renderHeader(onClose?: () => void) {
  return renderToStaticMarkup(
    <PublicationModeHeader
      onClose={onClose}
      onDelete={() => undefined}
      onRetrySave={() => undefined}
      onViewChange={() => undefined}
      publication={publication}
      saveError={null}
      saveStatus="saved"
      view="edit"
    />
  )
}

describe("PublicationModeHeader", () => {
  it("shows a close action beside the delete action when supplied", () => {
    const markup = renderHeader(() => undefined)

    expect(markup).toContain('aria-label="Delete publication project"')
    expect(markup).toContain('aria-label="Close"')
    expect(markup.indexOf('aria-label="Close"')).toBeGreaterThan(
      markup.indexOf('aria-label="Delete publication project"')
    )
  })

  it("omits the close action when the parent does not support deselection", () => {
    expect(renderHeader()).not.toContain('aria-label="Close"')
  })
})
