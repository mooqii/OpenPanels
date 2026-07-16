import {
  Button,
  Chip,
  Input,
  ListBox,
  Modal,
  Select,
  Tabs,
} from "@heroui/react"
import {
  AlertCircle,
  CheckCircle2,
  KeyRound,
  LoaderCircle,
  PlugZap,
  RefreshCw,
  SquareTerminal,
} from "lucide-react"
import { useEffect, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
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

const EMPTY_SETTINGS: ModelGatewaySettings = {
  mode: "localCli",
  localCli: {
    providerId: "codex",
    model: null,
    reasoning: null,
    executablePaths: {},
  },
  byok: { providerId: null, baseUrl: null, model: null },
}

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

  useEffect(() => {
    if (!isOpen) return
    let cancelled = false
    setIsLoading(true)
    setError(null)
    Promise.all([
      apiJson<{ settings: ModelGatewaySettings }>(
        transport.apiBase,
        "/api/model-gateway/settings"
      ),
      apiJson<{ localClis: LocalCliInfo[] }>(
        transport.apiBase,
        "/api/model-gateway/local-clis"
      ),
    ])
      .then(([settingsResponse, scanResponse]) => {
        if (cancelled) return
        setSettings(settingsResponse.settings)
        setLocalClis(scanResponse.localClis)
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

  const saveSettings = async (showConfirmation = true) => {
    const response = await apiJson<{ settings: ModelGatewaySettings }>(
      transport.apiBase,
      "/api/model-gateway/settings",
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          settings: { ...settings, mode: "localCli" },
        }),
      }
    )
    setSettings(response.settings)
    if (showConfirmation) {
      setSaved(true)
      window.setTimeout(() => setSaved(false), 1800)
    }
    return response.settings
  }

  const rescan = async () => {
    setIsScanning(true)
    setError(null)
    try {
      const response = await apiJson<{ localClis: LocalCliInfo[] }>(
        transport.apiBase,
        "/api/model-gateway/local-clis",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            executablePaths: settings.localCli.executablePaths,
          }),
        }
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
            model: settings.localCli.model,
            reasoning: settings.localCli.reasoning,
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

  const selectedCli = localClis.find(
    (cli) => cli.id === settings.localCli.providerId
  )
  const installedCount = localClis.filter((cli) => cli.available).length

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
                <div className="op-cli-list" aria-busy={isLoading}>
                  {localClis.map((cli) => {
                    const selected = settings.localCli.providerId === cli.id
                    const testing =
                      testState.status === "running" &&
                      testState.providerId === cli.id
                    const testResult =
                      testState.status === "done" &&
                      testState.providerId === cli.id
                        ? testState.result
                        : null
                    return (
                      <section
                        className={`op-cli-item ${selected ? "op-cli-item--selected" : ""}`}
                        key={cli.id}
                      >
                        <button
                          aria-pressed={selected}
                          className="op-cli-item__select"
                          onClick={() => {
                            setSettings((current) => ({
                              ...current,
                              localCli: {
                                ...current.localCli,
                                providerId: cli.id,
                                model:
                                  current.localCli.providerId === cli.id
                                    ? current.localCli.model
                                    : "default",
                                reasoning:
                                  current.localCli.providerId === cli.id
                                    ? current.localCli.reasoning
                                    : "default",
                              },
                            }))
                            setTestState({ status: "idle" })
                          }}
                          type="button"
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
                          <Chip
                            color={cli.available ? "success" : "default"}
                            size="sm"
                            variant="soft"
                          >
                            {cli.available ? t`Connected` : t`Unavailable`}
                          </Chip>
                        </button>
                        {selected ? (
                          <div className="op-cli-item__config">
                            <div className="op-cli-item__fields">
                              <label>
                                <span>{t`Model`}</span>
                                <Select
                                  aria-label={t`Model`}
                                  onChange={(key) =>
                                    setSettings((current) => ({
                                      ...current,
                                      localCli: {
                                        ...current.localCli,
                                        model: String(key),
                                      },
                                    }))
                                  }
                                  selectionMode="single"
                                  value={settings.localCli.model || "default"}
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
                              </label>
                              {cli.reasoningOptions.length ? (
                                <label>
                                  <span>{t`Reasoning`}</span>
                                  <Select
                                    aria-label={t`Reasoning`}
                                    onChange={(key) =>
                                      setSettings((current) => ({
                                        ...current,
                                        localCli: {
                                          ...current.localCli,
                                          reasoning: String(key),
                                        },
                                      }))
                                    }
                                    selectionMode="single"
                                    value={
                                      settings.localCli.reasoning || "default"
                                    }
                                    variant="secondary"
                                  >
                                    <Select.Trigger>
                                      <Select.Value />
                                      <Select.Indicator />
                                    </Select.Trigger>
                                    <Select.Popover>
                                      <ListBox>
                                        {cli.reasoningOptions.map((option) => (
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
                                </label>
                              ) : null}
                            </div>
                            <label className="op-cli-item__path">
                              <span>{t`Executable path`}</span>
                              <Input
                                aria-label={t`Executable path`}
                                onChange={(event) =>
                                  setSettings((current) => ({
                                    ...current,
                                    localCli: {
                                      ...current.localCli,
                                      executablePaths: {
                                        ...current.localCli.executablePaths,
                                        [cli.id]: event.currentTarget.value,
                                      },
                                    },
                                  }))
                                }
                                placeholder={cli.path || cli.bin}
                                value={
                                  settings.localCli.executablePaths[cli.id] ||
                                  ""
                                }
                                variant="secondary"
                              />
                            </label>
                            {cli.diagnostic || cli.authMessage ? (
                              <p className="op-cli-item__diagnostic">
                                <AlertCircle size={14} />
                                <span>{cli.diagnostic || cli.authMessage}</span>
                              </p>
                            ) : null}
                            <div className="op-cli-item__actions">
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
                              ) : (
                                <span />
                              )}
                              <Button
                                isPending={testing}
                                onPress={() => testLocalCli(cli)}
                                size="sm"
                                variant="outline"
                              >
                                {testing ? (
                                  <LoaderCircle className="op-spin" size={14} />
                                ) : (
                                  <PlugZap size={14} />
                                )}
                                {t`Test`}
                              </Button>
                            </div>
                          </div>
                        ) : null}
                      </section>
                    )
                  })}
                </div>
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
          <Modal.Footer>
            <div className="op-model-settings__footer-status">
              {saved ? (
                <>
                  <CheckCircle2 size={14} />
                  <span>{t`Saved`}</span>
                </>
              ) : selectedCli ? (
                <span>{selectedCli.name}</span>
              ) : null}
            </div>
            <Button onPress={() => onOpenChange(false)} variant="ghost">
              {t`Cancel`}
            </Button>
            <Button
              isDisabled={!settings.localCli.providerId}
              isPending={isSaving}
              onPress={async () => {
                setIsSaving(true)
                setError(null)
                try {
                  await saveSettings()
                } catch (cause) {
                  setError(String((cause as Error)?.message || cause))
                } finally {
                  setIsSaving(false)
                }
              }}
              variant="primary"
            >
              {t`Save settings`}
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}
