import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { createTypesettingPublication } from "../../lib/typesetting"
import { PublicationModeHeader } from "./TypesettingPublication"
import { PublicationTitleField } from "./TypesettingPublicationFields"

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

describe("PublicationTitleField", () => {
  it("shows the title count to the left of the expand arrow for multiple titles", () => {
    const markup = renderToStaticMarkup(
      <PublicationTitleField
        onGenerate={() => undefined}
        onOpenTask={() => undefined}
        onUpdate={() => undefined}
        publication={{
          ...publication,
          titles: [
            { id: "title:primary", value: "Primary" },
            { id: "title:alternative", value: "Alternative" },
          ],
          selectedTitleId: "title:primary",
          title: "Primary",
        }}
        task={null}
      />
    )

    const countIndex = markup.indexOf(
      'class="op-publication-title-field__count"'
    )
    const chevronIndex = markup.indexOf(
      'class="lucide lucide-chevron-down op-publication-title-field__chevron"'
    )

    expect(countIndex).toBeGreaterThan(-1)
    expect(markup.slice(countIndex, chevronIndex)).toContain(">2</span>")
    expect(chevronIndex).toBeGreaterThan(countIndex)
  })

  it("omits the title count when only one title is available", () => {
    const markup = renderToStaticMarkup(
      <PublicationTitleField
        onGenerate={() => undefined}
        onOpenTask={() => undefined}
        onUpdate={() => undefined}
        publication={publication}
        task={null}
      />
    )

    expect(markup).not.toContain("op-publication-title-field__count")
  })
})
