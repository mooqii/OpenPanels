import type { ProjectTask, WikiGeneratedDocument, WritingState } from "../types"

export type WritingSkillSelectionError = "required" | "revision_limit" | null
export type WritingReferenceSelectionError = "required" | "unready" | null
export type RefinementTaskGroup = "active" | "waiting" | "error"
export type WritingDocumentStatus =
  | "pending_create"
  | "pending_revise"
  | "active"
  | "failed"

const ACTIVE_TASK_STATUSES = new Set([
  "reserved",
  "running",
  "claimed",
  "converting",
  "indexing",
  "cancel_requested",
])
const WAITING_TASK_STATUSES = new Set(["waiting", "queued"])

export function refinementTaskGroups(
  tasks: ProjectTask[]
): Record<RefinementTaskGroup, ProjectTask[]> {
  const groups: Record<RefinementTaskGroup, ProjectTask[]> = {
    active: [],
    waiting: [],
    error: [],
  }
  for (const task of tasks) {
    if (!(task.queue === "writing" && task.type === "refine_writing_skill")) {
      continue
    }
    if (WAITING_TASK_STATUSES.has(task.status)) groups.waiting.push(task)
    else if (ACTIVE_TASK_STATUSES.has(task.status)) groups.active.push(task)
    else if (task.status === "failed") groups.error.push(task)
  }
  for (const group of Object.values(groups)) {
    group.sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
  }
  return groups
}

export function writingTaskTargetId(task: ProjectTask): string | null {
  if (!task.input || typeof task.input !== "object") return null
  const value = (task.input as { targetGeneratedDocumentId?: unknown })
    .targetGeneratedDocumentId
  return typeof value === "string" ? value : null
}

export function latestWritingTaskForDocument(
  tasks: ProjectTask[],
  document: Pick<WikiGeneratedDocument, "id" | "taskId">
): ProjectTask | null {
  return (
    tasks
      .filter(
        (task) =>
          task.queue === "writing" &&
          task.type === "generate_document" &&
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
  if (WAITING_TASK_STATUSES.has(task.status)) {
    const mode =
      task.input && typeof task.input === "object"
        ? (task.input as { mode?: unknown }).mode
        : null
    return mode === "revise" ? "pending_revise" : "pending_create"
  }
  if (ACTIVE_TASK_STATUSES.has(task.status)) return "active"
  if (task.status === "failed") return "failed"
  return null
}

export function sortGeneratedDocumentsByActivity(
  documents: WikiGeneratedDocument[],
  tasks: ProjectTask[]
): WikiGeneratedDocument[] {
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
  if (mode === "refine") return null
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
  if (mode === "refine") return current
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
