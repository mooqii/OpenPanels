import { describe, expect, it } from "vitest"
import {
  normalizeWikiAgentSelection,
  wikiAgentSelectionRequest,
} from "./wiki-selection"

describe("Wiki agent selection", () => {
  it("keeps only the Documents panel selection fields", () => {
    const selection = {
      isWikiSelected: true,
      selectedMyDocumentIds: ["my-document:1"],
    }

    expect(normalizeWikiAgentSelection(selection, false)).toEqual({
      isWikiSelected: false,
      selectedMyDocumentIds: ["my-document:1"],
    })
    expect(
      wikiAgentSelectionRequest(
        normalizeWikiAgentSelection(selection, false),
        false
      )
    ).toEqual({ selectedMyDocumentIds: ["my-document:1"] })
  })

  it("preserves Wiki and my-document selection in Writing", () => {
    const selection = {
      isWikiSelected: true,
      selectedMyDocumentIds: ["my-document:1"],
    }

    expect(wikiAgentSelectionRequest(selection, true)).toEqual(selection)
  })
})
