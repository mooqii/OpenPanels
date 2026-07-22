export interface WikiAgentSelection {
  isWikiSelected: boolean
  selectedGeneratedDocumentIds: string[]
}

export function normalizeWikiAgentSelection(
  selection: Partial<WikiAgentSelection> | undefined,
  isWritingPanel: boolean
): WikiAgentSelection {
  return {
    isWikiSelected: isWritingPanel && Boolean(selection?.isWikiSelected),
    selectedGeneratedDocumentIds: selection?.selectedGeneratedDocumentIds ?? [],
  }
}

export function wikiAgentSelectionRequest(
  selection: WikiAgentSelection,
  isWritingPanel: boolean
):
  | WikiAgentSelection
  | Pick<WikiAgentSelection, "selectedGeneratedDocumentIds"> {
  const normalized = normalizeWikiAgentSelection(selection, isWritingPanel)
  return isWritingPanel
    ? normalized
    : { selectedGeneratedDocumentIds: normalized.selectedGeneratedDocumentIds }
}
