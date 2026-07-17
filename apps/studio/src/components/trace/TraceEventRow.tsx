import { Button, Chip } from "@heroui/react"
import { Copy } from "lucide-react"
import { useMemo, useState } from "react"
import { formatTraceTime } from "../../lib/api"
import type { TraceEvent } from "../../types"
import { traceCategoryColor } from "./trace-utils"

export function TraceEventRow({
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
