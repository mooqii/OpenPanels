import { Button, ListBox, Select, Tooltip } from "@heroui/react"
import { Copy } from "lucide-react"
import { useEffect, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type {
  LocalCliInfo,
  ModelGatewaySettings,
  ProjectTask,
  TaskExecutionScope,
} from "../../types"
import { formatTaskError, taskExecutionScope } from "./trace-utils"

export function TaskDispatchControl({
  apiBase,
  hasUsableAgentCli,
  onOpenManualTask,
  task,
}: {
  apiBase: string
  hasUsableAgentCli: boolean | null
  onOpenManualTask: (scope: TaskExecutionScope) => void
  task: ProjectTask
}) {
  const { t } = useMyOpenPanelsI18n()
  const [channels, setChannels] = useState<LocalCliInfo[]>([])
  const [selection, setSelection] = useState(() => taskDispatchSelection(task))
  const [isSaving, setIsSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const canDispatch = ["waiting", "queued", "failed"].includes(task.status)
  const requiresManualInstruction = canDispatch && hasUsableAgentCli === false
  const currentTaskSelection = taskDispatchSelection(task, canDispatch)

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

  const assignedSelection = currentTaskSelection.startsWith("assigned:")
    ? currentTaskSelection
    : null
  const executionSelection = canDispatch
    ? null
    : taskExecutionSelection(task, channels, t)
  const selectedProviderId = currentTaskSelection.split(":", 2)[1]
  const selectedProviderIsMissing =
    !assignedSelection &&
    selectedProviderId &&
    !channels.some((channel) => channel.id === selectedProviderId)

  return (
    <div className="op-agent-task__dispatch">
      <div className="op-agent-task__dispatch-controls">
        <Select
          aria-label={t`Task channel`}
          isDisabled={isSaving || !canDispatch || requiresManualInstruction}
          onChange={(key) => {
            if (canDispatch && !requiresManualInstruction) {
              updateDispatch(String(key))
            }
          }}
          selectionMode="single"
          value={
            requiresManualInstruction
              ? "manual-unavailable"
              : (executionSelection?.id ?? selection)
          }
          variant="secondary"
        >
          <Select.Trigger>
            <Select.Value />
            <Select.Indicator />
          </Select.Trigger>
          <Select.Popover>
            <ListBox>
              {executionSelection ? (
                <ListBox.Item
                  id={executionSelection.id}
                  key={executionSelection.id}
                  textValue={executionSelection.label}
                >
                  {executionSelection.label}
                </ListBox.Item>
              ) : requiresManualInstruction ? (
                <ListBox.Item
                  id="manual-unavailable"
                  key="manual-unavailable"
                  textValue={t`Send instruction manually`}
                >
                  {t`Send instruction manually`}
                </ListBox.Item>
              ) : (
                <>
                  <ListBox.Item id="auto" key="auto" textValue="Automatic">
                    {t`Automatic`}
                  </ListBox.Item>
                  {assignedSelection && task.assignedTarget ? (
                    <ListBox.Item
                      id={assignedSelection}
                      key={assignedSelection}
                      textValue={task.assignedTarget.name}
                    >
                      {task.assignedTarget.name}
                    </ListBox.Item>
                  ) : null}
                  {selectedProviderIsMissing ? (
                    <ListBox.Item
                      id={currentTaskSelection}
                      key={currentTaskSelection}
                      textValue={`${t`Prefer`} ${selectedProviderId}`}
                    >
                      {t`Prefer`} {selectedProviderId}
                    </ListBox.Item>
                  ) : null}
                  {channels.map((channel) => (
                    <ListBox.Item
                      id={`prefer:${channel.id}`}
                      key={`prefer:${channel.id}`}
                      textValue={`Prefer ${channel.name}`}
                    >
                      {t`Prefer`} {channel.name}
                    </ListBox.Item>
                  ))}
                </>
              )}
            </ListBox>
          </Select.Popover>
        </Select>
        {canDispatch && hasUsableAgentCli === true ? (
          <Tooltip closeDelay={0} delay={300}>
            <Button
              aria-label={t`Copy task instruction`}
              isIconOnly
              onPress={() => onOpenManualTask(taskExecutionScope(task))}
              size="sm"
              variant="secondary"
            >
              <Copy size={14} />
            </Button>
            <Tooltip.Content placement="top">
              {t`Copy task instruction`}
            </Tooltip.Content>
          </Tooltip>
        ) : requiresManualInstruction ? (
          <Button
            className="op-agent-task__copy-instruction"
            onPress={() => onOpenManualTask(taskExecutionScope(task))}
            size="sm"
            variant="secondary"
          >
            <Copy size={14} />
            {t`Copy instruction`}
          </Button>
        ) : null}
      </div>
      {error ? <small role="alert">{error}</small> : null}
    </div>
  )
}

function taskExecutionSelection(
  task: ProjectTask,
  channels: LocalCliInfo[],
  t: (input: TemplateStringsArray | string, ...values: unknown[]) => string
): { id: string; label: string } | null {
  const method = task.executionMethod
  if (!method) return null
  if (method.kind === "manualInstruction") {
    return { id: "execution:manual", label: t`Manual instruction` }
  }
  if (method.kind === "localCli" && method.providerId) {
    const provider = channels.find(
      (channel) => channel.id === method.providerId
    )
    return {
      id: `execution:local-cli:${method.providerId}`,
      label: provider?.name ?? localCliFallbackName(method.providerId),
    }
  }
  const label = method.label?.trim()
  return label
    ? { id: `execution:target:${method.connectionId ?? label}`, label }
    : null
}

function localCliFallbackName(providerId: string): string {
  if (providerId === "codex") return "Codex CLI"
  if (providerId === "hermes") return "Hermes"
  return providerId
}

function taskDispatchSelection(
  task: ProjectTask,
  canDispatch = ["waiting", "queued", "failed"].includes(task.status)
): string {
  if (!(canDispatch || !task.assignedTarget)) {
    return `assigned:${task.assignedTarget.id}`
  }
  const providerId = task.requestedGatewayConnectionId?.replace(
    /^local-cli:/,
    ""
  )
  return task.dispatchMode && task.dispatchMode !== "auto" && providerId
    ? `${task.dispatchMode}:${providerId}`
    : "auto"
}
