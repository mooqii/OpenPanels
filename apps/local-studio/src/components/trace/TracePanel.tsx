import { Button, Chip } from "@heroui/react"
import {
  Activity,
  ArrowDown,
  ChevronLeft,
  ChevronRight,
  Copy,
  ListTodo,
  MessageSquare,
  Pause,
  Play,
  Trash2,
} from "lucide-react"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  appendTraceEvent,
  fetchTraceSnapshot,
  formatTraceConnection,
  formatTraceTime,
} from "../../lib/api"
import type {
  OpenPanelsBuildInfo,
  OpenPanelsTransport,
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
      {isOpen ? <ChevronRight size={14} /> : <ChevronLeft size={14} />}
      {pendingCount > 0 ? (
        <span className="op-trace-toggle__dot">
          {formatTaskCount(pendingCount)}
        </span>
      ) : null}
    </Button>
  )
}

export function TaskStatusButton({
  count,
  onPress,
}: {
  count: number
  onPress: () => void
}) {
  if (count <= 0) return null
  return (
    <Button
      aria-label={`${count} pending OpenPanels task${count === 1 ? "" : "s"}`}
      className="op-task-status-button"
      isIconOnly
      onPress={onPress}
      size="sm"
      variant="danger"
    >
      <ListTodo size={13} />
      <span>{formatTaskCount(count)}</span>
    </Button>
  )
}

export function AgentPanel({
  activeTab,
  buildInfo,
  isOpen,
  onTabChange,
  tasks,
  transport,
}: {
  activeTab: AgentPanelTab
  buildInfo?: OpenPanelsBuildInfo
  isOpen: boolean
  onTabChange: (tab: AgentPanelTab) => void
  tasks: ProjectTask[]
  transport: OpenPanelsTransport
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
  const scrollRef = useRef<HTMLDivElement | null>(null)

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
      aria-label="OpenPanels Agent panel"
      className={`op-trace-panel ${isOpen ? "op-trace-panel--open" : ""}`}
    >
      <div className="op-trace-panel__body">
        <header className="op-trace-panel__header">
          <div>
            <strong>Agent</strong>
            <span>
              {activeTab === "communication"
                ? formatTraceConnection(connectionState)
                : `${pendingTaskCount(tasks)} pending task${pendingTaskCount(tasks) === 1 ? "" : "s"}`}
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
          </div>
        </header>

        <div className="op-agent-panel__tabs" role="tablist">
          <button
            aria-selected={activeTab === "tasks"}
            className={
              activeTab === "tasks"
                ? "op-agent-panel__tab op-agent-panel__tab--active"
                : "op-agent-panel__tab"
            }
            onClick={() => onTabChange("tasks")}
            role="tab"
            type="button"
          >
            <ListTodo size={14} />
            Tasks
            {pendingTaskCount(tasks) > 0 ? (
              <span>{formatTaskCount(pendingTaskCount(tasks))}</span>
            ) : null}
          </button>
          <button
            aria-selected={activeTab === "communication"}
            className={
              activeTab === "communication"
                ? "op-agent-panel__tab op-agent-panel__tab--active"
                : "op-agent-panel__tab"
            }
            onClick={() => onTabChange("communication")}
            role="tab"
            type="button"
          >
            <MessageSquare size={14} />
            Communication
          </button>
        </div>

        {activeTab === "tasks" ? (
          <TaskList tasks={tasks} />
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

function TaskList({ tasks }: { tasks: ProjectTask[] }) {
  if (!tasks.length) {
    return (
      <div className="op-agent-tasks">
        <div className="op-trace-panel__empty">No project tasks yet.</div>
      </div>
    )
  }
  return (
    <div className="op-agent-tasks">
      {tasks.map((task) => (
        <article
          className={`op-agent-task op-agent-task--${taskStatusTone(task.status)}`}
          key={task.id}
        >
          <div className="op-agent-task__topline">
            <Chip
              className="op-agent-task__queue"
              color={taskStatusColor(task.status)}
              size="sm"
              variant="soft"
            >
              {task.queue}
            </Chip>
            <span>{formatTaskTime(task.updatedAt)}</span>
          </div>
          <strong>{formatTaskType(task.type)}</strong>
          <div className="op-agent-task__meta">
            <span>{task.status}</span>
            <span>{task.panelKind}</span>
            <span>{task.targetId || task.id}</span>
          </div>
          <code>{task.id}</code>
        </article>
      ))}
    </div>
  )
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
        <span className="op-trace-event__text">{event.summary}</span>
      </button>
      <div className="op-trace-event__meta">
        <span>{event.source ?? "openpanels"}</span>
        {event.direction ? <span>{event.direction}</span> : null}
        {event.taskId ? <span>{event.taskId}</span> : null}
      </div>
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
}: {
  info: OpenPanelsBuildInfo
  isChecking: boolean
  onCheckUpdate: () => void
}) {
  const localBuildTime = info.buildTime
    ? formatLocalBuildTime(info.buildTime)
    : null
  const label =
    info.channel === "development" && localBuildTime
      ? localBuildTime
      : info.label
  return (
    <Button
      aria-label={isChecking ? "正在检查更新" : "检查更新"}
      className="op-build-badge"
      isDisabled={isChecking}
      onFocus={onCheckUpdate}
      onMouseEnter={onCheckUpdate}
      onPress={onCheckUpdate}
      size="sm"
      variant="ghost"
    >
      {isChecking ? "checking..." : label}
    </Button>
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
  if (status === "queued") return "warning"
  if (["running", "claimed", "converting", "indexing"].includes(status)) {
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
