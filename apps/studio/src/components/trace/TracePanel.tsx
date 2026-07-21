import { AlertDialog, Button, Chip, Tabs, Tooltip } from "@heroui/react"
import { ArrowDown, ArrowLeft, Copy, RefreshCw, Trash2, X } from "lucide-react"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { appendTraceEvent, fetchTraceSnapshot } from "../../lib/api"
import type {
  AgentWorkerStatus,
  MyOpenPanelsBuildInfo,
  MyOpenPanelsTransport,
  ProjectTask,
  TaskExecutionScope,
  TraceCategory,
  TraceEvent,
} from "../../types"
import { TaskDispatchControl } from "./TaskDispatchControl"
import { TraceEventRow } from "./TraceEventRow"
import type { TraceFilter } from "./trace-utils"
import {
  canDeleteTask,
  compareTasksForDisplay,
  formatBlockedReason,
  formatDispatchState,
  formatTaskCount,
  formatTaskError,
  formatTaskTime,
  formatTaskType,
  groupWikiUpdateTasks,
  isActiveTask,
  isDoneTask,
  isPendingTask,
  pendingTaskCount,
  taskDeleteNeedsConfirmation,
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
    if (!isDevelopment) {
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
  }, [isDevelopment, transport])

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
  const { t } = useMyOpenPanelsI18n()
  const [deleteError, setDeleteError] = useState<string | null>(null)
  const [deletingTaskId, setDeletingTaskId] = useState<string | null>(null)
  const [hiddenTaskIds, setHiddenTaskIds] = useState<Set<string>>(
    () => new Set()
  )
  const [pendingDeleteTask, setPendingDeleteTask] =
    useState<ProjectTask | null>(null)
  const focusedTaskIdSet = focusedTaskIds ? new Set(focusedTaskIds) : null
  const visibleTasks = tasks.filter((task) => !hiddenTaskIds.has(task.id))
  const groupedTasks = groupWikiUpdateTasks(visibleTasks)
  const filteredTasks = groupWikiUpdateTasks(
    visibleTasks.filter(
      (task) => !focusedTaskIdSet || focusedTaskIdSet.has(task.id)
    )
  )
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
    { id: "pending", label: "Pending", count: pendingTaskCount(groupedTasks) },
    {
      id: "active",
      label: "Active",
      count: groupedTasks.filter(isActiveTask).length,
    },
    {
      id: "done",
      label: "Closed",
      count: groupedTasks.filter(isDoneTask).length,
    },
    { id: "all", label: "All", count: groupedTasks.length },
  ]

  const deleteTask = async (task: ProjectTask) => {
    setDeletingTaskId(task.id)
    setDeleteError(null)
    try {
      const response = await fetch(
        `${apiBase}/api/tasks/${encodeURIComponent(task.id)}`,
        { method: "DELETE" }
      )
      if (!response.ok) {
        const payload = await response.json().catch(() => null)
        throw new Error(
          payload?.error || `Task deletion failed (${response.status})`
        )
      }
      setHiddenTaskIds((current) => {
        const next = new Set(current)
        next.add(task.id)
        return next
      })
      if (expandedTaskId === task.id) onExpandTask(null)
      setPendingDeleteTask(null)
    } catch (cause) {
      setDeleteError(formatTaskError(cause))
    } finally {
      setDeletingTaskId(null)
    }
  }

  const requestTaskDelete = (task: ProjectTask) => {
    setDeleteError(null)
    if (taskDeleteNeedsConfirmation(task)) {
      setPendingDeleteTask(task)
      return
    }
    deleteTask(task).catch(() => undefined)
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
                  Refinement tasks <strong>{focusedTaskIds.length}</strong>
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
        {deleteError && !pendingDeleteTask ? (
          <p className="op-agent-task-list__error" role="alert">
            {deleteError}
          </p>
        ) : null}
        {filteredTasks.length ? (
          filteredTasks.map((task) => {
            const isExpanded =
              expandedTaskId === task.id ||
              Boolean(
                expandedTaskId &&
                  task.wikiUpdateGroup?.taskIds.includes(expandedTaskId)
              )
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
                    canDeleteTask(task)
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
                        {task.status}
                      </Chip>
                      {task.wikiUpdateGroup ? (
                        <Chip size="sm" variant="soft">
                          {task.wikiUpdateGroup.tasks.filter(isDoneTask).length}
                          /{task.wikiUpdateGroup.tasks.length} complete
                        </Chip>
                      ) : null}
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
                    {task.wikiUpdateGroup
                      ? `Wiki updates · ${task.wikiUpdateGroup.tasks.length} tasks`
                      : (task.capability ?? formatTaskType(task.type))}
                  </strong>
                </button>
                <TaskDeleteButton
                  isDeleting={deletingTaskId === task.id}
                  onDelete={requestTaskDelete}
                  task={task}
                />
                <TaskDispatchControl
                  apiBase={apiBase}
                  hasUsableAgentCli={hasUsableAgentCli}
                  onOpenManualTask={onOpenManualTask}
                  task={task}
                />
                {isExpanded ? (
                  <div className="op-agent-task__detail">
                    <div className="op-agent-task__meta">
                      <span>{task.queue}</span>
                      <span>{task.status}</span>
                      <span
                        className={
                          task.ready
                            ? "op-agent-task__state op-agent-task__state--ready"
                            : task.blockedReason
                              ? "op-agent-task__state op-agent-task__state--blocked"
                              : "op-agent-task__state"
                        }
                      >
                        {task.ready
                          ? "ready"
                          : task.blockedReason
                            ? formatBlockedReason(task.blockedReason)
                            : "not ready"}
                      </span>
                      {task.attempt ? (
                        <span>
                          attempt {task.attempt}
                          {task.maxAttempts ? `/${task.maxAttempts}` : ""}
                        </span>
                      ) : null}
                      {task.dispatchState ? (
                        <span>{formatDispatchState(task.dispatchState)}</span>
                      ) : null}
                      {task.workflowRunId ? <span>Workflow Run</span> : null}
                      {task.assignedTarget ? (
                        <span>{task.assignedTarget.name}</span>
                      ) : null}
                      <span>{task.panelKind}</span>
                      <span>{task.targetId || task.id}</span>
                    </div>
                    {task.error &&
                    (task.status === "failed" ||
                      task.status === "cancelled") ? (
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
                    <code>{task.id}</code>
                    {task.wikiUpdateGroup ? (
                      <div className="op-agent-task__members">
                        {task.wikiUpdateGroup.tasks.map((member) => (
                          <div
                            className="op-agent-task__member"
                            key={member.id}
                          >
                            <span>{formatTaskType(member.type)}</span>
                            <span>{member.status}</span>
                            <TaskDeleteButton
                              isDeleting={deletingTaskId === member.id}
                              onDelete={requestTaskDelete}
                              task={member}
                            />
                            <code>{member.id}</code>
                          </div>
                        ))}
                      </div>
                    ) : null}
                    {!task.wikiUpdateGroup && task.workflowRunId ? (
                      <div className="op-agent-task__command">
                        <span>Workflow Run</span>
                        <code>{task.workflowRunId}</code>
                      </div>
                    ) : null}
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
                    {task.requiredProtocolVersion ? (
                      <div className="op-agent-task__command">
                        <span>Execution</span>
                        <code>
                          protocol v{task.requiredProtocolVersion} · generation{" "}
                          {task.executionGeneration ?? 0} · compatible targets{" "}
                          {task.compatibleTargetCount ?? 0}
                        </code>
                      </div>
                    ) : null}
                    {task.wikiUpdateGroup ? null : (
                      <TaskHistory apiBase={apiBase} task={task} />
                    )}
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
        hasPendingTasks={pendingTaskCount(groupedTasks) > 0}
        hasUsableAgentCli={hasUsableAgentCli}
        isActive={isActive}
        onOpenManualTask={onOpenManualTask}
        onOpenModelSettings={onOpenModelSettings}
        projectId={visibleTasks[0]?.projectId}
        workerStatus={workerStatus}
      />
      {pendingDeleteTask ? (
        <AlertDialog.Backdrop
          isOpen
          onOpenChange={(isOpen) => {
            if (!(isOpen || deletingTaskId)) {
              setPendingDeleteTask(null)
              setDeleteError(null)
            }
          }}
        >
          <AlertDialog.Container placement="center">
            <AlertDialog.Dialog>
              <AlertDialog.Header>
                <AlertDialog.Icon status="danger" />
                <AlertDialog.Heading>{t`Delete task?`}</AlertDialog.Heading>
              </AlertDialog.Header>
              <AlertDialog.Body>
                <p>
                  {t`This task is waiting to run. Deleting it will cancel its related panel content and prevent it from running.`}
                </p>
                {deleteError ? (
                  <p className="op-agent-task-list__error" role="alert">
                    {deleteError}
                  </p>
                ) : null}
              </AlertDialog.Body>
              <AlertDialog.Footer>
                <Button
                  isDisabled={Boolean(deletingTaskId)}
                  onPress={() => {
                    setPendingDeleteTask(null)
                    setDeleteError(null)
                  }}
                  variant="tertiary"
                >
                  {t`Cancel`}
                </Button>
                <Button
                  isPending={deletingTaskId === pendingDeleteTask.id}
                  onPress={() =>
                    deleteTask(pendingDeleteTask).catch(() => undefined)
                  }
                  variant="danger"
                >
                  <Trash2 size={15} />
                  {t`Delete`}
                </Button>
              </AlertDialog.Footer>
            </AlertDialog.Dialog>
          </AlertDialog.Container>
        </AlertDialog.Backdrop>
      ) : null}
    </div>
  )
}

function TaskDeleteButton({
  isDeleting,
  onDelete,
  task,
}: {
  isDeleting: boolean
  onDelete: (task: ProjectTask) => void
  task: ProjectTask
}) {
  const { t } = useMyOpenPanelsI18n()
  if (!canDeleteTask(task)) return null
  return (
    <Tooltip closeDelay={0} delay={300}>
      <Button
        aria-label={t`Delete task`}
        className="op-agent-task__delete"
        isDisabled={isDeleting}
        isIconOnly
        onPress={() => onDelete(task)}
        size="sm"
        variant="ghost"
      >
        <Trash2 size={14} />
      </Button>
      <Tooltip.Content placement="top">{t`Delete task`}</Tooltip.Content>
    </Tooltip>
  )
}

function TaskHistory({
  apiBase,
  task,
}: {
  apiBase: string
  task: ProjectTask
}) {
  const [attempts, setAttempts] = useState<unknown[]>([])
  const [events, setEvents] = useState<unknown[]>([])
  const [isMutating, setIsMutating] = useState(false)

  useEffect(() => {
    let cancelled = false
    Promise.all([
      fetch(
        `${apiBase}/api/tasks/${encodeURIComponent(task.id)}/attempts`
      ).then((response) => response.json()),
      fetch(`${apiBase}/api/tasks/${encodeURIComponent(task.id)}/events`).then(
        (response) => response.json()
      ),
    ])
      .then(([attemptPayload, eventPayload]) => {
        if (cancelled) return
        setAttempts(
          Array.isArray(attemptPayload.attempts) ? attemptPayload.attempts : []
        )
        setEvents(Array.isArray(eventPayload.events) ? eventPayload.events : [])
      })
      .catch(() => {
        if (!cancelled) {
          setAttempts([])
          setEvents([])
        }
      })
    return () => {
      cancelled = true
    }
  }, [apiBase, task.id])

  const retry = async () => {
    setIsMutating(true)
    try {
      await fetch(`${apiBase}/api/tasks/${encodeURIComponent(task.id)}/retry`, {
        method: "POST",
      })
    } finally {
      setIsMutating(false)
    }
  }
  return (
    <>
      <div className="op-agent-task__command">
        <span>History</span>
        <code>
          {attempts.length} attempts · {events.length} events
        </code>
        {task.status === "failed" ? (
          <Button
            aria-label="Retry task"
            isDisabled={isMutating}
            isIconOnly
            onPress={() => retry()}
            size="sm"
            variant="ghost"
          >
            <RefreshCw size={14} />
          </Button>
        ) : null}
      </div>
      {attempts.length || events.length ? (
        <div className="op-agent-task__json">
          <pre>{JSON.stringify({ attempts, events }, null, 2)}</pre>
        </div>
      ) : null}
    </>
  )
}
