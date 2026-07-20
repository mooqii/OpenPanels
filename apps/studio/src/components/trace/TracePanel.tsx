import { Button, Chip, ListBox, Select, Tabs, Tooltip } from "@heroui/react"
import {
  ArrowDown,
  ArrowLeft,
  Copy,
  Info,
  RefreshCw,
  Settings,
  Trash2,
  X,
} from "lucide-react"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { MODEL_GATEWAY_SETTINGS_CHANGED_EVENT } from "../../constants"
import { apiJson, appendTraceEvent, fetchTraceSnapshot } from "../../lib/api"
import type {
  AgentWorkerStatus,
  LocalCliInfo,
  ModelGatewaySettings,
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
  pendingTaskCount,
  taskMatchesFilter,
  taskStatusColor,
  taskStatusTone,
  traceEventMatchesFilter,
} from "./trace-utils"

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
  const [expandedTaskId, setExpandedTaskId] = useState<string | null>(null)
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
            {displayedTab === "tasks" &&
            hasUsableAgentCli === false &&
            tasks[0] ? (
              <Tooltip closeDelay={0} delay={300}>
                <Button
                  aria-label={t`Copy Project drain instruction`}
                  isIconOnly
                  onPress={() =>
                    onOpenManualTask({
                      kind: "project-drain",
                      projectId: tasks[0].projectId,
                    })
                  }
                  size="sm"
                  variant="ghost"
                >
                  <Copy size={15} />
                </Button>
                <Tooltip.Content placement="bottom">
                  {t`Copy Project drain instruction`}
                </Tooltip.Content>
              </Tooltip>
            ) : null}
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
  const focusedTaskIdSet = focusedTaskIds ? new Set(focusedTaskIds) : null
  const groupedTasks = groupWikiUpdateTasks(tasks)
  const filteredTasks = groupWikiUpdateTasks(
    tasks.filter((task) => !focusedTaskIdSet || focusedTaskIdSet.has(task.id))
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

  if (!tasks.length) {
    return (
      <div className="op-agent-tasks">
        <div className="op-agent-tasks__scroll">
          <div className="op-trace-panel__empty">No project tasks yet.</div>
        </div>
        <WorkerStatusCard
          apiBase={apiBase}
          isActive={isActive}
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
          <div className="op-agent-task-focus">
            <span>
              Refinement tasks <strong>{focusedTaskIds.length}</strong>
            </span>
            <Button onPress={onClearFocusedTasks} size="sm" variant="ghost">
              <ArrowLeft size={14} />
              All tasks
            </Button>
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
        {filteredTasks.length ? (
          filteredTasks.map((task) => {
            const isExpanded = expandedTaskId === task.id
            const detail = JSON.stringify(task, null, 2)
            return (
              <article
                className={`op-agent-task op-agent-task--${taskStatusTone(task.status)}`}
                key={task.id}
                ref={
                  task.id === focusedTaskIds?.[0] ? focusedTaskRef : undefined
                }
              >
                <button
                  aria-expanded={isExpanded}
                  className="op-agent-task__summary"
                  onClick={() => onExpandTask(isExpanded ? null : task.id)}
                  type="button"
                >
                  <span className="op-agent-task__topline">
                    <Chip
                      className="op-agent-task__queue"
                      color={taskStatusColor(task.status)}
                      size="sm"
                      variant="soft"
                    >
                      {task.queue}
                    </Chip>
                    <span>{formatTaskTime(task.updatedAt)}</span>
                  </span>
                  <strong>
                    {task.wikiUpdateGroup
                      ? `Wiki updates · ${task.wikiUpdateGroup.tasks.length} tasks`
                      : (task.capability ?? formatTaskType(task.type))}
                  </strong>
                  <span className="op-agent-task__meta">
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
                    {task.wikiUpdateGroup ? (
                      <span>
                        {task.wikiUpdateGroup.tasks.filter(isDoneTask).length}/
                        {task.wikiUpdateGroup.tasks.length} complete
                      </span>
                    ) : null}
                  </span>
                  {task.error &&
                  (task.status === "failed" || task.status === "cancelled") ? (
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
                </button>
                <TaskDispatchControl
                  apiBase={apiBase}
                  onOpenManualTask={onOpenManualTask}
                  showManualInstruction={hasUsableAgentCli === false}
                  task={task}
                />
                {isExpanded ? (
                  <div className="op-agent-task__detail">
                    {task.wikiUpdateGroup ? (
                      <div className="op-agent-task__members">
                        {task.wikiUpdateGroup.tasks.map((member) => (
                          <div
                            className="op-agent-task__member"
                            key={member.id}
                          >
                            <span>{formatTaskType(member.type)}</span>
                            <span>{member.status}</span>
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
        isActive={isActive}
        onOpenModelSettings={onOpenModelSettings}
        workerStatus={workerStatus}
      />
    </div>
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

  const mutate = async (action: "archive" | "cancel" | "retry") => {
    setIsMutating(true)
    try {
      await fetch(
        `${apiBase}/api/tasks/${encodeURIComponent(task.id)}/${action}`,
        { method: "POST" }
      )
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
            onPress={() => mutate("retry")}
            size="sm"
            variant="ghost"
          >
            <RefreshCw size={14} />
          </Button>
        ) : null}
        {!isDoneTask(task) && task.status !== "failed" ? (
          <Button
            aria-label="Cancel task"
            isDisabled={isMutating}
            isIconOnly
            onPress={() => mutate("cancel")}
            size="sm"
            variant="ghost"
          >
            <X size={14} />
          </Button>
        ) : null}
        {isDoneTask(task) && !task.archivedAt ? (
          <Button
            aria-label="Archive task"
            isDisabled={isMutating}
            isIconOnly
            onPress={() => mutate("archive")}
            size="sm"
            variant="ghost"
          >
            <Trash2 size={14} />
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

type LocalCliTone = "normal" | "disabled" | "warning"

function WorkerStatusCard({
  apiBase,
  isActive,
  onOpenModelSettings,
  workerStatus,
}: {
  apiBase: string
  isActive: boolean
  onOpenModelSettings: () => void
  workerStatus?: AgentWorkerStatus
}) {
  const { t } = useMyOpenPanelsI18n()
  const [settings, setSettings] = useState<ModelGatewaySettings | null>(null)
  const [localClis, setLocalClis] = useState<LocalCliInfo[]>([])
  const [isLoading, setIsLoading] = useState(false)
  const [isSaving, setIsSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setIsLoading(true)
    setError(null)
    try {
      const [settingsResponse, scanResponse] = await Promise.all([
        apiJson<{ settings: ModelGatewaySettings }>(
          apiBase,
          "/api/model-gateway/settings"
        ),
        apiJson<{ localClis: LocalCliInfo[] }>(
          apiBase,
          "/api/model-gateway/local-clis"
        ),
      ])
      setSettings(settingsResponse.settings)
      setLocalClis(scanResponse.localClis)
    } catch (cause) {
      setError(formatTaskError(cause))
    } finally {
      setIsLoading(false)
    }
  }, [apiBase])

  useEffect(() => {
    if (!isActive) return
    refresh().catch(() => undefined)
    const onSettingsChanged = () => {
      refresh().catch(() => undefined)
    }
    window.addEventListener(
      MODEL_GATEWAY_SETTINGS_CHANGED_EVENT,
      onSettingsChanged
    )
    return () =>
      window.removeEventListener(
        MODEL_GATEWAY_SETTINGS_CHANGED_EVENT,
        onSettingsChanged
      )
  }, [isActive, refresh])

  const orderedLocalClis = useMemo(() => {
    const providerOrder = settings?.localCli.providerOrder ?? []
    return [
      ...providerOrder
        .map((providerId) => localClis.find((cli) => cli.id === providerId))
        .filter((cli): cli is LocalCliInfo => Boolean(cli)),
      ...localClis.filter((cli) => !providerOrder.includes(cli.id)),
    ].filter((cli) => cli.available)
  }, [localClis, settings])
  const maxConcurrency = settings?.maxConcurrency ?? 2
  const enabledProviderIds = settings?.localCli.enabledProviderIds ?? []
  const enabledLocalClis = orderedLocalClis.filter((cli) =>
    enabledProviderIds.includes(cli.id)
  )

  const statusForCli = (cli: LocalCliInfo): LocalCliTone => {
    if (!enabledProviderIds.includes(cli.id)) return "disabled"
    const target = workerStatus?.queue?.targets?.find(
      (candidate) =>
        candidate.modelGatewayConnectionId === `local-cli:${cli.id}`
    )
    return cli.authStatus === "missing" ||
      Boolean(cli.authMessage) ||
      Boolean(cli.diagnostic) ||
      target?.status === "offline" ||
      Boolean(target?.lastError)
      ? "warning"
      : "normal"
  }

  const updateConcurrency = async (value: string) => {
    const nextConcurrency = Number(value)
    if (!(settings && Number.isInteger(nextConcurrency))) return
    const previous = settings
    const next = { ...settings, maxConcurrency: nextConcurrency }
    setSettings(next)
    setIsSaving(true)
    setError(null)
    try {
      const response = await apiJson<{ settings: ModelGatewaySettings }>(
        apiBase,
        "/api/model-gateway/settings",
        {
          body: JSON.stringify({ settings: next }),
          headers: { "content-type": "application/json" },
          method: "PUT",
        }
      )
      setSettings(response.settings)
      window.dispatchEvent(new Event(MODEL_GATEWAY_SETTINGS_CHANGED_EVENT))
    } catch (cause) {
      setSettings(previous)
      setError(formatTaskError(cause))
    } finally {
      setIsSaving(false)
    }
  }

  return (
    <section aria-label={t`Task processing`} className="op-agent-worker">
      <div className="op-agent-worker__header">
        <div className="op-agent-worker__title">
          <strong>{t`Task processing`}</strong>
          <Tooltip closeDelay={0} delay={0}>
            <Button
              aria-label={t`How tasks are processed`}
              className="op-agent-worker__info"
              isIconOnly
              size="sm"
              variant="ghost"
            >
              <Info size={13} />
            </Button>
            <Tooltip.Content
              className="op-agent-worker__tooltip"
              placement="top end"
              showArrow
            >
              <Tooltip.Arrow />
              <p>
                {t`Enabled Agent CLIs claim queued tasks from left to right in priority order.`}
              </p>
              <p>
                {t`Current priority:`}{" "}
                {enabledLocalClis.length
                  ? enabledLocalClis.map((cli) => cli.name).join(" > ")
                  : t`No enabled automatic task channel.`}
              </p>
              <p>
                {t`This project can process up to`} {maxConcurrency}{" "}
                {t`tasks at the same time.`}
              </p>
            </Tooltip.Content>
          </Tooltip>
        </div>
        <div className="op-agent-worker__controls">
          <Select
            aria-label={t`Parallel task count`}
            className="op-agent-worker__concurrency"
            isDisabled={!settings || isLoading || isSaving}
            onChange={(key) => {
              updateConcurrency(String(key)).catch(() => undefined)
            }}
            selectionMode="single"
            value={String(maxConcurrency)}
            variant="secondary"
          >
            <Select.Trigger>
              <Select.Value />
              <Select.Indicator />
            </Select.Trigger>
            <Select.Popover placement="top end">
              <ListBox>
                {[1, 2, 3, 4, 5].map((count) => (
                  <ListBox.Item
                    id={String(count)}
                    key={count}
                    textValue={`${count} ${t`parallel`}`}
                  >
                    {count} {t`parallel`}
                    <ListBox.ItemIndicator />
                  </ListBox.Item>
                ))}
              </ListBox>
            </Select.Popover>
          </Select>
          <Tooltip closeDelay={0} delay={300}>
            <Button
              aria-label={t`Model and Agent settings`}
              className="op-agent-worker__settings"
              isIconOnly
              onPress={onOpenModelSettings}
              size="sm"
              variant="ghost"
            >
              <Settings size={15} />
            </Button>
            <Tooltip.Content placement="top">
              {t`Model and Agent settings`}
            </Tooltip.Content>
          </Tooltip>
        </div>
      </div>
      <div className="op-agent-worker__agents">
        {isLoading && !settings ? (
          <span className="op-agent-worker__loading">
            {t`Scanning available Agent CLIs`}
          </span>
        ) : orderedLocalClis.length ? (
          orderedLocalClis.map((cli) => {
            const tone = statusForCli(cli)
            const statusLabel =
              tone === "normal"
                ? t`Running normally`
                : tone === "disabled"
                  ? t`Disabled`
                  : t`Connection needs attention`
            return (
              <span
                aria-label={`${cli.name}: ${statusLabel}`}
                className="op-agent-worker__agent"
                key={cli.id}
                role="img"
                title={statusLabel}
              >
                <i
                  aria-hidden="true"
                  className={`op-agent-worker__dot op-agent-worker__dot--${tone}`}
                />
                {cli.name}
              </span>
            )
          })
        ) : (
          <p className="op-agent-worker__empty">
            {t`No task-processing model is available. Send each task's instructions to an Agent manually.`}
          </p>
        )}
      </div>
      {error ? (
        <p className="op-agent-worker__error" role="alert">
          {error}
        </p>
      ) : null}
    </section>
  )
}
