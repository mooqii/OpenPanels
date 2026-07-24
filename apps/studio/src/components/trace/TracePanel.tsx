import { Button, Chip, Tabs, Tooltip } from "@heroui/react"
import {
  Archive,
  ArrowDown,
  ArrowLeft,
  Copy,
  LoaderCircle,
  Trash2,
  X,
} from "lucide-react"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { appendTraceEvent, fetchTraceSnapshot } from "../../lib/api"
import { taskDisplayPhase } from "../../lib/task-status"
import type {
  AgentWorkerStatus,
  MyOpenPanelsBuildInfo,
  MyOpenPanelsTransport,
  ProjectTask,
  TaskExecutionScope,
  TraceCategory,
  TraceEvent,
} from "../../types"
import { TaskHandoffControl } from "./TaskHandoffControl"
import { TaskRetryControl } from "./TaskRetryControl"
import { TraceEventRow } from "./TraceEventRow"
import type { TraceFilter } from "./trace-utils"
import {
  canArchiveTask,
  compareTasksForDisplay,
  formatBlockedReason,
  formatDispatchState,
  formatTaskCount,
  formatTaskError,
  formatTaskTime,
  formatTaskType,
  isActiveTask,
  isDoneTask,
  isPendingTask,
  pendingTaskCount,
  taskMatchesFilter,
  taskStatusColor,
  traceEventMatchesFilter,
} from "./trace-utils"
import { WorkerStatusCard } from "./WorkerStatusCard"

const TRACE_CATEGORIES: TraceCategory[] = [
  "cli",
  "agent",
  "api",
  "task",
  "system",
  "error",
]
const TRACE_FILTERS: TraceFilter[] = ["all", ...TRACE_CATEGORIES]

export type AgentPanelTab = "tasks" | "communication"
export type TaskFilter = "pending" | "active" | "done" | "all"

export function AgentPanel({
  activeTab,
  buildInfo,
  focusedTaskIds,
  hasUsableAgentCli,
  isOpen,
  onClearFocusedTasks,
  onClose,
  onOpenModelSettings,
  onOpenManualTask,
  onTabChange,
  onTaskFilterChange,
  taskFilter,
  tasks,
  transport,
  workerStatus,
}: {
  activeTab: AgentPanelTab
  buildInfo?: MyOpenPanelsBuildInfo
  focusedTaskIds: string[] | null
  hasUsableAgentCli: boolean | null
  isOpen: boolean
  onClearFocusedTasks: () => void
  onClose: () => void
  onOpenModelSettings: () => void
  onOpenManualTask: (scope: TaskExecutionScope) => void
  onTabChange: (tab: AgentPanelTab) => void
  onTaskFilterChange: (filter: TaskFilter) => void
  taskFilter: TaskFilter
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
  workerStatus?: AgentWorkerStatus
}) {
  const isDevelopment = buildInfo?.channel === "development"
  const displayedTab = isDevelopment ? activeTab : "tasks"
  const [traceFilter, setTraceFilter] = useState<TraceFilter>("all")
  const [events, setEvents] = useState<TraceEvent[]>([])
  const [isFollowing, setIsFollowing] = useState(true)
  const [expandedTaskId, setExpandedTaskId] = useState<string | null>(
    focusedTaskIds?.[0] ?? null
  )
  const scrollRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (focusedTaskIds?.length) setExpandedTaskId(focusedTaskIds[0])
  }, [focusedTaskIds])

  useEffect(() => {
    if (!(isDevelopment && isOpen)) {
      setEvents([])
      return
    }
    let cancelled = false
    fetchTraceSnapshot(transport, "development")
      .then((snapshot) => {
        if (!cancelled) setEvents(snapshot.events)
      })
      .catch(() => undefined)

    const source = new EventSource(
      `${transport.apiBase}/api/trace/stream?audience=development`
    )
    source.addEventListener("trace", (message) => {
      try {
        const event = JSON.parse((message as MessageEvent).data) as TraceEvent
        setEvents((current) => appendTraceEvent(current, event))
      } catch {
        // Ignore malformed trace frames; the panel should never disturb work.
      }
    })
    return () => {
      cancelled = true
      source.close()
    }
  }, [isDevelopment, isOpen, transport])

  const visibleEvents = useMemo(() => {
    if (!isDevelopment) return []
    return events.filter((event) => traceEventMatchesFilter(event, traceFilter))
  }, [events, isDevelopment, traceFilter])
  const latestVisibleSeq = visibleEvents.at(-1)?.seq ?? 0

  useEffect(() => {
    if (!(isOpen && isFollowing)) return
    if (latestVisibleSeq < 0) return
    const element = scrollRef.current
    if (!element) return
    element.scrollTop = element.scrollHeight
  }, [isFollowing, isOpen, latestVisibleSeq])

  const onScroll = useCallback(() => {
    const element = scrollRef.current
    if (!element) return
    const distanceToBottom =
      element.scrollHeight - element.scrollTop - element.clientHeight
    setIsFollowing(distanceToBottom < 80)
  }, [])

  return (
    <aside
      aria-label="MyOpenPanels Agent panel"
      className={`op-trace-panel ${isOpen ? "op-trace-panel--open" : ""}`}
    >
      <div className="op-trace-panel__body">
        <header className="op-trace-panel__header">
          <div className="op-trace-panel__heading">
            {isDevelopment ? (
              <Tabs
                className="op-agent-panel__page-tabs"
                onSelectionChange={(key) =>
                  onTabChange(String(key) as AgentPanelTab)
                }
                selectedKey={displayedTab}
                variant="secondary"
              >
                <Tabs.ListContainer>
                  <Tabs.List aria-label="Agent panel pages">
                    <Tabs.Tab id="tasks">
                      Tasks
                      <Tabs.Indicator />
                    </Tabs.Tab>
                    <Tabs.Tab id="communication">
                      Communication
                      <Tabs.Indicator />
                    </Tabs.Tab>
                  </Tabs.List>
                </Tabs.ListContainer>
              </Tabs>
            ) : (
              <strong>Tasks</strong>
            )}
          </div>
          <div className="op-trace-panel__actions">
            {displayedTab === "communication" ? (
              <Button
                aria-label="Clear communication view"
                isIconOnly
                onPress={() => setEvents([])}
                size="sm"
                variant="ghost"
              >
                <Trash2 size={15} />
              </Button>
            ) : null}
            <Button
              aria-label="关闭 Agent 面板"
              isIconOnly
              onPress={onClose}
              size="sm"
              variant="ghost"
            >
              <X size={16} />
            </Button>
          </div>
        </header>

        {displayedTab === "tasks" ? (
          <TaskList
            apiBase={transport.apiBase}
            buildInfo={buildInfo}
            expandedTaskId={expandedTaskId}
            filter={taskFilter}
            focusedTaskIds={focusedTaskIds}
            hasUsableAgentCli={hasUsableAgentCli}
            isActive={isOpen}
            onClearFocusedTasks={onClearFocusedTasks}
            onExpandTask={setExpandedTaskId}
            onFilterChange={onTaskFilterChange}
            onOpenManualTask={onOpenManualTask}
            onOpenModelSettings={onOpenModelSettings}
            tasks={tasks}
            workerStatus={workerStatus}
          />
        ) : (
          <section className="op-trace-panel__communication">
            <Tabs
              className="op-trace-panel__filters"
              onSelectionChange={(key) =>
                setTraceFilter(String(key) as TraceFilter)
              }
              selectedKey={traceFilter}
              variant="secondary"
            >
              <Tabs.ListContainer>
                <Tabs.List aria-label="Communication event types">
                  {TRACE_FILTERS.map((filter) => (
                    <Tabs.Tab
                      className="op-trace-panel__filter"
                      id={filter}
                      key={filter}
                    >
                      {filter}
                      <Tabs.Indicator />
                    </Tabs.Tab>
                  ))}
                </Tabs.List>
              </Tabs.ListContainer>
            </Tabs>
            <div
              className="op-trace-panel__events"
              onScroll={onScroll}
              ref={scrollRef}
            >
              {visibleEvents.length ? (
                visibleEvents.map((event) => (
                  <TraceEventRow
                    event={event}
                    isDevelopment={isDevelopment}
                    key={event.id}
                  />
                ))
              ) : (
                <div className="op-trace-panel__empty">
                  No communication events in this view.
                </div>
              )}
            </div>

            {!isFollowing && visibleEvents.length ? (
              <Button
                className="op-trace-panel__jump"
                onPress={() => {
                  setIsFollowing(true)
                  const element = scrollRef.current
                  if (element) element.scrollTop = element.scrollHeight
                }}
                size="sm"
                variant="secondary"
              >
                <ArrowDown size={15} />
                Latest
              </Button>
            ) : null}
          </section>
        )}
      </div>
    </aside>
  )
}

function TaskList({
  apiBase,
  buildInfo,
  expandedTaskId,
  filter,
  focusedTaskIds,
  hasUsableAgentCli,
  isActive,
  onClearFocusedTasks,
  onExpandTask,
  onFilterChange,
  onOpenModelSettings,
  onOpenManualTask,
  tasks,
  workerStatus,
}: {
  apiBase: string
  buildInfo?: MyOpenPanelsBuildInfo
  expandedTaskId: string | null
  filter: TaskFilter
  focusedTaskIds: string[] | null
  hasUsableAgentCli: boolean | null
  isActive: boolean
  onClearFocusedTasks: () => void
  onExpandTask: (taskId: string | null) => void
  onFilterChange: (filter: TaskFilter) => void
  onOpenModelSettings: () => void
  onOpenManualTask: (scope: TaskExecutionScope) => void
  tasks: ProjectTask[]
  workerStatus?: AgentWorkerStatus
}) {
  const [archiveError, setArchiveError] = useState<string | null>(null)
  const [archivingTaskId, setArchivingTaskId] = useState<string | null>(null)
  const [hiddenTaskIds, setHiddenTaskIds] = useState<Set<string>>(
    () => new Set()
  )
  const focusedTaskIdSet = focusedTaskIds ? new Set(focusedTaskIds) : null
  const visibleTasks = tasks.filter((task) => !hiddenTaskIds.has(task.id))
  const filteredTasks = visibleTasks
    .filter((task) => !focusedTaskIdSet || focusedTaskIdSet.has(task.id))
    .filter((task) =>
      focusedTaskIdSet ? true : taskMatchesFilter(task, filter)
    )
    .sort(compareTasksForDisplay)
  const focusedTaskRef = useRef<HTMLElement | null>(null)

  useEffect(() => {
    if (!focusedTaskIds?.length) return
    const frame = window.requestAnimationFrame(() => {
      focusedTaskRef.current?.scrollIntoView({ block: "nearest" })
    })
    return () => window.cancelAnimationFrame(frame)
  }, [focusedTaskIds])
  const filterItems: Array<{
    count: number
    id: TaskFilter
    label: string
  }> = [
    { id: "pending", label: "Pending", count: pendingTaskCount(visibleTasks) },
    {
      id: "active",
      label: "Active",
      count: visibleTasks.filter(isActiveTask).length,
    },
    {
      id: "done",
      label: "Closed",
      count: visibleTasks.filter(isDoneTask).length,
    },
    { id: "all", label: "All", count: visibleTasks.length },
  ]

  const archiveTask = async (task: ProjectTask) => {
    setArchivingTaskId(task.id)
    setArchiveError(null)
    try {
      const response = await fetch(
        `${apiBase}/api/tasks/${encodeURIComponent(task.id)}/archive`,
        { method: "POST" }
      )
      if (!response.ok) {
        const payload = await response.json().catch(() => null)
        throw new Error(
          payload?.error || `Task archive failed (${response.status})`
        )
      }
      setHiddenTaskIds((current) => {
        const next = new Set(current)
        next.add(task.id)
        return next
      })
      if (expandedTaskId === task.id) onExpandTask(null)
    } catch (cause) {
      setArchiveError(formatTaskError(cause))
    } finally {
      setArchivingTaskId(null)
    }
  }

  if (!visibleTasks.length) {
    return (
      <div className="op-agent-tasks">
        <div className="op-agent-tasks__scroll">
          <div className="op-trace-panel__empty">No project tasks yet.</div>
        </div>
        <WorkerStatusCard
          apiBase={apiBase}
          hasPendingTasks={false}
          hasUsableAgentCli={hasUsableAgentCli}
          isActive={isActive}
          onOpenManualTask={onOpenManualTask}
          onOpenModelSettings={onOpenModelSettings}
          workerStatus={workerStatus}
        />
      </div>
    )
  }
  return (
    <div className="op-agent-tasks">
      <div className="op-agent-tasks__scroll">
        {focusedTaskIds?.length ? (
          <div
            className={
              focusedTaskIds.length === 1
                ? "op-agent-task-focus op-agent-task-focus--single"
                : "op-agent-task-focus"
            }
          >
            {focusedTaskIds.length === 1 ? (
              <Tooltip>
                <Button
                  aria-label="Back to all tasks"
                  isIconOnly
                  onPress={onClearFocusedTasks}
                  size="sm"
                  variant="ghost"
                >
                  <ArrowLeft size={14} />
                </Button>
                <Tooltip.Content>All tasks</Tooltip.Content>
              </Tooltip>
            ) : (
              <>
                <span>
                  Distillation tasks <strong>{focusedTaskIds.length}</strong>
                </span>
                <Button onPress={onClearFocusedTasks} size="sm" variant="ghost">
                  <ArrowLeft size={14} />
                  All tasks
                </Button>
              </>
            )}
          </div>
        ) : (
          <div className="op-agent-task-filters">
            {filterItems.map((item) => (
              <button
                aria-label={`${item.label} tasks (${item.count})`}
                aria-pressed={filter === item.id}
                className={
                  filter === item.id
                    ? "op-agent-task-filter op-agent-task-filter--active"
                    : "op-agent-task-filter"
                }
                key={item.id}
                onClick={() => onFilterChange(item.id)}
                type="button"
              >
                <span>{item.label}</span>
                <strong
                  className={
                    item.id === "pending" && item.count > 0
                      ? "op-agent-task-filter__count op-agent-task-filter__count--danger"
                      : "op-agent-task-filter__count"
                  }
                >
                  {formatTaskCount(item.count)}
                </strong>
              </button>
            ))}
          </div>
        )}
        {archiveError ? (
          <p className="op-agent-task-list__error" role="alert">
            {archiveError}
          </p>
        ) : null}
        {filteredTasks.length ? (
          filteredTasks.map((task) => {
            const isExpanded = expandedTaskId === task.id
            const detail = JSON.stringify(task, null, 2)
            return (
              <article
                className="op-agent-task"
                key={task.id}
                ref={
                  task.id === focusedTaskIds?.[0] ? focusedTaskRef : undefined
                }
              >
                <button
                  aria-expanded={isExpanded}
                  className={
                    canArchiveTask(task)
                      ? "op-agent-task__summary op-agent-task__summary--deletable"
                      : "op-agent-task__summary"
                  }
                  onClick={() => onExpandTask(isExpanded ? null : task.id)}
                  type="button"
                >
                  <span className="op-agent-task__topline">
                    <span className="op-agent-task__statusline">
                      <Chip
                        className="op-agent-task__status"
                        color={taskStatusColor(task.status)}
                        size="sm"
                        variant="soft"
                      >
                        <Chip.Label>{task.status}</Chip.Label>
                        {task.status === "running" ? (
                          <LoaderCircle
                            aria-hidden="true"
                            className="op-agent-task__status-spinner"
                            size={13}
                          />
                        ) : null}
                      </Chip>
                      {isPendingTask(task) && task.ready === false ? (
                        <Chip color="warning" size="sm" variant="soft">
                          {task.blockedReason
                            ? formatBlockedReason(task.blockedReason)
                            : "not ready"}
                        </Chip>
                      ) : null}
                    </span>
                    <time>{formatTaskTime(task.updatedAt)}</time>
                  </span>
                  <strong>
                    {task.capability ?? formatTaskType(task.type)}
                  </strong>
                </button>
                <TaskArchiveButton
                  isArchiving={archivingTaskId === task.id}
                  onArchive={(selected) =>
                    archiveTask(selected).catch(() => undefined)
                  }
                  task={task}
                />
                <TaskHandoffControl
                  onOpenManualTask={onOpenManualTask}
                  task={task}
                />
                {isExpanded ? (
                  <div className="op-agent-task__detail">
                    <div className="op-agent-task__meta">
                      <span>{task.queue}</span>
                      <span>{task.status}</span>
                      {task.attempt ? (
                        <span>
                          attempt {task.attempt}
                          {task.attemptLimit ? `/${task.attemptLimit}` : ""}
                        </span>
                      ) : null}
                      {task.dispatchState ? (
                        <span>{formatDispatchState(task.dispatchState)}</span>
                      ) : null}
                      <span>{task.panelKind}</span>
                      <span>{task.targetId || task.id}</span>
                    </div>
                    {task.error &&
                    (taskDisplayPhase(task) === "failed" ||
                      taskDisplayPhase(task) === "cancelled") ? (
                      <span className="op-agent-task__note">
                        {formatTaskError(task.error)}
                      </span>
                    ) : null}
                    {task.nextRunAt ? (
                      <span className="op-agent-task__note">
                        Next run {formatTaskTime(task.nextRunAt)}
                      </span>
                    ) : task.lease?.expiresAt &&
                      task.blockedReason === "leased" ? (
                      <span className="op-agent-task__note">
                        Lease until {formatTaskTime(task.lease.expiresAt)}
                      </span>
                    ) : null}
                    <TaskRetryControl
                      apiBase={apiBase}
                      buildInfo={buildInfo}
                      task={task}
                    />
                    <code>{task.id}</code>
                    {task.dependencies?.length ? (
                      <div className="op-agent-task__command">
                        <span>Prerequisites</span>
                        <code>
                          {task.dependencies
                            .map(
                              (dependency) =>
                                `${dependency.prerequisiteTaskId} · ${dependency.status} · ${dependency.failurePolicy}`
                            )
                            .join("\n")}
                        </code>
                      </div>
                    ) : null}
                    {task.executionGeneration !== undefined ||
                    task.compatibleTargetCount !== undefined ? (
                      <div className="op-agent-task__command">
                        <span>Execution</span>
                        <code>
                          generation {task.executionGeneration ?? 0} ·
                          compatible targets {task.compatibleTargetCount ?? 0}
                        </code>
                      </div>
                    ) : null}
                    <div className="op-agent-task__json">
                      <Button
                        aria-label="Copy task detail"
                        isIconOnly
                        onPress={() => navigator.clipboard?.writeText(detail)}
                        size="sm"
                        variant="ghost"
                      >
                        <Copy size={14} />
                      </Button>
                      <pre>{detail}</pre>
                    </div>
                  </div>
                ) : null}
              </article>
            )
          })
        ) : (
          <div className="op-trace-panel__empty">
            No {filter === "all" ? "project" : filter} tasks.
          </div>
        )}
      </div>
      <WorkerStatusCard
        apiBase={apiBase}
        hasPendingTasks={pendingTaskCount(visibleTasks) > 0}
        hasUsableAgentCli={hasUsableAgentCli}
        isActive={isActive}
        onOpenManualTask={onOpenManualTask}
        onOpenModelSettings={onOpenModelSettings}
        projectId={visibleTasks[0]?.projectId}
        workerStatus={workerStatus}
      />
    </div>
  )
}

function TaskArchiveButton({
  isArchiving,
  onArchive,
  task,
}: {
  isArchiving: boolean
  onArchive: (task: ProjectTask) => void
  task: ProjectTask
}) {
  const { t } = useMyOpenPanelsI18n()
  if (!canArchiveTask(task)) return null
  return (
    <Tooltip closeDelay={0} delay={300}>
      <Button
        aria-label={t`Archive task`}
        className="op-agent-task__delete"
        isDisabled={isArchiving}
        isIconOnly
        onPress={() => onArchive(task)}
        size="sm"
        variant="ghost"
      >
        <Archive size={14} />
      </Button>
      <Tooltip.Content placement="top">{t`Archive task`}</Tooltip.Content>
    </Tooltip>
  )
}
