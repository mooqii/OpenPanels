import type { JSONContent } from "@tiptap/core"
import type {
  ProjectTask,
  PublishingAttempt,
  PublishingRelease,
} from "../types"
import { taskDisplayPhase } from "./task-status"

export type PublishingBusinessStatus =
  | "queued"
  | "running"
  | "committing"
  | "published"
  | "needs_user_action"
  | "not_published"
  | "unknown"

export type PublishingPublicationStatus =
  | "pending"
  | "publishing"
  | "needs_user_action"
  | "error"
  | "unknown"

export interface PublishingPublicationSummary {
  publishedCount: number
  statuses: PublishingPublicationStatus[]
}

export function publishingSourceHasContent(
  bodyText: string,
  coverCount: number
): boolean {
  return bodyText.trim().length > 0 || coverCount > 0
}

export function typesettingContentToPlainText(document: JSONContent): string {
  const output: string[] = []
  renderNode(document, output)
  return output
    .join("")
    .split("\n")
    .map((line) => line.trimEnd())
    .filter((line, index, lines) => line || (index > 0 && lines[index - 1]))
    .join("\n")
    .trim()
}

function renderNode(node: JSONContent, output: string[]) {
  if (node.type === "text") {
    output.push(node.text ?? "")
    return
  }
  if (node.type === "hardBreak") {
    output.push("\n")
    return
  }
  if (node.type === "image") return
  if (node.type === "bulletList" || node.type === "orderedList") {
    for (const [index, item] of (node.content ?? []).entries()) {
      output.push(node.type === "orderedList" ? `${index + 1}. ` : "- ")
      for (const child of item.content ?? []) {
        renderNode(child, output)
      }
      output.push("\n")
    }
    output.push("\n")
    return
  }
  for (const child of node.content ?? []) {
    renderNode(child, output)
  }
  if (["paragraph", "heading", "blockquote"].includes(node.type ?? "")) {
    output.push("\n\n")
  }
}

export function publishingAttemptStatus(
  attempt: PublishingAttempt,
  task?: ProjectTask
): PublishingBusinessStatus {
  if (attempt.outcome) return attempt.outcome
  if (attempt.phase === "committing") {
    return task && taskDisplayPhase(task) === "running"
      ? "committing"
      : "unknown"
  }
  if (!task) return "unknown"
  const phase = taskDisplayPhase(task)
  if (phase === "running") return "running"
  if (phase === "failed" || phase === "cancelled") {
    return "not_published"
  }
  return phase === "waiting" ? "queued" : "unknown"
}

export function publishingAttemptIsActive(
  attempt: PublishingAttempt,
  task?: ProjectTask
) {
  const status = publishingAttemptStatus(attempt, task)
  return ["queued", "running", "committing"].includes(status)
}

export function publishingPublicationSummary(
  releases: PublishingRelease[],
  tasks: ProjectTask[]
): PublishingPublicationSummary {
  const taskById = new Map(tasks.map((task) => [task.id, task]))
  const attemptsById = new Map<string, PublishingAttempt>()
  for (const release of releases) {
    for (const attempt of release.attempts) {
      attemptsById.set(attempt.id, attempt)
    }
  }

  const attempts = [...attemptsById.values()]
  const publishedCount = attempts.filter(
    (attempt) =>
      publishingAttemptStatus(attempt, taskById.get(attempt.taskId)) ===
      "published"
  ).length
  const latestBySkill = new Map<string, PublishingAttempt>()
  for (const attempt of attempts) {
    const current = latestBySkill.get(attempt.skillId)
    if (!current || attemptUpdatedAt(attempt) > attemptUpdatedAt(current)) {
      latestBySkill.set(attempt.skillId, attempt)
    }
  }

  const activeStatuses = new Set<PublishingPublicationStatus>()
  for (const attempt of latestBySkill.values()) {
    const status = publishingAttemptStatus(
      attempt,
      taskById.get(attempt.taskId)
    )
    if (status === "queued") activeStatuses.add("pending")
    if (status === "running" || status === "committing") {
      activeStatuses.add("publishing")
    }
    if (status === "needs_user_action") {
      activeStatuses.add("needs_user_action")
    }
    if (status === "not_published") activeStatuses.add("error")
    if (status === "unknown") activeStatuses.add("unknown")
  }

  const statusPriority: PublishingPublicationStatus[] = [
    "error",
    "needs_user_action",
    "publishing",
    "pending",
    "unknown",
  ]
  return {
    publishedCount,
    statuses: statusPriority.filter((status) => activeStatuses.has(status)),
  }
}

function attemptUpdatedAt(attempt: PublishingAttempt): number {
  const timestamp =
    attempt.updatedAt ?? attempt.completedAt ?? attempt.createdAt
  const parsed = Date.parse(timestamp)
  return Number.isNaN(parsed) ? 0 : parsed
}
