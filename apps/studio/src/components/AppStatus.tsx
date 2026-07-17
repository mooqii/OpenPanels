import { Button } from "@heroui/react"
import { ExternalLink } from "lucide-react"
import { apiFetch } from "../lib/api"
import { externalBrowserPath } from "../lib/browser-context"
import type { AgentOperation, MyOpenPanelsTransport } from "../types"

export function OpenBrowserPrompt({
  label,
  transport,
}: {
  label: string
  transport: MyOpenPanelsTransport
}) {
  const open = () => {
    apiFetch(transport.apiBase, "/api/studio/open-browser", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ path: externalBrowserPath(window.location) }),
    }).catch((error) => {
      console.error("Failed to open MyOpenPanels in the default browser", error)
    })
  }
  return (
    <Button
      className="op-open-browser-prompt"
      onPress={open}
      size="sm"
      variant="secondary"
    >
      <ExternalLink size={14} strokeWidth={1.8} />
      <span>{label}</span>
    </Button>
  )
}

export function BootStatus({
  error,
  failedLabel,
  loadingLabel,
}: {
  error: string | null
  failedLabel: string
  loadingLabel: string
}) {
  return (
    <main className="design-shell design-shell--status">
      <div className="op-boot-status">
        <div>{error ? failedLabel : loadingLabel}</div>
        {error ? <div className="op-boot-status__detail">{error}</div> : null}
      </div>
    </main>
  )
}

export function OperationNotice({
  completedLabel,
  failedLabel,
  notice,
}: {
  completedLabel: string
  failedLabel: string
  notice: AgentOperation
}) {
  return (
    <div
      className={`op-operation-notice${
        notice.status === "failed" ? "op-operation-notice--failed" : ""
      }`}
      role="status"
    >
      <strong>
        {notice.status === "completed" ? completedLabel : failedLabel}
      </strong>
      <span>
        {notice.projectTitle ?? notice.projectId}
        {" · "}
        {notice.panelTitle ?? notice.panelKind}
      </span>
    </div>
  )
}
