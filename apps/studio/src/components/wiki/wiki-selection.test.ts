import { describe, expect, it } from "vitest"
import {
  normalizeWikiAgentSelection,
  wikiAgentSelectionRequest,
} from "./wiki-selection"

describe("Wiki agent selection", () => {
  it("ignores legacy Wiki and raw-document fields in the Documents panel", () => {
    const legacy = {
      isWikiSelected: true,
      selectedGeneratedDocumentIds: ["generated:1"],
      selectedRawDocumentIds: ["raw:1"],
    }

    expect(normalizeWikiAgentSelection(legacy, false)).toEqual({
      isWikiSelected: false,
      selectedGeneratedDocumentIds: ["generated:1"],
    })
    expect(
      wikiAgentSelectionRequest(
        normalizeWikiAgentSelection(legacy, false),
        false
      )
    ).toEqual({ selectedGeneratedDocumentIds: ["generated:1"] })
  })

  it("preserves Wiki and generated-document selection in Writing", () => {
    const selection = {
      isWikiSelected: true,
      selectedGeneratedDocumentIds: ["generated:1"],
    }

    expect(wikiAgentSelectionRequest(selection, true)).toEqual(selection)
  })
})
