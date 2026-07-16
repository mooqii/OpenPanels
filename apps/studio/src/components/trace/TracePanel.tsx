import { Button, Chip, ListBox, Popover, Select, Tabs } from "@heroui/react"
import {
  Activity,
  ArrowDown,
  ArrowLeft,
  CheckCircle2,
  Copy,
  Download,
  ListTodo,
  MessageSquare,
  Pause,
  Play,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  appendTraceEvent,
  fetchTraceSnapshot,
  formatTraceConnection,
  formatTraceTime,
} from "../../lib/api"
import type {
  AgentWorkerStatus,
  LocalCliInfo,
  ModelGatewaySettings,
  MyOpenPanelsBuildInfo,
  MyOpenPanelsTransport,
  MyOpenPanelsUpdateStatus,
  ProjectTask,
  TraceCategory,
  TraceEvent,
} from "../../types"

const TRACE_CATEGORIES: TraceCategory[] = [
  "agent",
  "cli",
  "api",
  "task",
  "system",
  "error",
]

export type AgentPanelTab = "tasks" | "communication"
export type TaskFilter = "pending" | "active" | "done" | "all"

export function AgentToggleButton({
  isOpen,
  pendingCount,
  onToggle,
}: {
  isOpen: boolean
  pendingCount: number
  onToggle: () => void
}) {
  return (
    <Button
      aria-expanded={isOpen}
      aria-label={isOpen ? "折叠 Agent 面板" : "展开 Agent 面板"}
      className={`op-trace-toggle ${isOpen ? "op-trace-toggle--active" : ""}`}
      isIconOnly
      onPress={onToggle}
      size="sm"
      variant={isOpen ? "secondary" : "ghost"}
    >
      <Activity size={14} />
      {pendingCount > 0 ? (
        <span className="op-trace-toggle__dot">
          {formatTaskCount(pendingCount)}
        </span>
      ) : null}
    </Button>
  )
}

export function AgentPanel({
  activeTab,
  buildInfo,
  focusedTaskIds,
  isOpen,
  onClearFocusedTasks,
  onClose,
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
  isOpen: boolean
  onClearFocusedTasks: () => void
  onClose: () => void
  onTabChange: (tab: AgentPanelTab) => void
  onTaskFilterChange: (filter: TaskFilter) => void
  taskFilter: TaskFilter
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
  workerStatus?: AgentWorkerStatus
}) {
  const isDevelopment = buildInfo?.channel === "development"
  const audience = isDevelopment ? "development" : "release"
  const [isPaused, setIsPaused] = useState(false)
  const [activeCategories, setActiveCategories] = useState<Set<TraceCategory>>(
    () => new Set(TRACE_CATEGORIES)
  )
  const [events, setEvents] = useState<TraceEvent[]>([])
  const [isFollowing, setIsFollowing] = useState(true)
  const [connectionState, setConnectionState] = useState<
    "connecting" | "live" | "paused" | "offline"
  >("connecting")
  const [expandedTaskId, setExpandedTaskId] = useState<string | null>(null)
  const scrollRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (focusedTaskIds?.length) setExpandedTaskId(focusedTaskIds[0])
  }, [focusedTaskIds])

  useEffect(() => {
    if (isPaused) {
      setConnectionState("paused")
      return
    }
    let cancelled = false
    setConnectionState("connecting")
    fetchTraceSnapshot(transport, audience)
      .then((snapshot) => {
        if (!cancelled) setEvents(snapshot.events)
      })
      .catch(() => {
        if (!cancelled) setConnectionState("offline")
      })

    const source = new EventSource(
      `${transport.apiBase}/api/trace/stream?audience=${encodeURIComponent(audience)}`
    )
    source.addEventListener("open", () => setConnectionState("live"))
    source.addEventListener("error", () => setConnectionState("offline"))
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
  }, [audience, isPaused, transport])

  const visibleEvents = useMemo(() => {
    if (!isDevelopment) return events
    return events.filter((event) => activeCategories.has(event.category))
  }, [activeCategories, events, isDevelopment])
  const latestVisibleSeq = visibleEvents.at(-1)?.seq ?? 0
  const pendingTasks = pendingTaskCount(tasks)

  useEffect(() => {
    if (!(isOpen && isFollowing)) return
    if (latestVisibleSeq < 0) return
    const element = scrollRef.current
    if (!element) return
    element.scrollTop = element.scrollHeight
  }, [isFollowing, isOpen, latestVisibleSeq])

  const toggleCategory = useCallback((category: TraceCategory) => {
    setActiveCategories((current) => {
      const next = new Set(current)
      if (next.has(category)) {
        next.delete(category)
      } else {
        next.add(category)
      }
      return next.size ? next : new Set([category])
    })
  }, [])

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
          <div>
            <strong>Agent</strong>
            <span>
              {activeTab === "communication"
                ? formatTraceConnection(connectionState)
                : `${pendingTasks} pending task${pendingTasks === 1 ? "" : "s"}`}
            </span>
          </div>
          <div className="op-trace-panel__actions">
            {activeTab === "communication" ? (
              <>
                <Button
                  aria-label={
                    isPaused
                      ? "Resume communication stream"
                      : "Pause communication stream"
                  }
                  isIconOnly
                  onPress={() => setIsPaused((value) => !value)}
                  size="sm"
                  variant="ghost"
                >
                  {isPaused ? <Play size={15} /> : <Pause size={15} />}
                </Button>
                <Button
                  aria-label="Clear communication view"
                  isIconOnly
                  onPress={() => setEvents([])}
                  size="sm"
                  variant="ghost"
                >
                  <Trash2 size={15} />
                </Button>
              </>
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

        <Tabs
          className="op-agent-panel__tabs"
          onSelectionChange={(key) => onTabChange(String(key) as AgentPanelTab)}
          selectedKey={activeTab}
        >
          <Tabs.ListContainer>
            <Tabs.List aria-label="Agent panel sections">
              <Tabs.Tab className="op-agent-panel__tab" id="tasks">
                <ListTodo size={14} />
                Tasks
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab className="op-agent-panel__tab" id="communication">
                <MessageSquare size={14} />
                Communication
                <Tabs.Indicator />
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.ListContainer>
        </Tabs>

        {activeTab === "tasks" ? (
          <TaskList
            apiBase={transport.apiBase}
            expandedTaskId={expandedTaskId}
            filter={taskFilter}
            focusedTaskIds={focusedTaskIds}
            onClearFocusedTasks={onClearFocusedTasks}
            onExpandTask={setExpandedTaskId}
            onFilterChange={onTaskFilterChange}
            tasks={tasks}
            workerStatus={workerStatus}
          />
        ) : (
          <>
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
                  {isDevelopment
                    ? "No communication events in this view."
                    : "No agent activity yet."}
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
          </>
        )}

        {activeTab === "communication" && isDevelopment ? (
          <footer className="op-trace-panel__filters">
            {TRACE_CATEGORIES.map((category) => (
              <Button
                aria-pressed={activeCategories.has(category)}
                className={
                  activeCategories.has(category)
                    ? "op-trace-panel__filter op-trace-panel__filter--active"
                    : "op-trace-panel__filter"
                }
                key={category}
                onPress={() => toggleCategory(category)}
                size="sm"
                variant={activeCategories.has(category) ? "secondary" : "ghost"}
              >
                {category}
              </Button>
            ))}
          </footer>
        ) : null}
      </div>
    </aside>
  )
}

function TaskList({
  apiBase,
  expandedTaskId,
  filter,
  focusedTaskIds,
  onClearFocusedTasks,
  onExpandTask,
  onFilterChange,
  tasks,
  workerStatus,
}: {
  apiBase: string
  expandedTaskId: string | null
  filter: TaskFilter
  focusedTaskIds: string[] | null
  onClearFocusedTasks: () => void
  onExpandTask: (taskId: string | null) => void
  onFilterChange: (filter: TaskFilter) => void
  tasks: ProjectTask[]
  workerStatus?: AgentWorkerStatus
}) {
  const focusedTaskIdSet = focusedTaskIds ? new Set(focusedTaskIds) : null
  const filteredTasks = tasks
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
    { id: "pending", label: "Pending", count: pendingTaskCount(tasks) },
    { id: "active", label: "Active", count: tasks.filter(isActiveTask).length },
    { id: "done", label: "Closed", count: tasks.filter(isDoneTask).length },
    { id: "all", label: "All", count: tasks.length },
  ]

  if (!tasks.length) {
    return (
      <div className="op-agent-tasks">
        <WorkerStatusCard workerStatus={workerStatus} />
        <div className="op-trace-panel__empty">No project tasks yet.</div>
      </div>
    )
  }
  return (
    <div className="op-agent-tasks">
      <WorkerStatusCard workerStatus={workerStatus} />
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
          const command = taskCommand(task)
          return (
            <article
              className={`op-agent-task op-agent-task--${taskStatusTone(task.status)}`}
              key={task.id}
              ref={task.id === focusedTaskIds?.[0] ? focusedTaskRef : undefined}
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
                <strong>{task.capability ?? formatTaskType(task.type)}</strong>
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
                  {task.workflowId ? <span>workflow</span> : null}
                  {task.assignedTarget ? (
                    <span>{task.assignedTarget.name}</span>
                  ) : null}
                  <span>{task.panelKind}</span>
                  <span>{task.targetId || task.id}</span>
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
                ) : task.lease?.expiresAt && task.blockedReason === "leased" ? (
                  <span className="op-agent-task__note">
                    Lease until {formatTaskTime(task.lease.expiresAt)}
                  </span>
                ) : null}
                <code>{task.id}</code>
              </button>
              <TaskDispatchControl apiBase={apiBase} task={task} />
              {isExpanded ? (
                <div className="op-agent-task__detail">
                  {task.workflowId ? (
                    <div className="op-agent-task__command">
                      <span>Workflow</span>
                      <code>{task.workflowId}</code>
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
                  <TaskHistory apiBase={apiBase} task={task} />
                  {command ? (
                    <div className="op-agent-task__command">
                      <span>{command.label}</span>
                      <Button
                        aria-label="Copy task command"
                        isIconOnly
                        onPress={() =>
                          navigator.clipboard?.writeText(command.value)
                        }
                        size="sm"
                        variant="ghost"
                      >
                        <Copy size={14} />
                      </Button>
                      <code>{command.value}</code>
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
  )
}

function TaskDispatchControl({
  apiBase,
  task,
}: {
  apiBase: string
  task: ProjectTask
}) {
  const [channels, setChannels] = useState<LocalCliInfo[]>([])
  const [selection, setSelection] = useState(() => taskDispatchSelection(task))
  const [isSaving, setIsSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const canDispatch = ["waiting", "queued", "failed"].includes(task.status)
  const currentTaskSelection = taskDispatchSelection(task)

  useEffect(() => {
    setSelection(currentTaskSelection)
  }, [currentTaskSelection])

  useEffect(() => {
    let cancelled = false
    Promise.all([
      fetch(`${apiBase}/api/model-gateway/settings`).then((response) =>
        response.json()
      ),
      fetch(`${apiBase}/api/model-gateway/local-clis`).then((response) =>
        response.json()
      ),
    ])
      .then(([settingsPayload, scanPayload]) => {
        if (cancelled) return
        const settings = settingsPayload.settings as ModelGatewaySettings
        const localClis = Array.isArray(scanPayload.localClis)
          ? (scanPayload.localClis as LocalCliInfo[])
          : []
        const enabled = new Set(settings.localCli.enabledProviderIds)
        setChannels(
          settings.localCli.providerOrder
            .filter((providerId) => enabled.has(providerId))
            .map((providerId) =>
              localClis.find((channel) => channel.id === providerId)
            )
            .filter((channel): channel is LocalCliInfo => Boolean(channel))
        )
      })
      .catch(() => {
        if (!cancelled) setChannels([])
      })
    return () => {
      cancelled = true
    }
  }, [apiBase])

  const updateDispatch = async (value: string) => {
    const [mode, providerId] = value.split(":", 2)
    const connectionId = providerId ? `local-cli:${providerId}` : null
    setSelection(value)
    setIsSaving(true)
    setError(null)
    try {
      const response = await fetch(
        `${apiBase}/api/tasks/${encodeURIComponent(task.id)}/dispatch`,
        {
          body: JSON.stringify({
            mode,
            modelGatewayConnectionId: connectionId,
          }),
          headers: { "content-type": "application/json" },
          method: "PUT",
        }
      )
      if (!response.ok) {
        const payload = await response.json().catch(() => null)
        throw new Error(
          payload?.error || `Dispatch update failed (${response.status})`
        )
      }
    } catch (cause) {
      setSelection(currentTaskSelection)
      setError(formatTaskError(cause))
    } finally {
      setIsSaving(false)
    }
  }

  if (!canDispatch) return null
  return (
    <div className="op-agent-task__dispatch">
      <span>Channel</span>
      <Select
        aria-label="Task channel"
        isDisabled={isSaving}
        onChange={(key) => updateDispatch(String(key))}
        selectionMode="single"
        value={selection}
        variant="secondary"
      >
        <Select.Trigger>
          <Select.Value />
          <Select.Indicator />
        </Select.Trigger>
        <Select.Popover>
          <ListBox>
            <ListBox.Item id="auto" textValue="Automatic">
              Automatic
            </ListBox.Item>
            {channels.map((channel) => (
              <ListBox.Item
                id={`prefer:${channel.id}`}
                key={`prefer:${channel.id}`}
                textValue={`Prefer ${channel.name}`}
              >
                Prefer {channel.name}
              </ListBox.Item>
            ))}
          </ListBox>
        </Select.Popover>
      </Select>
      {error ? <small role="alert">{error}</small> : null}
    </div>
  )
}

function taskDispatchSelection(task: ProjectTask): string {
  const providerId = task.requestedGatewayConnectionId?.replace(
    /^local-cli:/,
    ""
  )
  return task.dispatchMode && task.dispatchMode !== "auto" && providerId
    ? `${task.dispatchMode}:${providerId}`
    : "auto"
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

function WorkerStatusCard({
  workerStatus,
}: {
  workerStatus?: AgentWorkerStatus
}) {
  const queue = workerStatus?.queue
  const status = queue?.status ?? workerStatus?.status ?? "idle"
  const tone =
    status === "running"
      ? "active"
      : status === "error" || status === "noTarget"
        ? "danger"
        : "success"
  return (
    <div className={`op-agent-worker op-agent-worker--${tone}`}>
      <span>
        <Activity size={13} />
        Worker
      </span>
      <strong>{formatWorkerStatus(status)}</strong>
      {queue ? (
        <small>
          {queue.onlineTargetCount ?? 0}/{queue.targetCount ?? 0} targets
        </small>
      ) : null}
      {workerStatus?.lastError ? <em>{workerStatus.lastError}</em> : null}
      {workerStatus?.heartbeatAt ? (
        <small>{formatTaskTime(workerStatus.heartbeatAt)}</small>
      ) : null}
    </div>
  )
}

function taskCommand(
  task: ProjectTask
): { label: string; value: string } | null {
  if (task.status !== "queued" && task.status !== "failed") return null
  if (!task.ready) return null
  return {
    label: "Claim with a registered target",
    value: `myopenpanels task claim --task-id ${shellQuote(task.id)} --target-id <target-id> --format json`,
  }
}

function compareTasksForDisplay(left: ProjectTask, right: ProjectTask): number {
  const rank = taskDisplayRank(left) - taskDisplayRank(right)
  if (rank !== 0) return rank
  return Date.parse(right.updatedAt) - Date.parse(left.updatedAt)
}

function taskDisplayRank(task: ProjectTask): number {
  if (task.ready && task.status === "failed") return 0
  if (task.ready && task.status === "queued") return 1
  if (!task.ready && task.status === "failed") return 2
  if (!task.ready && task.status === "queued") return 3
  return 4
}

function formatBlockedReason(reason: string): string {
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

function formatTaskError(error: unknown): string {
  if (typeof error === "string") return error
  try {
    return JSON.stringify(error)
  } catch {
    return "Task failed"
  }
}

function shellQuote(value: string): string {
  if (/^[A-Za-z0-9_./:-]+$/.test(value)) return value
  return `'${value.replaceAll("'", "'\\''")}'`
}

function taskMatchesFilter(task: ProjectTask, filter: TaskFilter): boolean {
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

function isActiveTask(task: ProjectTask): boolean {
  return [
    "reserved",
    "running",
    "claimed",
    "converting",
    "indexing",
    "cancel_requested",
  ].includes(task.status)
}

function isDoneTask(task: ProjectTask): boolean {
  return ["succeeded", "cancelled", "stale", "superseded"].includes(task.status)
}

function TraceEventRow({
  event,
  isDevelopment,
}: {
  event: TraceEvent
  isDevelopment: boolean
}) {
  const [isExpanded, setIsExpanded] = useState(false)
  const detail = useMemo(
    () => JSON.stringify(event.detail ?? event, null, 2),
    [event]
  )
  return (
    <article className={`op-trace-event op-trace-event--${event.category}`}>
      <button
        className="op-trace-event__summary"
        onClick={() => setIsExpanded((value) => !value)}
        type="button"
      >
        <span className="op-trace-event__header">
          <span className="op-trace-event__time">
            {formatTraceTime(event.timestamp)}
          </span>
          <Chip
            className="op-trace-event__badge"
            color={traceCategoryColor(event.category)}
            size="sm"
            variant="soft"
          >
            {event.category}
          </Chip>
          <span className="op-trace-event__meta">
            <span>{event.source ?? "myopenpanels"}</span>
            {event.direction ? <span>{event.direction}</span> : null}
            {event.taskId ? <span>{event.taskId}</span> : null}
          </span>
        </span>
        <span className="op-trace-event__text">{event.summary}</span>
      </button>
      {isDevelopment && isExpanded ? (
        <div className="op-trace-event__detail">
          <Button
            aria-label="Copy trace detail"
            isIconOnly
            onPress={() => navigator.clipboard?.writeText(detail)}
            size="sm"
            variant="ghost"
          >
            <Copy size={14} />
          </Button>
          <pre>{detail}</pre>
        </div>
      ) : null}
    </article>
  )
}

export function BuildVersionBadge({
  info,
  isChecking,
  onCheckUpdate,
  onUpdate,
  status,
}: {
  info: MyOpenPanelsBuildInfo
  isChecking: boolean
  onCheckUpdate: (options?: { refresh?: boolean }) => void
  onUpdate: () => void
  status: MyOpenPanelsUpdateStatus | null
}) {
  const localBuildTime = info.buildTime
    ? formatLocalBuildTime(info.buildTime)
    : null
  const label =
    info.channel === "development" && localBuildTime
      ? localBuildTime
      : info.label
  const hasUpdate = Boolean(status?.updateAvailable || status?.readyToInstall)
  const currentVersion = status?.currentVersion ?? info.version
  const latestVersion = status?.latestVersion ?? null
  const updateText = isChecking
    ? "正在检查更新"
    : hasUpdate
      ? `发现新版本 ${latestVersion ?? ""}`.trim()
      : status
        ? "当前已是最新版"
        : "点击检查更新"
  const updateDetail = status
    ? `当前 ${currentVersion}${latestVersion ? ` · 最新 ${latestVersion}` : ""}`
    : "会从 GitHub Release 获取最新状态"

  return (
    <Popover
      onOpenChange={(isOpen) => {
        if (isOpen) onCheckUpdate({ refresh: true })
      }}
    >
      <Button
        aria-label="查看版本与更新状态"
        className="op-build-badge"
        size="sm"
        variant="ghost"
      >
        {label}
      </Button>
      <Popover.Content placement="top end">
        <Popover.Dialog className="min-w-72">
          <div className="op-build-popover__status">
            <span
              className={`op-build-popover__icon ${
                isChecking
                  ? "op-build-popover__icon--checking"
                  : hasUpdate
                    ? "op-build-popover__icon--update"
                    : "op-build-popover__icon--current"
              }`}
            >
              {isChecking ? (
                <RefreshCw size={15} />
              ) : hasUpdate ? (
                <Download size={15} />
              ) : (
                <CheckCircle2 size={15} />
              )}
            </span>
            <div>
              <strong>{updateText}</strong>
              <span>{updateDetail}</span>
            </div>
          </div>
          {hasUpdate ? (
            <div className="op-build-popover__actions">
              <Button
                isDisabled={isChecking}
                onPress={onUpdate}
                size="sm"
                variant="primary"
              >
                立即更新
              </Button>
            </div>
          ) : null}
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  )
}

function pendingTaskCount(tasks: ProjectTask[]): number {
  return tasks.filter(
    (task) => task.status === "queued" || task.status === "failed"
  ).length
}

function formatTaskCount(count: number): string {
  return count > 99 ? "99+" : String(count)
}

function formatTaskType(type: string): string {
  return type
    .split("_")
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ")
}

function formatWorkerStatus(status: string): string {
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

function formatDispatchState(status: string): string {
  switch (status) {
    case "noTarget":
      return "no target"
    default:
      return status
  }
}

function formatTaskTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    month: "short",
    day: "numeric",
  }).format(date)
}

function taskStatusTone(status: string): string {
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

function taskStatusColor(status: string) {
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

function traceCategoryColor(category: TraceCategory) {
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

function formatLocalBuildTime(value: string): string | null {
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

function padDatePart(value: number): string {
  return String(value).padStart(2, "0")
}
