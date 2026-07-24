import { Button, ListBox, Select, Tooltip } from "@heroui/react"
import { Copy, Info, Settings } from "lucide-react"
import { useCallback, useEffect, useMemo, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { MODEL_GATEWAY_SETTINGS_CHANGED_EVENT } from "../../constants"
import { apiJson } from "../../lib/api"
import type {
  AgentWorkerStatus,
  LocalCliInfo,
  ModelGatewaySettings,
} from "../../types"
import { formatTaskError } from "./trace-utils"

type LocalCliTone = "normal" | "disabled" | "warning"

export function WorkerStatusCard({
  apiBase,
  hasPendingTasks,
  hasUsableAgentCli,
  isActive,
  onOpenManualTask,
  onOpenModelSettings,
  projectId,
  workerStatus,
}: {
  apiBase: string
  hasPendingTasks: boolean
  hasUsableAgentCli: boolean | null
  isActive: boolean
  onOpenManualTask: (scope: {
    kind: "project-drain"
    projectId: string
  }) => void
  onOpenModelSettings: () => void
  projectId?: string
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
  const requiresManualInstruction = hasUsableAgentCli === false

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
              {requiresManualInstruction ? (
                <>
                  <p>{t`No active Agent CLI is currently available.`}</p>
                  <p>
                    {t`Use the settings button on the right to configure the Agent CLI that processes tasks.`}
                  </p>
                  <p>
                    {t`Until an Agent CLI is activated, copy the task instruction manually and send it to an Agent.`}
                  </p>
                </>
              ) : (
                <>
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
                </>
              )}
            </Tooltip.Content>
          </Tooltip>
        </div>
        <div className="op-agent-worker__controls">
          <Select
            aria-label={t`Parallel task count`}
            className={
              requiresManualInstruction
                ? "op-agent-worker__concurrency op-agent-worker__concurrency--manual"
                : "op-agent-worker__concurrency"
            }
            isDisabled={
              requiresManualInstruction || !settings || isLoading || isSaving
            }
            onChange={(key) => {
              if (!requiresManualInstruction) {
                updateConcurrency(String(key)).catch(() => undefined)
              }
            }}
            selectionMode="single"
            value={
              requiresManualInstruction
                ? "manual-unavailable"
                : String(maxConcurrency)
            }
            variant="secondary"
          >
            <Select.Trigger>
              <Select.Value />
              <Select.Indicator />
            </Select.Trigger>
            <Select.Popover placement="top end">
              <ListBox>
                {requiresManualInstruction ? (
                  <ListBox.Item
                    id="manual-unavailable"
                    textValue={t`Send instruction manually`}
                  >
                    {t`Send instruction manually`}
                  </ListBox.Item>
                ) : (
                  [1, 2, 3, 4].map((count) => (
                    <ListBox.Item
                      id={String(count)}
                      key={count}
                      textValue={`${count} ${t`parallel`}`}
                    >
                      {count} {t`parallel`}
                      <ListBox.ItemIndicator />
                    </ListBox.Item>
                  ))
                )}
              </ListBox>
            </Select.Popover>
          </Select>
          {requiresManualInstruction && projectId ? (
            <Button
              aria-label={t`Copy Project drain instruction`}
              className="op-agent-worker__project-drain"
              isDisabled={!hasPendingTasks}
              onPress={() =>
                onOpenManualTask({ kind: "project-drain", projectId })
              }
              size="sm"
              variant="ghost"
            >
              <Copy size={14} />
              {t`Copy instruction`}
            </Button>
          ) : null}
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
