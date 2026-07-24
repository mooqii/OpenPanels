import type { MyOpenPanelsLocale } from "../../canvas"
import {
  type AgentRuntimeIdentity,
  agentCliBoundaryInstruction,
  agentCliExecutable,
} from "../../lib/agent-instructions"
import {
  taskDisplayPhase,
  taskIsActive,
  taskIsTerminal,
} from "../../lib/task-status"
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
  locale: MyOpenPanelsLocale,
  runtime: AgentRuntimeIdentity
): string {
  const selector =
    scope.kind === "project-drain"
      ? `--scope project-drain --project-id ${shellQuote(scope.projectId)}`
      : scope.kind === "wiki-mutation-drain"
        ? `--scope wiki-mutation-drain --project-id ${shellQuote(scope.projectId)} --mutation-key ${shellQuote(scope.mutationKey)}`
        : `--scope exact-task --task-id ${shellQuote(scope.taskId)}`
  const command = `${agentCliExecutable(runtime)} task handoff start ${selector} --format json`
  const cliBoundary = agentCliBoundaryInstruction(runtime, locale)

  if (locale === "zh-CN") {
    if (scope.kind === "project-drain") {
      return `请通过 MyOpenPanels 排空 Project ${scope.projectId} 中的任务。${cliBoundary}执行下面的 Task Handoff 命令，按照返回的 ExecutionBundle 和 Delivery Contract 工作；每完成一个任务后继续处理 Runtime 返回的下一项，直到 scopeState 为 complete 或 blocked：\n\n${command}`
    }
    if (scope.kind === "wiki-mutation-drain") {
      return `请通过 MyOpenPanels 处理 Project ${scope.projectId} 中 Wiki mutation ${scope.mutationKey} 的串行更新队列。${cliBoundary}执行下面的 Task Handoff 命令，按照返回的 ExecutionBundle 和 Delivery Contract 工作；只处理这个 Wiki mutation 范围内返回的任务，不要处理 Project 中的其他任务，直到 scopeState 为 complete 或 blocked：\n\n${command}`
    }
    return `请通过 MyOpenPanels 只处理任务 ${scope.taskId}。${cliBoundary}执行下面的 Task Handoff 命令，并按照返回的 ExecutionBundle 和 Delivery Contract 完成这个任务；完成后即停止，不要领取或处理其他任务：\n\n${command}`
  }
  if (scope.kind === "project-drain") {
    return `Use MyOpenPanels to drain tasks in Project ${scope.projectId}. ${cliBoundary} Run the Task Handoff command below, follow its ExecutionBundle and Delivery Contract, and continue with each Task returned by the Runtime until scopeState is complete or blocked:\n\n${command}`
  }
  if (scope.kind === "wiki-mutation-drain") {
    return `Use MyOpenPanels to process the serial Wiki mutation queue ${scope.mutationKey} in Project ${scope.projectId}. ${cliBoundary} Run the Task Handoff command below and follow its ExecutionBundle and Delivery Contract. Process only Tasks returned within this Wiki mutation scope. Do not process other Project tasks; stop when scopeState is complete or blocked:\n\n${command}`
  }
  return `Use MyOpenPanels to process only Task ${scope.taskId}. ${cliBoundary} Run the Task Handoff command below and follow its ExecutionBundle and Delivery Contract for this Task. Stop after it completes. Do not claim or process another Task:\n\n${command}`
}

export function retryTaskAgentMessage(
  taskId: string,
  locale: MyOpenPanelsLocale,
  runtime: AgentRuntimeIdentity
): string {
  const cli = agentCliExecutable(runtime)
  const quotedTaskId = shellQuote(taskId)
  const readCommand = `${cli} task read --task-id ${quotedTaskId} --format json`
  const retryCommand = `${cli} task retry --task-id ${quotedTaskId} --format json`
  const cliBoundary = agentCliBoundaryInstruction(runtime, locale)

  if (locale === "zh-CN") {
    return `请通过 MyOpenPanels 重试失败任务 ${taskId}。${cliBoundary}先运行下面的读取命令，确认该任务仍处于可重试的终态；确认后只运行一次重试命令。报告新任务的 id、status 和 ready，不要领取或执行新任务：\n\n${readCommand}\n\n${retryCommand}`
  }
  return `Use MyOpenPanels to retry failed Task ${taskId}. ${cliBoundary} First run the read command below and confirm the Task is still in a retryable terminal state. Then run the retry command exactly once. Report the new Task id, status, and readiness; do not claim or execute the new Task:\n\n${readCommand}\n\n${retryCommand}`
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
  const phase = taskDisplayPhase(task)
  if (task.ready && phase === "failed") return 0
  if (task.ready && phase === "waiting") return 1
  if (!task.ready && phase === "failed") return 2
  if (!task.ready && phase === "waiting") return 3
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
  if (error instanceof Error) return error.message
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
      return taskDisplayPhase(task) === "waiting"
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
  return taskIsActive(task)
}

export function isDoneTask(task: ProjectTask): boolean {
  return taskIsTerminal(task)
}

export function canArchiveTask(task: ProjectTask): boolean {
  return isDoneTask(task)
}

export function isPendingTask(task: ProjectTask): boolean {
  return taskDisplayPhase(task) === "waiting"
}

function isTaskReadyForManualAgent(task: ProjectTask): boolean {
  return Boolean(task.ready) && taskDisplayPhase(task) === "waiting"
}

export function pendingTaskCount(tasks: ProjectTask[]): number {
  return tasks.filter(isPendingTask).length
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

export function taskStatusTone(status: ProjectTask["status"]): string {
  const phase = taskDisplayPhase({ status })
  if (phase === "failed") return "danger"
  if (phase === "waiting") return "warning"
  if (phase === "running") return "active"
  if (phase === "succeeded") return "success"
  return "muted"
}

export function taskStatusColor(status: ProjectTask["status"]) {
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
