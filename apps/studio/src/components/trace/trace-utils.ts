import type { ProjectTask, TraceCategory } from "../../types"
import type { TaskFilter } from "./TracePanel"

export function taskCommand(
  task: ProjectTask
): { label: string; value: string } | null {
  if (task.status !== "queued" && task.status !== "failed") return null
  if (!task.ready) return null
  return {
    label: "Claim with a registered target",
    value: `myopenpanels task claim --task-id ${shellQuote(task.id)} --target-id <target-id> --format json`,
  }
}

export function compareTasksForDisplay(
  left: ProjectTask,
  right: ProjectTask
): number {
  const rank = taskDisplayRank(left) - taskDisplayRank(right)
  if (rank !== 0) return rank
  return Date.parse(right.updatedAt) - Date.parse(left.updatedAt)
}

export function taskDisplayRank(task: ProjectTask): number {
  if (task.ready && task.status === "failed") return 0
  if (task.ready && task.status === "queued") return 1
  if (!task.ready && task.status === "failed") return 2
  if (!task.ready && task.status === "queued") return 3
  return 4
}

export function formatBlockedReason(reason: string): string {
  switch (reason) {
    case "attemptsExceeded":
      return "exhausted"
    case "retryLater":
      return "retry later"
    case "leased":
      return "leased"
    default:
      return reason
  }
}

export function formatTaskError(error: unknown): string {
  if (typeof error === "string") return error
  try {
    return JSON.stringify(error)
  } catch {
    return "Task failed"
  }
}

export function shellQuote(value: string): string {
  if (/^[A-Za-z0-9_./:-]+$/.test(value)) return value
  return `'${value.replaceAll("'", "'\\''")}'`
}

export function taskMatchesFilter(
  task: ProjectTask,
  filter: TaskFilter
): boolean {
  switch (filter) {
    case "pending":
      return ["waiting", "queued", "failed"].includes(task.status)
    case "active":
      return isActiveTask(task)
    case "done":
      return isDoneTask(task)
    case "all":
      return true
    default:
      return true
  }
}

export function isActiveTask(task: ProjectTask): boolean {
  return [
    "reserved",
    "running",
    "claimed",
    "converting",
    "indexing",
    "cancel_requested",
  ].includes(task.status)
}

export function isDoneTask(task: ProjectTask): boolean {
  return ["succeeded", "cancelled", "stale", "superseded"].includes(task.status)
}

export function pendingTaskCount(tasks: ProjectTask[]): number {
  return tasks.filter(
    (task) => task.status === "queued" || task.status === "failed"
  ).length
}

export function formatTaskCount(count: number): string {
  return count > 99 ? "99+" : String(count)
}

export function formatTaskType(type: string): string {
  return type
    .split("_")
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ")
}

export function formatWorkerStatus(status: string): string {
  switch (status) {
    case "running":
      return "Running"
    case "error":
      return "Error"
    case "noTarget":
      return "No target"
    default:
      return "Idle"
  }
}

export function formatDispatchState(status: string): string {
  switch (status) {
    case "noTarget":
      return "no target"
    default:
      return status
  }
}

export function formatTaskTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    month: "short",
    day: "numeric",
  }).format(date)
}

export function taskStatusTone(status: string): string {
  if (status === "failed") return "danger"
  if (["waiting", "queued"].includes(status)) return "warning"
  if (
    [
      "reserved",
      "running",
      "claimed",
      "converting",
      "indexing",
      "cancel_requested",
    ].includes(status)
  ) {
    return "active"
  }
  if (status === "succeeded") return "success"
  return "muted"
}

export function taskStatusColor(status: string) {
  switch (taskStatusTone(status)) {
    case "danger":
      return "danger"
    case "warning":
      return "warning"
    case "success":
      return "success"
    case "active":
      return "accent"
    default:
      return "default"
  }
}

export function traceCategoryColor(category: TraceCategory) {
  switch (category) {
    case "error":
      return "danger"
    case "cli":
      return "warning"
    case "task":
      return "success"
    case "api":
    case "agent":
      return "accent"
    case "system":
      return "default"
    default:
      return "default"
  }
}

export function formatLocalBuildTime(value: string): string | null {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return null
  const datePart = [
    padDatePart(date.getMonth() + 1),
    padDatePart(date.getDate()),
  ].join("-")
  const timePart = [
    padDatePart(date.getHours()),
    padDatePart(date.getMinutes()),
    padDatePart(date.getSeconds()),
  ].join(":")
  return `${datePart} ${timePart}`
}

export function padDatePart(value: number): string {
  return String(value).padStart(2, "0")
}
