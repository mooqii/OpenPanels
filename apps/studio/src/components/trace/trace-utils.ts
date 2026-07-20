import type { MyOpenPanelsLocale } from "../../canvas"
import type {
  ProjectTask,
  TaskExecutionScope,
  TraceCategory,
  TraceEvent,
} from "../../types"
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

export function taskExecutionScope(task: ProjectTask): TaskExecutionScope {
  if (task.wikiUpdateGroup) {
    return {
      kind: "wiki-mutation-drain",
      mutationKey: task.wikiUpdateGroup.mutationKey,
      projectId: task.projectId,
    }
  }
  return { kind: "exact-task", taskId: task.id }
}

export interface ManualAgentScopeCandidate {
  isReady: boolean
  key: string
  scope: TaskExecutionScope
}

export function manualAgentScopeCandidates(
  tasks: ProjectTask[]
): ManualAgentScopeCandidate[] {
  const byId = new Map(tasks.map((task) => [task.id, task]))
  const coveredTaskIds = new Set<string>()
  const candidates: ManualAgentScopeCandidate[] = []
  const mutationGroups = new Map<string, ProjectTask[]>()

  for (const task of tasks) {
    if (
      task.queue === "wiki" &&
      task.mutationKey &&
      WIKI_UPDATE_TASK_TYPES.has(task.type) &&
      !isDoneTask(task)
    ) {
      const group = mutationGroups.get(task.mutationKey) ?? []
      group.push(task)
      mutationGroups.set(task.mutationKey, group)
    }
  }

  for (const [mutationKey, mutationTasks] of mutationGroups) {
    const scopedTasks = [...mutationTasks]
    const visitPrerequisites = (task: ProjectTask) => {
      for (const dependency of task.dependencies ?? []) {
        const prerequisite = byId.get(dependency.prerequisiteTaskId)
        if (!prerequisite || coveredTaskIds.has(prerequisite.id)) continue
        coveredTaskIds.add(prerequisite.id)
        scopedTasks.push(prerequisite)
        visitPrerequisites(prerequisite)
      }
    }
    for (const task of mutationTasks) {
      coveredTaskIds.add(task.id)
      visitPrerequisites(task)
    }
    const projectId = mutationTasks[0].projectId
    candidates.push({
      isReady: scopedTasks.some(isTaskReadyForManualAgent),
      key: `wiki-mutation-drain:${projectId}:${mutationKey}`,
      scope: { kind: "wiki-mutation-drain", mutationKey, projectId },
    })
  }

  for (const task of tasks) {
    if (coveredTaskIds.has(task.id) || isDoneTask(task)) continue
    candidates.push({
      isReady: isTaskReadyForManualAgent(task),
      key: `exact-task:${task.id}`,
      scope: { kind: "exact-task", taskId: task.id },
    })
  }
  return candidates
}

export function taskExecutionScopeKey(scope: TaskExecutionScope): string {
  switch (scope.kind) {
    case "project-drain":
      return `project-drain:${scope.projectId}`
    case "wiki-mutation-drain":
      return `wiki-mutation-drain:${scope.projectId}:${scope.mutationKey}`
    case "exact-task":
      return `exact-task:${scope.taskId}`
    default:
      throw new Error("Unknown Task execution scope")
  }
}

export function manualTaskInstruction(
  scope: TaskExecutionScope,
  locale: MyOpenPanelsLocale
): string {
  const selector =
    scope.kind === "project-drain"
      ? `--scope project-drain --project-id ${shellQuote(scope.projectId)}`
      : scope.kind === "wiki-mutation-drain"
        ? `--scope wiki-mutation-drain --project-id ${shellQuote(scope.projectId)} --mutation-key ${shellQuote(scope.mutationKey)}`
        : `--scope exact-task --task-id ${shellQuote(scope.taskId)}`
  const command = `myopenpanels task scope read ${selector} --format json`

  if (locale === "zh-CN") {
    const objective =
      scope.kind === "project-drain"
        ? `排空 Project ${scope.projectId} 中的任务`
        : scope.kind === "wiki-mutation-drain"
          ? `排空 Project ${scope.projectId} 中 Wiki mutation ${scope.mutationKey} 的串行更新队列`
          : `只处理任务 ${scope.taskId}`
    return `请通过 MyOpenPanels ${objective}。先执行下面的 scope 命令，按照返回的 required actions 工作，并重复 scope claim，直到 scopeState 为 complete 或 blocked 后再退出：\n\n${command}`
  }
  const objective =
    scope.kind === "project-drain"
      ? `drain tasks in Project ${scope.projectId}`
      : scope.kind === "wiki-mutation-drain"
        ? `drain the serial Wiki mutation queue ${scope.mutationKey} in Project ${scope.projectId}`
        : `process only Task ${scope.taskId}`
  return `Use MyOpenPanels to ${objective}. Run the scope command below first, follow its required actions, and repeat scope claim until scopeState is complete or blocked before exiting:\n\n${command}`
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

function isTaskReadyForManualAgent(task: ProjectTask): boolean {
  return (
    Boolean(task.ready) &&
    (task.status === "queued" || task.status === "failed")
  )
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
