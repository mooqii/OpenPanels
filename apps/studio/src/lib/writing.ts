import type { MyDocument, ProjectTask, WritingState } from "../types"
import { taskDisplayPhase } from "./task-status"

export type WritingSkillSelectionError = "required" | "revision_limit" | null
export type WritingReferenceSelectionError = "required" | "unready" | null
export type DistillationTaskGroup = "active" | "waiting" | "error"
export type WritingDocumentStatus =
  | "pending_create"
  | "pending_revise"
  | "active"
  | "failed"

export function distillationTaskGroups(
  tasks: ProjectTask[]
): Record<DistillationTaskGroup, ProjectTask[]> {
  const groups: Record<DistillationTaskGroup, ProjectTask[]> = {
    active: [],
    waiting: [],
    error: [],
  }
  for (const task of tasks) {
    if (!(task.queue === "writing" && task.type === "distill_writing_skill")) {
      continue
    }
    const phase = taskDisplayPhase(task)
    if (phase === "waiting") groups.waiting.push(task)
    else if (phase === "running") groups.active.push(task)
    else if (phase === "failed") groups.error.push(task)
  }
  for (const group of Object.values(groups)) {
    group.sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
  }
  return groups
}

export function writingTaskTargetId(task: ProjectTask): string | null {
  if (!task.input || typeof task.input !== "object") return null
  const value = (task.input as { targetMyDocumentId?: unknown })
    .targetMyDocumentId
  return typeof value === "string" ? value : null
}

export function latestWritingTaskForDocument(
  tasks: ProjectTask[],
  document: Pick<MyDocument, "id" | "taskId">
): ProjectTask | null {
  return (
    tasks
      .filter(
        (task) =>
          task.queue === "writing" &&
          task.type === "write_my_document" &&
          (writingTaskTargetId(task) === document.id ||
            task.id === document.taskId)
      )
      .sort((left, right) =>
        right.updatedAt.localeCompare(left.updatedAt)
      )[0] ?? null
  )
}

export function writingDocumentStatus(
  task: ProjectTask | null
): WritingDocumentStatus | null {
  if (!task) return null
  const phase = taskDisplayPhase(task)
  if (phase === "waiting") {
    const mode =
      task.input && typeof task.input === "object"
        ? (task.input as { mode?: unknown }).mode
        : null
    return mode === "revise" ? "pending_revise" : "pending_create"
  }
  if (phase === "running") return "active"
  if (phase === "failed") return "failed"
  return null
}

export function sortMyDocumentsByActivity(
  documents: MyDocument[],
  tasks: ProjectTask[]
): MyDocument[] {
  return documents
    .map((document, index) => {
      const task = latestWritingTaskForDocument(tasks, document)
      return {
        document,
        index,
        timestamp: Math.max(
          Date.parse(document.updatedAt) || 0,
          Date.parse(task?.updatedAt ?? "") || 0
        ),
      }
    })
    .sort(
      (left, right) =>
        right.timestamp - left.timestamp || left.index - right.index
    )
    .map(({ document }) => document)
}

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
  if (mode === "distill") return null
  if (selectedIds.length === 0) return "required"
  if (mode === "revise" && selectedIds.length > 1) return "revision_limit"
  return null
}

export function writingReferenceSelectionError(
  mode: WritingState["mode"],
  selectedCount: number,
  unreadyCount: number
): WritingReferenceSelectionError {
  if (mode !== "create") return null
  if (selectedCount === 0) return "required"
  if (unreadyCount > 0) return "unready"
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
  if (mode === "distill") return current
  return isSelected
    ? [...current.filter((id) => id !== skillId), skillId]
    : current.filter((id) => id !== skillId)
}

export function selectSingleSkill(
  current: string | null,
  skillId: string,
  isSelected: boolean
): string | null {
  return isSelected ? skillId : current
}
