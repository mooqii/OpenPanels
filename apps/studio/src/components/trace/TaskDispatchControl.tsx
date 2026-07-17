import { Button, ListBox, Select, Tooltip } from "@heroui/react"
import { Copy } from "lucide-react"
import { useEffect, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type {
  LocalCliInfo,
  ModelGatewaySettings,
  ProjectTask,
} from "../../types"
import { formatTaskError } from "./trace-utils"

export function TaskDispatchControl({
  apiBase,
  onOpenManualTask,
  showManualInstruction,
  task,
}: {
  apiBase: string
  onOpenManualTask: (task: ProjectTask) => void
  showManualInstruction: boolean
  task: ProjectTask
}) {
  const { t } = useMyOpenPanelsI18n()
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
        task.wikiUpdateGroup
          ? `${apiBase}/api/tasks/wiki-update-groups/dispatch`
          : `${apiBase}/api/tasks/${encodeURIComponent(task.id)}/dispatch`,
        {
          body: JSON.stringify({
            mode,
            modelGatewayConnectionId: connectionId,
            ...(task.wikiUpdateGroup
              ? { mutationKey: task.wikiUpdateGroup.mutationKey }
              : {}),
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
      <div className="op-agent-task__dispatch-controls">
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
        {showManualInstruction ? (
          <Tooltip closeDelay={0} delay={300}>
            <Button
              aria-label={t`Copy task instruction`}
              isIconOnly
              onPress={() => onOpenManualTask(task)}
              size="sm"
              variant="secondary"
            >
              <Copy size={14} />
            </Button>
            <Tooltip.Content placement="top">
              {t`Copy task instruction`}
            </Tooltip.Content>
          </Tooltip>
        ) : null}
      </div>
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
