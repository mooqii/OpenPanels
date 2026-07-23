import { describe, expect, it } from "vitest"
import {
  normalizeWikiAgentSelection,
  wikiAgentSelectionRequest,
} from "./wiki-selection"

describe("Wiki agent selection", () => {
  it("ignores legacy Wiki and raw-document fields in the Documents panel", () => {
    const legacy = {
      isWikiSelected: true,
      selectedMyDocumentIds: ["my-document:1"],
      selectedRawDocumentIds: ["raw:1"],
    }

    expect(normalizeWikiAgentSelection(legacy, false)).toEqual({
      isWikiSelected: false,
      selectedMyDocumentIds: ["my-document:1"],
    })
    expect(
      wikiAgentSelectionRequest(
        normalizeWikiAgentSelection(legacy, false),
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
