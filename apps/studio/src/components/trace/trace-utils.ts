import type { MyOpenPanelsLocale } from "../../canvas"
import type { ProjectTask, TraceCategory, TraceEvent } from "../../types"
import type { TaskFilter } from "./TracePanel"

export type TraceFilter = "all" | TraceCategory

const WIKI_UPDATE_TASK_TYPES = new Set([
  "ingest_markdown_into_wiki",
  "maintain_wiki",
])

export function groupWikiUpdateTasks(tasks: ProjectTask[]): ProjectTask[] {
  const grouped = new Map<string, ProjectTask[]>()
  const output: ProjectTask[] = []
  for (const task of tasks) {
    if (
      task.queue !== "wiki" ||
      !task.mutationKey ||
      !WIKI_UPDATE_TASK_TYPES.has(task.type)
    ) {
      output.push(task)
      continue
    }
    const group = grouped.get(task.mutationKey) ?? []
    group.push(task)
    grouped.set(task.mutationKey, group)
  }
  for (const [mutationKey, groupTasks] of grouped) {
    groupTasks.sort(
      (left, right) =>
        (left.mutationSequence ?? Number.MAX_SAFE_INTEGER) -
        (right.mutationSequence ?? Number.MAX_SAFE_INTEGER)
    )
    const representative =
      groupTasks.find(isActiveTask) ??
      groupTasks.find((task) => taskMatchesFilter(task, "pending")) ??
      groupTasks.at(-1)!
    const active = groupTasks.some(isActiveTask)
    const pending = groupTasks.some((task) =>
      ["waiting", "queued", "failed"].includes(task.status)
    )
    const failed = groupTasks.some((task) => task.status === "failed")
    const completed = groupTasks.filter(isDoneTask).length
    const statuses = new Set(groupTasks.map((task) => task.status))
    const requestedConnections = new Set(
      groupTasks.map((task) => task.requestedGatewayConnectionId ?? null)
    )
    const dispatchModes = new Set(
      groupTasks.map((task) => task.dispatchMode ?? "auto")
    )
    output.push({
      ...representative,
      blockedReason: active ? null : representative.blockedReason,
      capability: "wiki.updateBatch",
      dispatchMode:
        dispatchModes.size === 1 ? representative.dispatchMode : "auto",
      id: `wiki-update-group:${mutationKey}`,
      ready: active ? false : groupTasks.some((task) => task.ready),
      requestedGatewayConnectionId:
        requestedConnections.size === 1
          ? representative.requestedGatewayConnectionId
          : null,
      result: {
        completedTaskCount: completed,
        taskCount: groupTasks.length,
      },
      status: active
        ? "running"
        : pending
          ? failed
            ? "failed"
            : "queued"
          : statuses.size === 1
            ? representative.status
            : "succeeded",
      targetId:
        typeof representative.source === "object" && representative.source
          ? String(
              (representative.source as Record<string, unknown>).wikiSpaceId ??
                representative.targetId
            )
          : representative.targetId,
      type: "wiki_update_batch",
      updatedAt: groupTasks.reduce(
        (latest, task) =>
          Date.parse(task.updatedAt) > Date.parse(latest)
            ? task.updatedAt
            : latest,
        groupTasks[0].updatedAt
      ),
      wikiUpdateGroup: {
        mutationKey,
        taskIds: groupTasks.map((task) => task.id),
        tasks: groupTasks,
      },
    })
  }
  return output
}

export function traceEventMatchesFilter(
  event: TraceEvent,
  filter: TraceFilter
): boolean {
  if (filter !== "all") return event.category === filter
  return !isActiveProjectHeartbeat(event)
}

function isActiveProjectHeartbeat(event: TraceEvent): boolean {
  if (event.category !== "api") return false

  const detail = event.detail
  if (detail && typeof detail === "object" && !Array.isArray(detail)) {
    const record = detail as Record<string, unknown>
    if (record.method === "GET" && record.path === "/api/active-project") {
      return true
    }
  }

  return /^GET \/api\/active-project(?:\s|$)/.test(event.summary)
}

export function manualTaskInstruction(
  task: ProjectTask,
  locale: MyOpenPanelsLocale
): string {
  const taskIds = task.wikiUpdateGroup?.taskIds ?? [task.id]
  const commands = taskIds
    .map(
      (taskId) =>
        `myopenpanels task read --task-id ${shellQuote(taskId)} --format json`
    )
    .join("\n")

  if (locale === "zh-CN") {
    return `请处理 MyOpenPanels ${taskIds.length > 1 ? "中的以下任务" : "任务"} ${taskIds.join(", ")}。先执行下面的命令读取任务，再按照返回的任务信息和 actions 完成处理：\n\n${commands}`
  }
  return `Process ${taskIds.length > 1 ? "these MyOpenPanels tasks" : "this MyOpenPanels task"}: ${taskIds.join(", ")}. Run the command${taskIds.length > 1 ? "s" : ""} below first, then follow the returned task details and actions to complete the work:\n\n${commands}`
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
