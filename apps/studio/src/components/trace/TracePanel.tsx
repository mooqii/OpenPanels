import { Button, Chip, Tabs, Tooltip } from "@heroui/react"
import {
  Archive,
  ArrowDown,
  ArrowLeft,
  LoaderCircle,
  Trash2,
  X,
} from "lucide-react"
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
import { TaskDeleteButton } from "./TaskDeleteButton"
import { TaskDeleteConfirmationDialog } from "./TaskDeleteConfirmationDialog"
import { TaskDetail } from "./TaskDetail"
import { TaskHandoffControl } from "./TaskHandoffControl"
import { TraceEventRow } from "./TraceEventRow"
import type { TraceFilter } from "./trace-utils"
import {
  canArchiveTask,
  compareTasksForDisplay,
  formatBlockedReason,
  formatTaskCount,
  formatTaskError,
  formatTaskName,
  formatTaskTime,
  isActiveTask,
  isDoneTask,
  isPendingTask,
  taskExecutionScope,
  taskMatchesFilter,
  taskStatusColor,
  traceEventMatchesFilter,
  type WikiMutationTaskGroup,
  wikiMutationTaskGroups,
} from "./trace-utils"
import { WikiMutationTaskGroupCard } from "./WikiMutationTaskGroupCard"
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
  const { t } = useMyOpenPanelsI18n()
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
      aria-label={t`MyOpenPanels Agent panel`}
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
                  <Tabs.List aria-label={t`Agent panel pages`}>
                    <Tabs.Tab id="tasks">
                      {t`Tasks`}
                      <Tabs.Indicator />
                    </Tabs.Tab>
                    <Tabs.Tab id="communication">
                      {t`Communication`}
                      <Tabs.Indicator />
                    </Tabs.Tab>
                  </Tabs.List>
                </Tabs.ListContainer>
              </Tabs>
            ) : (
              <strong>{t`Tasks`}</strong>
            )}
          </div>
          <div className="op-trace-panel__actions">
            {displayedTab === "communication" ? (
              <Button
                aria-label={t`Clear communication view`}
                isIconOnly
                onPress={() => setEvents([])}
                size="sm"
                variant="ghost"
              >
                <Trash2 size={15} />
              </Button>
            ) : null}
            <Button
              aria-label={t`Close Agent panel`}
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
                <Tabs.List aria-label={t`Communication event types`}>
                  {TRACE_FILTERS.map((filter) => (
                    <Tabs.Tab
                      className="op-trace-panel__filter"
                      id={filter}
                      key={filter}
                    >
                      {t(filter)}
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
                  {t`No communication events in this view.`}
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
                {t`Latest`}
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
  const { locale, t } = useMyOpenPanelsI18n()
  const [archiveError, setArchiveError] = useState<string | null>(null)
  const [archivingTaskId, setArchivingTaskId] = useState<string | null>(null)
  const [deletingTaskId, setDeletingTaskId] = useState<string | null>(null)
  const [taskPendingDeletion, setTaskPendingDeletion] =
    useState<ProjectTask | null>(null)
  const [hiddenTaskIds, setHiddenTaskIds] = useState<Set<string>>(
    () => new Set()
  )
  const focusedTaskIdSet = focusedTaskIds ? new Set(focusedTaskIds) : null
  const visibleTasks = tasks.filter((task) => !hiddenTaskIds.has(task.id))
  const displayItems = taskListItems(visibleTasks)
  const filteredItems = displayItems
    .filter(
      (item) =>
        !focusedTaskIdSet ||
        taskListItemTasks(item).some((task) => focusedTaskIdSet.has(task.id))
    )
    .filter((item) =>
      focusedTaskIdSet ? true : taskListItemMatchesFilter(item, filter)
    )
  const focusedTaskRef = useRef<HTMLElement | null>(null)

  useEffect(() => {
    if (!focusedTaskIds?.length) return
    const frame = window.requestAnimationFrame(() => {
      focusedTaskRef.current?.scrollIntoView({ block: "nearest" })
    })
    return () => window.cancelAnimationFrame(frame)
  }, [focusedTaskIds])
  const filterItems: Array<{
    ariaLabel: string
    count: number
    id: TaskFilter
    label: string
  }> = [
    {
      ariaLabel: t`Pending tasks`,
      id: "pending",
      label: t`Pending`,
      count: displayItems.filter(taskListItemIsPending).length,
    },
    {
      ariaLabel: t`Active tasks`,
      id: "active",
      label: t`In progress`,
      count: displayItems.filter(taskListItemIsActive).length,
    },
    {
      ariaLabel: t`Closed tasks`,
      id: "done",
      label: t`Closed`,
      count: displayItems.filter(taskListItemIsDone).length,
    },
    {
      ariaLabel: t`All tasks`,
      id: "all",
      label: t`All`,
      count: displayItems.length,
    },
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
      setArchiveError(t(formatTaskError(cause)))
    } finally {
      setArchivingTaskId(null)
    }
  }

  const deleteTask = async (task: ProjectTask) => {
    setDeletingTaskId(task.id)
    setArchiveError(null)
    try {
      const response = await fetch(
        `${apiBase}/api/tasks/${encodeURIComponent(task.id)}`,
        { method: "DELETE" }
      )
      const payload = await response.json().catch(() => null)
      if (!response.ok) {
        throw new Error(
          payload?.error || `Task deletion failed (${response.status})`
        )
      }
      const deletedTaskIds = Array.isArray(payload?.deletedTaskIds)
        ? payload.deletedTaskIds.filter(
            (value: unknown): value is string => typeof value === "string"
          )
        : [task.id]
      setHiddenTaskIds((current) => {
        const next = new Set(current)
        for (const taskId of deletedTaskIds) next.add(taskId)
        return next
      })
      if (expandedTaskId && deletedTaskIds.includes(expandedTaskId)) {
        onExpandTask(null)
      }
      return true
    } catch (cause) {
      setArchiveError(t(formatTaskError(cause)))
      return false
    } finally {
      setDeletingTaskId(null)
    }
  }

  const confirmTaskDeletion = async () => {
    if (!taskPendingDeletion) return
    if (await deleteTask(taskPendingDeletion)) setTaskPendingDeletion(null)
  }

  if (!visibleTasks.length) {
    return (
      <div className="op-agent-tasks">
        <div className="op-agent-tasks__scroll">
          <div className="op-trace-panel__empty">
            {t`No project tasks yet.`}
          </div>
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
                  aria-label={t`Back to all tasks`}
                  isIconOnly
                  onPress={onClearFocusedTasks}
                  size="sm"
                  variant="ghost"
                >
                  <ArrowLeft size={14} />
                </Button>
                <Tooltip.Content>{t`All tasks`}</Tooltip.Content>
              </Tooltip>
            ) : (
              <>
                <span>
                  {t`Distillation tasks`}{" "}
                  <strong>{focusedTaskIds.length}</strong>
                </span>
                <Button onPress={onClearFocusedTasks} size="sm" variant="ghost">
                  <ArrowLeft size={14} />
                  {t`All tasks`}
                </Button>
              </>
            )}
          </div>
        ) : (
          <div className="op-agent-task-filters">
            {filterItems.map((item) => (
              <button
                aria-label={`${item.ariaLabel} (${item.count})`}
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
        {filteredItems.length ? (
          filteredItems.map((item) => {
            if (item.kind === "wiki-mutation") {
              return (
                <WikiMutationTaskGroupCard
                  apiBase={apiBase}
                  buildInfo={buildInfo}
                  deletingTaskId={deletingTaskId}
                  expandedTaskId={expandedTaskId}
                  focusedTaskRef={focusedTaskRef}
                  group={item.group}
                  hasUsableAgentCli={hasUsableAgentCli}
                  isFocused={item.group.tasks.some(
                    (task) => task.id === focusedTaskIds?.[0]
                  )}
                  key={item.group.key}
                  onDeleteTask={setTaskPendingDeletion}
                  onExpandTask={onExpandTask}
                  onOpenManualTask={onOpenManualTask}
                />
              )
            }
            const task = item.task
            const isExpanded = expandedTaskId === task.id
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
                    canArchiveTask(task) || isPendingTask(task)
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
                        <Chip.Label>{t(task.status)}</Chip.Label>
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
                            ? t(formatBlockedReason(task.blockedReason))
                            : t`not ready`}
                        </Chip>
                      ) : null}
                    </span>
                    <time>{formatTaskTime(task.updatedAt, locale)}</time>
                  </span>
                  <strong>{t(formatTaskName(task))}</strong>
                </button>
                <TaskArchiveButton
                  isArchiving={archivingTaskId === task.id}
                  onArchive={(selected) =>
                    archiveTask(selected).catch(() => undefined)
                  }
                  task={task}
                />
                <TaskDeleteButton
                  isDeleting={deletingTaskId === task.id}
                  onDelete={setTaskPendingDeletion}
                  task={task}
                />
                {isPendingTask(task) ? (
                  <TaskHandoffControl
                    hasUsableAgentCli={hasUsableAgentCli}
                    onOpenManualTask={onOpenManualTask}
                    scope={taskExecutionScope(task)}
                  />
                ) : null}
                {isExpanded ? (
                  <TaskDetail
                    apiBase={apiBase}
                    buildInfo={buildInfo}
                    task={task}
                  />
                ) : null}
              </article>
            )
          })
        ) : (
          <div className="op-trace-panel__empty">
            {filter === "all"
              ? t`No project tasks.`
              : filter === "pending"
                ? t`No pending tasks.`
                : filter === "active"
                  ? t`No active tasks.`
                  : t`No closed tasks.`}
          </div>
        )}
      </div>
      <WorkerStatusCard
        apiBase={apiBase}
        hasPendingTasks={displayItems.some(taskListItemIsPending)}
        hasUsableAgentCli={hasUsableAgentCli}
        isActive={isActive}
        onOpenManualTask={onOpenManualTask}
        onOpenModelSettings={onOpenModelSettings}
        projectId={visibleTasks[0]?.projectId}
        workerStatus={workerStatus}
      />
      <TaskDeleteConfirmationDialog
        isDeleting={deletingTaskId !== null}
        onCancel={() => setTaskPendingDeletion(null)}
        onConfirm={() => confirmTaskDeletion().catch(() => undefined)}
        task={taskPendingDeletion}
      />
    </div>
  )
}

type TaskListItem =
  | { group: WikiMutationTaskGroup; kind: "wiki-mutation" }
  | { kind: "task"; task: ProjectTask }

function taskListItems(tasks: ProjectTask[]): TaskListItem[] {
  const groups = wikiMutationTaskGroups(tasks)
  const groupedTaskIds = new Set(
    groups.flatMap((group) => group.tasks.map((task) => task.id))
  )
  return [
    ...groups.map((group): TaskListItem => ({ group, kind: "wiki-mutation" })),
    ...tasks
      .filter((task) => !groupedTaskIds.has(task.id))
      .map((task): TaskListItem => ({ kind: "task", task })),
  ].sort((left, right) =>
    compareTasksForDisplay(
      taskListItemRepresentative(left),
      taskListItemRepresentative(right)
    )
  )
}

function taskListItemTasks(item: TaskListItem): ProjectTask[] {
  return item.kind === "wiki-mutation" ? item.group.tasks : [item.task]
}

function taskListItemRepresentative(item: TaskListItem): ProjectTask {
  if (item.kind === "task") return item.task
  return (
    item.group.tasks.find(isActiveTask) ??
    [...item.group.tasks].sort(compareTasksForDisplay)[0]
  )
}

function taskListItemIsActive(item: TaskListItem): boolean {
  return taskListItemTasks(item).some(isActiveTask)
}

function taskListItemIsPending(item: TaskListItem): boolean {
  return (
    !taskListItemIsActive(item) && taskListItemTasks(item).some(isPendingTask)
  )
}

function taskListItemIsDone(item: TaskListItem): boolean {
  return item.kind === "task" && isDoneTask(item.task)
}

function taskListItemMatchesFilter(
  item: TaskListItem,
  filter: TaskFilter
): boolean {
  switch (filter) {
    case "pending":
      return taskListItemIsPending(item)
    case "active":
      return taskListItemIsActive(item)
    case "done":
      return taskListItemIsDone(item)
    case "all":
      return true
    default:
      return item.kind === "task" && taskMatchesFilter(item.task, filter)
  }
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
