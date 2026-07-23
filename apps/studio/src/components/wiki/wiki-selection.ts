export interface WikiAgentSelection {
  isWikiSelected: boolean
  selectedMyDocumentIds: string[]
}

export function normalizeWikiAgentSelection(
  selection: Partial<WikiAgentSelection> | undefined,
  isWritingPanel: boolean
): WikiAgentSelection {
  return {
    isWikiSelected: isWritingPanel && Boolean(selection?.isWikiSelected),
    selectedMyDocumentIds: selection?.selectedMyDocumentIds ?? [],
  }
}

export function wikiAgentSelectionRequest(
  selection: WikiAgentSelection,
  isWritingPanel: boolean
): WikiAgentSelection | Pick<WikiAgentSelection, "selectedMyDocumentIds"> {
  const normalized = normalizeWikiAgentSelection(selection, isWritingPanel)
  return isWritingPanel
    ? normalized
    : { selectedMyDocumentIds: normalized.selectedMyDocumentIds }
}
