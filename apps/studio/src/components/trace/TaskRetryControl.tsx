import { Button } from "@heroui/react"
import { Copy, RefreshCw } from "lucide-react"
import { useRef, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { copyTextToClipboard } from "../../lib/clipboard"
import { taskCanRetry } from "../../lib/task-status"
import type { MyOpenPanelsBuildInfo, ProjectTask } from "../../types"
import { formatTaskError, retryTaskAgentMessage } from "./trace-utils"

export function TaskRetryControl({
  apiBase,
  buildInfo,
  task,
}: {
  apiBase: string
  buildInfo?: MyOpenPanelsBuildInfo
  task: ProjectTask
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [copyStatus, setCopyStatus] = useState<"copied" | "failed" | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [isRetrying, setIsRetrying] = useState(false)
  const [retryTaskId, setRetryTaskId] = useState<string | null>(null)
  const retryInFlight = useRef(false)

  if (!taskCanRetry(task)) return null

  const runtime = buildInfo ?? {
    channel: "release" as const,
    label: "release",
    version: "unknown",
  }
  const agentMessage = retryTaskAgentMessage(task.id, locale, runtime)

  const retryTask = async () => {
    if (retryInFlight.current) return
    retryInFlight.current = true
    setError(null)
    setRetryTaskId(null)
    setIsRetrying(true)
    try {
      const response = await fetch(
        `${apiBase}/api/tasks/${encodeURIComponent(task.id)}/retry`,
        { method: "POST" }
      )
      const payload = await response.json().catch(() => null)
      if (!response.ok) {
        throw new Error(
          payload?.error || `Task retry failed (${response.status})`
        )
      }
      const nextTaskId = payload?.task?.id
      if (typeof nextTaskId !== "string" || !nextTaskId) {
        throw new Error("Task retry returned no new Task id.")
      }
      setRetryTaskId(nextTaskId)
    } catch (cause) {
      setError(t(formatTaskError(cause)))
    } finally {
      retryInFlight.current = false
      setIsRetrying(false)
    }
  }

  const copyAgentMessage = async () => {
    const copied = await copyTextToClipboard(agentMessage)
    setCopyStatus(copied ? "copied" : "failed")
  }

  return (
    <div className="op-agent-task__retry">
      <div className="op-agent-task__retry-actions">
        <Button
          aria-label={t`Retry task`}
          isDisabled={isRetrying}
          isPending={isRetrying}
          onPress={retryTask}
          size="sm"
          variant="primary"
        >
          <RefreshCw size={14} />
          {t`Retry task`}
        </Button>
        <Button
          aria-label={t`Copy Agent message`}
          onPress={copyAgentMessage}
          size="sm"
          variant="secondary"
        >
          <Copy size={14} />
          {t`Copy Agent message`}
        </Button>
      </div>
      {retryTaskId ? (
        <span className="op-agent-task__retry-status" role="status">
          {t`New retry task`}: <code>{retryTaskId}</code>
        </span>
      ) : copyStatus ? (
        <span
          className={`op-agent-task__retry-status op-agent-task__retry-status--${copyStatus}`}
          role="status"
        >
          {copyStatus === "copied" ? t`Agent message copied` : t`Copy failed`}
        </span>
      ) : null}
      {error ? (
        <span className="op-agent-task__retry-error" role="alert">
          {error}
        </span>
      ) : null}
    </div>
  )
}
