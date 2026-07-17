import {
  Button,
  Chip,
  Disclosure,
  ListBox,
  Modal,
  Select,
  Switch,
  Tabs,
} from "@heroui/react"
import {
  AlertCircle,
  CheckCircle2,
  GripVertical,
  KeyRound,
  LoaderCircle,
  PlugZap,
  RefreshCw,
  SquareTerminal,
} from "lucide-react"
import { useEffect, useRef, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { MODEL_GATEWAY_SETTINGS_CHANGED_EVENT } from "../../constants"
import { apiJson, apiJsonWithTimeout } from "../../lib/api"
import type {
  LocalCliConnectionTestResult,
  LocalCliInfo,
  ModelGatewaySettings,
  MyOpenPanelsTransport,
} from "../../types"

type TestState =
  | { status: "idle" }
  | { status: "running"; providerId: string }
  | {
      status: "done"
      providerId: string
      result: LocalCliConnectionTestResult
    }

type LocalCliScanResponse = {
  cached?: boolean
  localClis: LocalCliInfo[]
}

const EMPTY_SETTINGS: ModelGatewaySettings = {
  maxConcurrency: 2,
  mode: "localCli",
  localCli: {
    providerId: "codex",
    model: null,
    reasoning: null,
    providerModels: {},
    providerReasoning: {},
    enabledProviderIds: ["codex"],
    providerOrder: ["codex"],
    executablePaths: {},
  },
  byok: { providerId: null, baseUrl: null, model: null },
}

export function withLocalCliProviderOrder(
  settings: ModelGatewaySettings,
  providerOrder: string[]
): ModelGatewaySettings {
  const normalizedOrder = [...new Set(providerOrder)]
  const providerId = normalizedOrder[0] ?? null
  const primaryChanged = providerId !== settings.localCli.providerId
  const model = providerId
    ? settings.localCli.providerModels[providerId] || "default"
    : null
  const reasoning = providerId
    ? settings.localCli.providerReasoning[providerId] || "default"
    : null
  return {
    ...settings,
    localCli: {
      ...settings.localCli,
      enabledProviderIds: normalizedOrder,
      model: primaryChanged ? model : settings.localCli.model,
      providerId,
      providerOrder: normalizedOrder,
      reasoning: primaryChanged ? reasoning : settings.localCli.reasoning,
    },
  }
}

export function withLocalCliProviderModel(
  settings: ModelGatewaySettings,
  providerId: string,
  model: string
): ModelGatewaySettings {
  const selected = settings.localCli.providerId === providerId
  return {
    ...settings,
    localCli: {
      ...settings.localCli,
      model: selected ? model : settings.localCli.model,
      providerModels: {
        ...settings.localCli.providerModels,
        [providerId]: model,
      },
      providerReasoning: {
        ...settings.localCli.providerReasoning,
        [providerId]: "default",
      },
      reasoning: selected ? "default" : settings.localCli.reasoning,
    },
  }
}

export function withLocalCliProviderReasoning(
  settings: ModelGatewaySettings,
  providerId: string,
  reasoning: string
): ModelGatewaySettings {
  const selected = settings.localCli.providerId === providerId
  return {
    ...settings,
    localCli: {
      ...settings.localCli,
      providerReasoning: {
        ...settings.localCli.providerReasoning,
        [providerId]: reasoning,
      },
      reasoning: selected ? reasoning : settings.localCli.reasoning,
    },
  }
}

export function withLocalCliProviderEnabled(
  settings: ModelGatewaySettings,
  providerId: string,
  enabled: boolean,
  available: boolean
): ModelGatewaySettings {
  if (!available) return settings
  const currentOrder = settings.localCli.providerOrder.filter(
    (id) => id !== providerId
  )
  return withLocalCliProviderOrder(
    settings,
    enabled ? [...currentOrder, providerId] : currentOrder
  )
}

export function withLocalCliProviderMoved(
  settings: ModelGatewaySettings,
  providerId: string,
  targetProviderId: string
): ModelGatewaySettings {
  const currentOrder = settings.localCli.providerOrder
  const currentIndex = currentOrder.indexOf(providerId)
  const targetIndex = currentOrder.indexOf(targetProviderId)
  if (currentIndex < 0 || targetIndex < 0 || currentIndex === targetIndex) {
    return settings
  }
  const providerOrder = [...currentOrder]
  providerOrder.splice(currentIndex, 1)
  providerOrder.splice(targetIndex, 0, providerId)
  return withLocalCliProviderOrder(settings, providerOrder)
}

const scanLocalClis = (
  apiBase: string,
  executablePaths: Record<string, string>
) =>
  apiJson<LocalCliScanResponse>(apiBase, "/api/model-gateway/local-clis", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ executablePaths }),
  })

export function ModelGatewaySettingsDialog({
  isOpen,
  onOpenChange,
  transport,
}: {
  isOpen: boolean
  onOpenChange: (isOpen: boolean) => void
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const [activeTab, setActiveTab] = useState<"localCli" | "byok">("localCli")
  const [settings, setSettings] = useState<ModelGatewaySettings>(EMPTY_SETTINGS)
  const [localClis, setLocalClis] = useState<LocalCliInfo[]>([])
  const [isLoading, setIsLoading] = useState(false)
  const [isScanning, setIsScanning] = useState(false)
  const [isSaving, setIsSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)
  const [testState, setTestState] = useState<TestState>({ status: "idle" })
  const [expandedProviderIds, setExpandedProviderIds] = useState<Set<string>>(
    new Set()
  )
  const [showUninstalled, setShowUninstalled] = useState(false)
  const [draggedProviderId, setDraggedProviderId] = useState<string | null>(
    null
  )
  const [dropTargetProviderId, setDropTargetProviderId] = useState<
    string | null
  >(null)
  const settingsRef = useRef(settings)
  const confirmedSettingsRef = useRef(settings)
  const saveQueueRef = useRef<Promise<void>>(Promise.resolve())
  const saveRevisionRef = useRef(0)
  const savedTimerRef = useRef<number | null>(null)

  useEffect(() => {
    if (!isOpen) return
    let cancelled = false
    setShowUninstalled(false)
    setIsLoading(true)
    setError(null)
    Promise.all([
      apiJson<{ settings: ModelGatewaySettings }>(
        transport.apiBase,
        "/api/model-gateway/settings"
      ),
      apiJson<LocalCliScanResponse>(
        transport.apiBase,
        "/api/model-gateway/local-clis"
      ),
    ])
      .then(([settingsResponse, scanResponse]) => {
        if (cancelled) return
        settingsRef.current = settingsResponse.settings
        confirmedSettingsRef.current = settingsResponse.settings
        setSettings(settingsResponse.settings)
        setLocalClis(scanResponse.localClis)
        setExpandedProviderIds((current) => {
          if (
            current.size > 0 ||
            !settingsResponse.settings.localCli.providerId
          )
            return current
          return new Set([settingsResponse.settings.localCli.providerId])
        })
        if (scanResponse.cached) {
          setIsScanning(true)
          scanLocalClis(
            transport.apiBase,
            settingsResponse.settings.localCli.executablePaths
          )
            .then((response) => {
              if (!cancelled) setLocalClis(response.localClis)
            })
            .catch((cause) => {
              if (!cancelled) setError(String(cause?.message || cause))
            })
            .finally(() => {
              if (!cancelled) setIsScanning(false)
            })
        }
      })
      .catch((cause) => {
        if (!cancelled) setError(String(cause?.message || cause))
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [isOpen, transport.apiBase])

  useEffect(
    () => () => {
      if (savedTimerRef.current !== null) {
        window.clearTimeout(savedTimerRef.current)
      }
    },
    []
  )

  const queueSettingsSave = (nextSettings: ModelGatewaySettings) => {
    const revision = ++saveRevisionRef.current
    if (savedTimerRef.current !== null) {
      window.clearTimeout(savedTimerRef.current)
      savedTimerRef.current = null
    }
    setSaved(false)
    setIsSaving(true)
    setError(null)
    saveQueueRef.current = saveQueueRef.current
      .catch(() => undefined)
      .then(async () => {
        const response = await apiJson<{ settings: ModelGatewaySettings }>(
          transport.apiBase,
          "/api/model-gateway/settings",
          {
            method: "PUT",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              settings: { ...nextSettings, mode: "localCli" },
            }),
          }
        )
        confirmedSettingsRef.current = response.settings
        window.dispatchEvent(new Event(MODEL_GATEWAY_SETTINGS_CHANGED_EVENT))
        if (revision !== saveRevisionRef.current) return
        settingsRef.current = response.settings
        setSettings(response.settings)
        setIsSaving(false)
        setSaved(true)
        savedTimerRef.current = window.setTimeout(() => {
          setSaved(false)
          savedTimerRef.current = null
        }, 1800)
      })
      .catch((cause) => {
        if (revision !== saveRevisionRef.current) return
        const confirmedSettings = confirmedSettingsRef.current
        settingsRef.current = confirmedSettings
        setSettings(confirmedSettings)
        setIsSaving(false)
        setError(String((cause as Error)?.message || cause))
      })
  }

  const updateSettings = (
    update: (current: ModelGatewaySettings) => ModelGatewaySettings
  ) => {
    const current = settingsRef.current
    const next = update(current)
    if (next === current) return
    settingsRef.current = next
    setSettings(next)
    queueSettingsSave(next)
  }

  const rescan = async () => {
    setIsScanning(true)
    setError(null)
    try {
      const response = await scanLocalClis(
        transport.apiBase,
        settings.localCli.executablePaths
      )
      setLocalClis(response.localClis)
    } catch (cause) {
      setError(String((cause as Error)?.message || cause))
    } finally {
      setIsScanning(false)
    }
  }

  const testLocalCli = async (cli: LocalCliInfo) => {
    setTestState({ status: "running", providerId: cli.id })
    setError(null)
    try {
      const result = await apiJsonWithTimeout<LocalCliConnectionTestResult>(
        transport.apiBase,
        "/api/model-gateway/local-clis/test",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            providerId: cli.id,
            model:
              settings.localCli.providerModels[cli.id] ||
              (settings.localCli.providerId === cli.id
                ? settings.localCli.model
                : null),
            reasoning:
              settings.localCli.providerReasoning[cli.id] ||
              (settings.localCli.providerId === cli.id
                ? settings.localCli.reasoning
                : null),
            executablePath: settings.localCli.executablePaths[cli.id] || null,
          }),
        },
        130_000
      )
      setTestState({ status: "done", providerId: cli.id, result })
    } catch (cause) {
      setTestState({
        status: "done",
        providerId: cli.id,
        result: {
          ok: false,
          kind: "requestFailed",
          latencyMs: 0,
          providerId: cli.id,
          providerName: cli.name,
          detail: String((cause as Error)?.message || cause),
        },
      })
    }
  }

  const installedCount = localClis.filter((cli) => cli.available).length
  const orderedLocalClis = [
    ...settings.localCli.providerOrder
      .map((providerId) => localClis.find((cli) => cli.id === providerId))
      .filter((cli): cli is LocalCliInfo => Boolean(cli)),
    ...localClis.filter(
      (cli) => !settings.localCli.providerOrder.includes(cli.id)
    ),
  ]
  const installedLocalClis = orderedLocalClis.filter((cli) => cli.available)
  const uninstalledLocalClis = localClis.filter((cli) => !cli.available)

  const setProviderOrder = (providerOrder: string[]) => {
    updateSettings((current) =>
      withLocalCliProviderOrder(current, providerOrder)
    )
    setTestState({ status: "idle" })
  }

  const toggleProvider = (cli: LocalCliInfo, enabled: boolean) => {
    updateSettings((current) =>
      withLocalCliProviderEnabled(current, cli.id, enabled, cli.available)
    )
    setTestState({ status: "idle" })
  }

  const moveProvider = (providerId: string, offset: -1 | 1) => {
    const currentOrder = settingsRef.current.localCli.providerOrder
    const currentIndex = currentOrder.indexOf(providerId)
    const nextIndex = currentIndex + offset
    if (currentIndex < 0 || nextIndex < 0 || nextIndex >= currentOrder.length)
      return
    const providerOrder = [...currentOrder]
    ;[providerOrder[currentIndex], providerOrder[nextIndex]] = [
      providerOrder[nextIndex],
      providerOrder[currentIndex],
    ]
    setProviderOrder(providerOrder)
  }

  const moveProviderTo = (providerId: string, targetProviderId: string) => {
    updateSettings((current) =>
      withLocalCliProviderMoved(current, providerId, targetProviderId)
    )
    setTestState({ status: "idle" })
  }

  return (
    <Modal.Backdrop isOpen={isOpen} onOpenChange={onOpenChange} variant="blur">
      <Modal.Container size="lg">
        <Modal.Dialog className="op-model-settings">
          <Modal.CloseTrigger aria-label={t`Close`} />
          <Modal.Header className="op-model-settings__header">
            <div>
              <Modal.Heading>{t`Models and Agents`}</Modal.Heading>
              <p>{t`Task execution`}</p>
            </div>
          </Modal.Header>
          <Modal.Body className="op-model-settings__body">
            <Tabs
              className="op-model-settings__tabs"
              onSelectionChange={(key) =>
                setActiveTab(String(key) as "localCli" | "byok")
              }
              selectedKey={activeTab}
            >
              <Tabs.ListContainer>
                <Tabs.List aria-label={t`Execution mode`}>
                  <Tabs.Tab id="localCli">
                    <SquareTerminal size={15} />
                    <span>{t`Local CLI`}</span>
                    <Chip size="sm" variant="soft">
                      {installedCount}
                    </Chip>
                    <Tabs.Indicator />
                  </Tabs.Tab>
                  <Tabs.Tab id="byok">
                    <KeyRound size={15} />
                    <span>{t`BYOK API`}</span>
                    <Tabs.Indicator />
                  </Tabs.Tab>
                </Tabs.List>
              </Tabs.ListContainer>
              <Tabs.Panel id="localCli">
                <div className="op-model-settings__toolbar">
                  <span>
                    {isLoading
                      ? t`Scanning local CLIs`
                      : `${installedCount} ${t`installed`}`}
                  </span>
                  <div className="op-model-settings__toolbar-actions">
                    <span
                      aria-live="polite"
                      className="op-model-settings__save-status"
                    >
                      {isSaving ? (
                        <>
                          <LoaderCircle className="op-spin" size={13} />
                          {t`Saving`}
                        </>
                      ) : saved ? (
                        <>
                          <CheckCircle2 size={13} />
                          {t`Saved`}
                        </>
                      ) : null}
                    </span>
                    <Button
                      isPending={isScanning}
                      onPress={rescan}
                      size="sm"
                      variant="ghost"
                    >
                      <RefreshCw
                        className={isScanning ? "op-spin" : undefined}
                        size={14}
                      />
                      {t`Rescan`}
                    </Button>
                  </div>
                </div>
                <div aria-busy={isLoading} className="op-cli-list">
                  {installedLocalClis.map((cli) => {
                    const selected = settings.localCli.providerId === cli.id
                    const providerModel =
                      settings.localCli.providerModels[cli.id] ||
                      (selected ? settings.localCli.model : null) ||
                      "default"
                    const modelReasoningOptions =
                      cli.models.find((model) => model.id === providerModel)
                        ?.reasoningOptions || []
                    const reasoningOptions = modelReasoningOptions.length
                      ? modelReasoningOptions
                      : cli.reasoningOptions
                    const providerReasoning =
                      settings.localCli.providerReasoning[cli.id] ||
                      (selected ? settings.localCli.reasoning : null) ||
                      "default"
                    const enabled =
                      cli.available &&
                      settings.localCli.enabledProviderIds.includes(cli.id)
                    const expanded = expandedProviderIds.has(cli.id)
                    const orderIndex = settings.localCli.providerOrder.indexOf(
                      cli.id
                    )
                    const testing =
                      testState.status === "running" &&
                      testState.providerId === cli.id
                    const testResult =
                      testState.status === "done" &&
                      testState.providerId === cli.id
                        ? testState.result
                        : null
                    return (
                      <div
                        className="op-cli-drag-slot"
                        key={cli.id}
                        onDragLeave={() => {
                          if (dropTargetProviderId === cli.id) {
                            setDropTargetProviderId(null)
                          }
                        }}
                        onDragOver={(event) => {
                          if (
                            !draggedProviderId ||
                            draggedProviderId === cli.id ||
                            !enabled
                          )
                            return
                          event.preventDefault()
                          event.dataTransfer.dropEffect = "move"
                          setDropTargetProviderId(cli.id)
                        }}
                        onDrop={(event) => {
                          if (!(draggedProviderId && enabled)) return
                          event.preventDefault()
                          moveProviderTo(draggedProviderId, cli.id)
                          setDraggedProviderId(null)
                          setDropTargetProviderId(null)
                        }}
                      >
                        <Disclosure
                          className={`op-cli-item ${selected ? "op-cli-item--selected" : ""} ${expanded ? "op-cli-item--expanded" : ""} ${draggedProviderId === cli.id ? "op-cli-item--dragging" : ""} ${dropTargetProviderId === cli.id ? "op-cli-item--drop-target" : ""}`}
                          isExpanded={expanded}
                          onExpandedChange={(isExpanded) =>
                            setExpandedProviderIds((current) => {
                              const next = new Set(current)
                              if (isExpanded) next.add(cli.id)
                              else next.delete(cli.id)
                              return next
                            })
                          }
                        >
                          <div className="op-cli-item__header">
                            <Disclosure.Heading className="op-cli-item__heading">
                              <Disclosure.Trigger
                                aria-label={`${expanded ? t`Collapse module` : t`Expand module`}: ${cli.name}`}
                                className="op-cli-item__select"
                              >
                                <span className="op-cli-item__icon">
                                  <SquareTerminal size={18} />
                                </span>
                                <span className="op-cli-item__identity">
                                  <strong>{cli.name}</strong>
                                  <span>
                                    {cli.available
                                      ? cli.version || cli.path
                                      : t`Not installed`}
                                  </span>
                                </span>
                              </Disclosure.Trigger>
                            </Disclosure.Heading>
                            <div className="op-cli-item__controls">
                              <Chip
                                color={cli.available ? "success" : "default"}
                                size="sm"
                                variant="soft"
                              >
                                {cli.available ? t`Connected` : t`Unavailable`}
                              </Chip>
                              <Switch
                                aria-label={`${enabled ? t`Deactivate` : t`Activate`}: ${cli.name}`}
                                isDisabled={!cli.available}
                                isSelected={enabled}
                                onChange={(isSelected) =>
                                  toggleProvider(cli, isSelected)
                                }
                              >
                                <Switch.Content>
                                  <Switch.Control>
                                    <Switch.Thumb />
                                  </Switch.Control>
                                  <span className="op-cli-item__switch-label">
                                    {enabled ? t`Active` : t`Inactive`}
                                  </span>
                                </Switch.Content>
                              </Switch>
                              {enabled ? (
                                <button
                                  aria-label={t`Drag to reorder`}
                                  className="op-cli-item__drag-handle"
                                  draggable
                                  onDragEnd={() => {
                                    setDraggedProviderId(null)
                                    setDropTargetProviderId(null)
                                  }}
                                  onDragStart={(event) => {
                                    setDraggedProviderId(cli.id)
                                    event.dataTransfer.effectAllowed = "move"
                                    event.dataTransfer.setData(
                                      "application/x-myopenpanels-cli-provider",
                                      cli.id
                                    )
                                  }}
                                  onKeyDown={(event) => {
                                    if (
                                      event.key === "ArrowUp" &&
                                      orderIndex > 0
                                    ) {
                                      event.preventDefault()
                                      moveProvider(cli.id, -1)
                                    } else if (
                                      event.key === "ArrowDown" &&
                                      orderIndex <
                                        settings.localCli.providerOrder.length -
                                          1
                                    ) {
                                      event.preventDefault()
                                      moveProvider(cli.id, 1)
                                    }
                                  }}
                                  type="button"
                                >
                                  <GripVertical size={16} />
                                </button>
                              ) : null}
                            </div>
                          </div>
                          <Disclosure.Content>
                            <Disclosure.Body className="op-cli-item__config">
                              <div className="op-cli-item__fields">
                                <div className="op-cli-item__field">
                                  <span>{t`Model`}</span>
                                  <Select
                                    aria-label={t`Model`}
                                    onChange={(key) =>
                                      updateSettings((current) =>
                                        withLocalCliProviderModel(
                                          current,
                                          cli.id,
                                          String(key)
                                        )
                                      )
                                    }
                                    selectionMode="single"
                                    value={providerModel}
                                    variant="secondary"
                                  >
                                    <Select.Trigger>
                                      <Select.Value />
                                      <Select.Indicator />
                                    </Select.Trigger>
                                    <Select.Popover>
                                      <ListBox>
                                        {cli.models.map((model) => (
                                          <ListBox.Item
                                            id={model.id}
                                            key={model.id}
                                            textValue={model.label}
                                          >
                                            {model.label}
                                          </ListBox.Item>
                                        ))}
                                      </ListBox>
                                    </Select.Popover>
                                  </Select>
                                </div>
                                {reasoningOptions.length ? (
                                  <div className="op-cli-item__field">
                                    <span>{t`Reasoning`}</span>
                                    <Select
                                      aria-label={t`Reasoning`}
                                      onChange={(key) =>
                                        updateSettings((current) =>
                                          withLocalCliProviderReasoning(
                                            current,
                                            cli.id,
                                            String(key)
                                          )
                                        )
                                      }
                                      selectionMode="single"
                                      value={providerReasoning}
                                      variant="secondary"
                                    >
                                      <Select.Trigger>
                                        <Select.Value />
                                        <Select.Indicator />
                                      </Select.Trigger>
                                      <Select.Popover>
                                        <ListBox>
                                          {reasoningOptions.map((option) => (
                                            <ListBox.Item
                                              id={option.id}
                                              key={option.id}
                                              textValue={option.label}
                                            >
                                              {option.label}
                                            </ListBox.Item>
                                          ))}
                                        </ListBox>
                                      </Select.Popover>
                                    </Select>
                                  </div>
                                ) : null}
                              </div>
                              <div className="op-cli-item__path-row">
                                <div className="op-cli-item__path">
                                  <span>{t`Executable path`}</span>
                                  <div
                                    className="op-cli-item__path-value"
                                    title={
                                      settings.localCli.executablePaths[
                                        cli.id
                                      ] ||
                                      cli.path ||
                                      cli.bin
                                    }
                                  >
                                    {settings.localCli.executablePaths[
                                      cli.id
                                    ] ||
                                      cli.path ||
                                      cli.bin}
                                  </div>
                                </div>
                                <Button
                                  isDisabled={!cli.available}
                                  isPending={testing}
                                  onPress={() => testLocalCli(cli)}
                                  size="sm"
                                  variant="outline"
                                >
                                  {testing ? (
                                    <LoaderCircle
                                      className="op-spin"
                                      size={14}
                                    />
                                  ) : (
                                    <PlugZap size={14} />
                                  )}
                                  {t`Test`}
                                </Button>
                              </div>
                              {cli.diagnostic || cli.authMessage ? (
                                <p className="op-cli-item__diagnostic">
                                  <AlertCircle size={14} />
                                  <span>
                                    {cli.diagnostic || cli.authMessage}
                                  </span>
                                </p>
                              ) : null}
                              {testResult ? (
                                <span
                                  className={`op-cli-test-result ${testResult.ok ? "op-cli-test-result--ok" : "op-cli-test-result--error"}`}
                                  role={testResult.ok ? "status" : "alert"}
                                >
                                  {testResult.ok ? (
                                    <CheckCircle2 size={14} />
                                  ) : (
                                    <AlertCircle size={14} />
                                  )}
                                  {testResult.ok
                                    ? t`Connection successful`
                                    : testResult.detail || t`Connection failed`}
                                </span>
                              ) : null}
                            </Disclosure.Body>
                          </Disclosure.Content>
                        </Disclosure>
                      </div>
                    )
                  })}
                </div>
                {uninstalledLocalClis.length ? (
                  <Disclosure
                    className="op-cli-uninstalled"
                    isExpanded={showUninstalled}
                    onExpandedChange={setShowUninstalled}
                  >
                    <Disclosure.Heading>
                      <Disclosure.Trigger className="op-cli-uninstalled__trigger">
                        <span>
                          {uninstalledLocalClis.length} {t`Not installed`}
                        </span>
                        <Disclosure.Indicator />
                      </Disclosure.Trigger>
                    </Disclosure.Heading>
                    <Disclosure.Content>
                      <Disclosure.Body className="op-cli-uninstalled__body">
                        <div className="op-cli-list">
                          {uninstalledLocalClis.map((cli) => (
                            <div
                              className="op-cli-item op-cli-item--uninstalled"
                              key={cli.id}
                            >
                              <div className="op-cli-item__header">
                                <div className="op-cli-item__summary">
                                  <span className="op-cli-item__icon">
                                    <SquareTerminal size={18} />
                                  </span>
                                  <span className="op-cli-item__identity">
                                    <strong>{cli.name}</strong>
                                    <span>{cli.bin}</span>
                                  </span>
                                </div>
                                <span className="op-cli-item__install-state">
                                  {t`Not installed`}
                                </span>
                              </div>
                            </div>
                          ))}
                        </div>
                      </Disclosure.Body>
                    </Disclosure.Content>
                  </Disclosure>
                ) : null}
              </Tabs.Panel>
              <Tabs.Panel id="byok">
                <div className="op-byok-placeholder">
                  <span className="op-byok-placeholder__icon">
                    <KeyRound size={20} />
                  </span>
                  <div>
                    <strong>{t`BYOK API providers`}</strong>
                    <span>{t`Coming in a later release`}</span>
                  </div>
                  <Chip size="sm" variant="soft">
                    {t`Reserved`}
                  </Chip>
                </div>
              </Tabs.Panel>
            </Tabs>
            {error ? (
              <div className="op-model-settings__error" role="alert">
                <AlertCircle size={15} />
                <span>{error}</span>
              </div>
            ) : null}
          </Modal.Body>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}
