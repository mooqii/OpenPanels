import type { JSONContent } from "@tiptap/core"
import type { ProjectTask, PublishingAttempt } from "../types"

export type PublishingBusinessStatus =
  | "queued"
  | "running"
  | "committing"
  | "published"
  | "needs_user_action"
  | "not_published"
  | "unknown"

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
    return task && ["reserved", "claimed", "running"].includes(task.status)
      ? "committing"
      : "unknown"
  }
  if (task && ["reserved", "claimed", "running"].includes(task.status)) {
    return "running"
  }
  if (task && ["failed", "cancelled", "blocked"].includes(task.status)) {
    return "not_published"
  }
  return "queued"
}

export function publishingAttemptIsActive(
  attempt: PublishingAttempt,
  task?: ProjectTask
) {
  const status = publishingAttemptStatus(attempt, task)
  return ["queued", "running", "committing"].includes(status)
}
