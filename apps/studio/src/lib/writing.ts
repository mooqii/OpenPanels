import type { WritingState } from "../types"

export type WritingSkillSelectionError = "required" | "revision_limit" | null

export function activeWritingSkillIds(
  mode: WritingState["mode"],
  createIds: string[],
  revisionId: string | null
): string[] {
  if (mode === "create") return createIds
  if (mode === "revise") return revisionId ? [revisionId] : []
  return []
}

export function writingSkillSelectionError(
  mode: WritingState["mode"],
  selectedIds: string[]
): WritingSkillSelectionError {
  if (mode === "refine") return null
  if (selectedIds.length === 0) return "required"
  if (mode === "revise" && selectedIds.length > 1) return "revision_limit"
  return null
}

export function toggleWritingSkillSelection(
  current: string[],
  skillId: string,
  isSelected: boolean,
  mode: WritingState["mode"]
): string[] {
  if (mode === "revise") {
    if (isSelected || current.length > 1) return [skillId]
    return []
  }
  if (mode === "refine") return current
  return isSelected
    ? [...current.filter((id) => id !== skillId), skillId]
    : current.filter((id) => id !== skillId)
}
