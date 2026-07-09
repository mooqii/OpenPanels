import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  CanvasPanel,
  type CanvasSelectionSnapshot,
  DataUrlAssetStore,
  type StoreSnapshot,
  useOpenPanelsI18n,
} from "./canvas"
import {
  BottomPanelTabs,
  OpenPanelsBrowserAssetStore,
  ProjectChrome,
} from "./components/project/ProjectChrome"
import {
  AgentPanel,
  type AgentPanelTab,
  AgentToggleButton,
  BuildVersionBadge,
  type TaskFilter,
} from "./components/trace/TracePanel"
import { UpdatePrompt } from "./components/update/UpdatePrompt"
import { WikiPanel } from "./components/wiki/WikiPanel"
import { ACTIVE_SESSION_STORAGE_KEY } from "./constants"
import {
  apiFetch,
  apiUrl,
  canvasRevisionFromState,
  canvasSnapshotFromState,
  fetchActiveSessionId,
  fetchSessions,
  fetchStudioHealth,
  fetchUpdateStatus,
  isNotFoundError,
  loadBootstrap,
  normalizeBootstrap,
  normalizePanelState,
  normalizeSnapshot,
  requestUpdateDownload,
  requestUpdateInstallRestart,
  savePanelState,
  saveSelectionState,
  wikiStateFromAppState,
} from "./lib/api"
import { mergeLiveProjectBootstrap } from "./lib/app-sync"
import type {
  OpenPanelsPanel,
  OpenPanelsPanelKind,
  OpenPanelsSession,
} from "./protocol"
import type {
  AppState,
  BootstrapResponse,
  OpenPanelsTransport,
  OpenPanelsUpdateStatus,
} from "./types"

type UpdateAction =
  | "checking"
  | "downloading"
  | "installing"
  | "restarting"
  | "failed"
  | null

export function App({ transport }: { transport: OpenPanelsTransport }) {
  const { t } = useOpenPanelsI18n()
  const [appState, setAppState] = useState<AppState | null>(null)
  const [bootstrapError, setBootstrapError] = useState<string | null>(null)
  const [canvasSnapshot, setCanvasSnapshot] = useState<StoreSnapshot | null>(
    null
  )
  const [selection, setSelection] = useState<CanvasSelectionSnapshot | null>(
    null
  )
  const [sessions, setSessions] = useState<OpenPanelsSession[]>([])
  const [snapshotLoadVersion, setSnapshotLoadVersion] = useState(0)
  const [updateStatus, setUpdateStatus] =
    useState<OpenPanelsUpdateStatus | null>(null)
  const [updateAction, setUpdateAction] = useState<UpdateAction>(null)
  const [updateError, setUpdateError] = useState<string | null>(null)
  const [isTraceOpen, setIsTraceOpen] = useState(false)
  const [agentPanelTab, setAgentPanelTab] = useState<AgentPanelTab>("tasks")
  const [agentTaskFilter, setAgentTaskFilter] = useState<TaskFilter>("pending")
  const appStateRef = useRef<AppState | null>(null)
  const canvasSnapshotRef = useRef<StoreSnapshot | null>(null)
  const canvasRevisionRef = useRef(0)
  const skipNextCanvasSaveRef = useRef(false)

  useEffect(() => {
    appStateRef.current = appState
  }, [appState])

  useEffect(() => {
    canvasSnapshotRef.current = canvasSnapshot
  }, [canvasSnapshot])

  useEffect(() => {
    if (transport.kind !== "http") return
    let cancelled = false
    const markFocused = () => {
      if (cancelled) return
      apiFetch(transport.apiBase, "/api/studio/focus", {
        method: "POST",
      }).catch((error) => {
        console.error("Failed to update OpenPanels Studio focus", error)
      })
    }
    markFocused()
    window.addEventListener("focus", markFocused)
    document.addEventListener("visibilitychange", markFocused)
    return () => {
      cancelled = true
      window.removeEventListener("focus", markFocused)
      document.removeEventListener("visibilitychange", markFocused)
    }
  }, [transport])

  const loadProject = useCallback(
    async (sessionId?: string | null) => {
      setBootstrapError(null)
      const data = await loadBootstrap(transport, sessionId)
      const normalized = normalizeBootstrap(data)
      window.localStorage.setItem(
        ACTIVE_SESSION_STORAGE_KEY,
        normalized.session.id
      )
      const nextCanvasSnapshot = canvasSnapshotFromState(normalized)
      appStateRef.current = normalized
      canvasSnapshotRef.current = nextCanvasSnapshot
      canvasRevisionRef.current = canvasRevisionFromState(normalized)
      skipNextCanvasSaveRef.current = true
      setSelection(null)
      setAppState(normalized)
      setCanvasSnapshot(nextCanvasSnapshot)
      setSnapshotLoadVersion((version) => version + 1)
      setSessions(data.sessions ?? (await fetchSessions(transport)))
    },
    [transport]
  )

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const sessionId =
        transport.kind === "http" ? await fetchActiveSessionId(transport) : null
      if (cancelled) return
      await loadProject(sessionId)
    })().catch((error) => {
      console.error("Failed to bootstrap OpenPanels", error)
      setBootstrapError(String(error?.message || error))
    })
    return () => {
      cancelled = true
    }
  }, [loadProject, transport])

  const activeAppSessionId = appState?.session.id

  useEffect(() => {
    if (!(activeAppSessionId && transport.kind === "http")) return
    let syncing = false
    const timer = window.setInterval(async () => {
      if (syncing) return
      syncing = true
      try {
        const activeSessionId = await fetchActiveSessionId(transport)
        if (activeSessionId && activeSessionId !== activeAppSessionId) {
          await loadProject(activeSessionId)
        }
      } catch (error) {
        console.error("Failed to sync OpenPanels active project", error)
      } finally {
        syncing = false
      }
    }, 5000)
    return () => window.clearInterval(timer)
  }, [activeAppSessionId, loadProject, transport])

  useEffect(() => {
    if (transport.kind !== "http") return
    let cancelled = false
    let syncing = false
    let pending = false

    const syncProject = async () => {
      if (syncing) {
        pending = true
        return
      }
      syncing = true
      try {
        while (!cancelled) {
          pending = false
          const current = appStateRef.current
          if (!current) return
          const data = await loadBootstrap(transport, current.session.id)
          if (cancelled) return

          const latest = appStateRef.current ?? current
          const merged = mergeLiveProjectBootstrap({
            current: latest,
            currentCanvasRevision: canvasRevisionRef.current,
            currentCanvasSnapshot: canvasSnapshotRef.current,
            remote: data,
          })
          canvasRevisionRef.current = merged.canvasRevision
          if (data.sessions) {
            setSessions(data.sessions)
          }
          if (merged.changed) {
            appStateRef.current = merged.appState
            canvasSnapshotRef.current = merged.canvasSnapshot
            skipNextCanvasSaveRef.current = merged.shouldReloadCanvas
            if (merged.shouldReloadCanvas) {
              setSelection(null)
            }
            setAppState(merged.appState)
            setCanvasSnapshot(merged.canvasSnapshot)
            if (merged.shouldReloadCanvas) {
              setSnapshotLoadVersion((version) => version + 1)
            }
          }
          if (!pending) return
        }
      } catch (error) {
        console.error("Failed to sync OpenPanels project changes", error)
      } finally {
        syncing = false
      }
    }

    const source = new EventSource(
      apiUrl(transport.apiBase, "/api/events").toString()
    )
    source.addEventListener("project", () => {
      syncProject()
    })
    return () => {
      cancelled = true
      source.close()
    }
  }, [transport])

  const refreshUpdateStatus = useCallback(
    async (options?: { refresh?: boolean }) => {
      setUpdateAction("checking")
      setUpdateError(null)
      try {
        const status = await fetchUpdateStatus(transport, options)
        setUpdateStatus(status)
      } catch (error) {
        if (!isNotFoundError(error)) {
          console.error("Failed to check OpenPanels update status", error)
        }
      } finally {
        setUpdateAction((current) => (current === "checking" ? null : current))
      }
    },
    [transport]
  )

  useEffect(() => {
    refreshUpdateStatus()
  }, [refreshUpdateStatus])

  const waitForStudioRestart = useCallback(
    async (expectedVersion?: string | null) => {
      const started = Date.now()
      const timeoutMs = 30_000
      while (Date.now() - started < timeoutMs) {
        try {
          const health = await fetchStudioHealth(transport, { timeoutMs: 900 })
          if (
            health.ok &&
            (!expectedVersion || health.version === expectedVersion)
          ) {
            return true
          }
        } catch {
          // The server is expected to disappear briefly while the new binary starts.
        }
        await new Promise((resolve) => window.setTimeout(resolve, 500))
      }
      return false
    },
    [transport]
  )

  const downloadUpdate = useCallback(async () => {
    setUpdateAction("downloading")
    setUpdateError(null)
    try {
      const status = await requestUpdateDownload(transport)
      setUpdateAction((current) => (current === "downloading" ? null : current))
      return status
    } catch (error) {
      console.error("Failed to download OpenPanels update", error)
      setUpdateError(
        "更新下载失败。请稍后重试，或让 agent 重新打开 MyOpenPanels 面板。"
      )
      setUpdateAction("failed")
      return null
    }
  }, [transport])

  const installAndRestartUpdate = useCallback(async () => {
    setUpdateAction("installing")
    setUpdateError(null)
    try {
      const result = await requestUpdateInstallRestart(transport)
      const expectedVersion =
        result.update.latestVersion ?? updateStatus?.latestVersion ?? null
      if (!result.restarting) {
        setUpdateAction(null)
        await refreshUpdateStatus({ refresh: true })
        return
      }
      setUpdateAction("restarting")
      const restored = await waitForStudioRestart(expectedVersion)
      if (restored) {
        window.location.reload()
        return
      }
      setUpdateError(
        "更新可能已安装，但 Studio 没有自动恢复。请让 agent 重新打开 MyOpenPanels 面板。"
      )
      setUpdateAction("failed")
    } catch (error) {
      console.error("Failed to install OpenPanels update", error)
      setUpdateError(
        error instanceof Error && error.message
          ? error.message
          : "更新安装失败。请稍后重试，或让 agent 重新打开 MyOpenPanels 面板。"
      )
      setUpdateAction("failed")
    }
  }, [refreshUpdateStatus, transport, updateStatus, waitForStudioRestart])

  const retryUpdateReconnect = useCallback(async () => {
    setUpdateAction("restarting")
    setUpdateError(null)
    const restored = await waitForStudioRestart(
      updateStatus?.latestVersion ?? null
    )
    if (restored) {
      window.location.reload()
      return
    }
    setUpdateError(
      "仍然无法连接到新版 Studio。请让 agent 重新打开 MyOpenPanels 面板。"
    )
    setUpdateAction("failed")
  }, [updateStatus, waitForStudioRestart])

  const dismissUpdateError = useCallback(() => {
    setUpdateAction(null)
    setUpdateError(null)
  }, [])

  const updateNow = useCallback(async () => {
    if (!(updateStatus?.updateAvailable || updateStatus?.readyToInstall)) {
      return
    }
    if (updateAction && updateAction !== "failed") return
    const downloaded = Boolean(
      updateStatus.downloaded || updateStatus.readyToInstall
    )
    if (downloaded) {
      installAndRestartUpdate()
      return
    }
    const status = await downloadUpdate()
    if (!(status?.downloaded || status?.readyToInstall)) return
    setUpdateStatus(status)
    installAndRestartUpdate()
  }, [downloadUpdate, installAndRestartUpdate, updateAction, updateStatus])

  const checkUpdateFromBadge = useCallback(
    (options?: { refresh?: boolean }) => {
      if (!updateAction) {
        refreshUpdateStatus(options)
      }
    },
    [refreshUpdateStatus, updateAction]
  )

  const refreshUpdateNow = useCallback(() => {
    refreshUpdateStatus({ refresh: true })
  }, [refreshUpdateStatus])

  const canvasPanel = useMemo(
    () =>
      appState?.panels.find(({ panel }) => panel.kind === "canvas")?.panel ??
      null,
    [appState]
  )
  const activeSessionId = appState?.session.id ?? null
  const canvasPanelId = canvasPanel?.id ?? null

  const assetStore = useMemo(() => {
    if (!(canvasPanelId && activeSessionId)) return new DataUrlAssetStore()
    return new OpenPanelsBrowserAssetStore(
      transport.apiBase,
      activeSessionId,
      canvasPanelId
    )
  }, [activeSessionId, canvasPanelId, transport.apiBase])

  const saveSnapshot = useCallback((nextSnapshot: StoreSnapshot) => {
    canvasSnapshotRef.current = nextSnapshot
    setCanvasSnapshot(nextSnapshot)
    setAppState((current) => {
      const next = current
        ? {
            ...current,
            panels: current.panels.map((snapshot) =>
              snapshot.panel.kind === "canvas"
                ? { ...snapshot, state: nextSnapshot }
                : snapshot
            ),
            state:
              current.panel.kind === "canvas" ? nextSnapshot : current.state,
          }
        : current
      appStateRef.current = next
      return next
    })
  }, [])

  const createProject = useCallback(async () => {
    const response = await apiFetch(transport.apiBase, "/api/projects", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({}),
    })
    const data = (await response.json()) as BootstrapResponse
    window.localStorage.setItem(ACTIVE_SESSION_STORAGE_KEY, data.session.id)
    const normalized = normalizeBootstrap(data)
    const nextCanvasSnapshot = canvasSnapshotFromState(normalized)
    appStateRef.current = normalized
    canvasSnapshotRef.current = nextCanvasSnapshot
    canvasRevisionRef.current = canvasRevisionFromState(normalized)
    skipNextCanvasSaveRef.current = true
    setSelection(null)
    setAppState(normalized)
    setCanvasSnapshot(nextCanvasSnapshot)
    setSnapshotLoadVersion((version) => version + 1)
    setSessions(data.sessions ?? (await fetchSessions(transport)))
  }, [transport])

  const renameProject = useCallback(
    async (title: string) => {
      if (!appState) return
      const response = await apiFetch(
        transport.apiBase,
        `/api/sessions/${encodeURIComponent(appState.session.id)}`,
        {
          method: "PATCH",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ title }),
        }
      )
      const data = (await response.json()) as { session: OpenPanelsSession }
      setAppState((current) =>
        current && current.session.id === data.session.id
          ? { ...current, session: data.session }
          : current
      )
      setSessions((current) =>
        current.map((session) =>
          session.id === data.session.id ? data.session : session
        )
      )
    },
    [appState, transport]
  )

  const deleteProject = useCallback(
    async (sessionId: string) => {
      if (!appState || sessions.length <= 1) return
      const response = await apiFetch(
        transport.apiBase,
        `/api/sessions/${encodeURIComponent(sessionId)}`,
        { method: "DELETE" }
      )
      const data = (await response.json()) as {
        activeSessionId: string
        deletedSessionId: string
        sessions: OpenPanelsSession[]
      }
      if (sessionId === appState.session.id) {
        await loadProject(data.activeSessionId)
        return
      }
      setSessions(data.sessions)
    },
    [appState, loadProject, sessions.length, transport]
  )

  const switchPanel = useCallback(
    async (kind: OpenPanelsPanelKind) => {
      if (!(appState && kind !== appState.activePanelKind)) return
      const response = await apiFetch(transport.apiBase, "/api/active-panel", {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          sessionId: appState.session.id,
          kind,
        }),
      })
      const data = (await response.json()) as {
        activePanelId: string
        activePanelKind: OpenPanelsPanelKind
        panel: OpenPanelsPanel
        revision?: number
        state: unknown
      }
      const normalizedState = normalizePanelState(data.panel.kind, data.state)
      const nextAppState: AppState = {
        ...appState,
        activePanelId: data.activePanelId,
        activePanelKind: data.activePanelKind,
        panel: data.panel,
        panels: appState.panels.map((snapshot) =>
          snapshot.panel.id === data.panel.id
            ? {
                panel: data.panel,
                revision: data.revision ?? snapshot.revision,
                state: normalizedState,
              }
            : snapshot
        ),
        revision: data.revision ?? appState.revision,
        state: normalizedState,
      }
      appStateRef.current = nextAppState
      setSelection(null)
      setAppState((current) =>
        current && current.session.id === appState.session.id
          ? nextAppState
          : current
      )
      if (data.panel.kind === "canvas") {
        const nextCanvasSnapshot = normalizeSnapshot(
          normalizedState as StoreSnapshot
        )
        canvasSnapshotRef.current = nextCanvasSnapshot
        canvasRevisionRef.current = data.revision ?? canvasRevisionRef.current
        skipNextCanvasSaveRef.current = true
        setCanvasSnapshot(nextCanvasSnapshot)
      }
    },
    [appState, transport]
  )

  const reloadCurrentProject = useCallback(async () => {
    if (!appState?.session.id) return
    await loadProject(appState.session.id)
  }, [appState?.session.id, loadProject])

  useEffect(() => {
    if (!(appState && canvasPanel && canvasSnapshot)) return
    if (skipNextCanvasSaveRef.current) {
      skipNextCanvasSaveRef.current = false
      return
    }
    const timer = window.setTimeout(() => {
      savePanelState(
        transport,
        appState.session.id,
        canvasPanel.id,
        canvasSnapshot,
        canvasRevisionRef.current
      )
        .then((payload) => {
          canvasRevisionRef.current = payload.revision
        })
        .catch((error) => {
          if (error instanceof Error && error.message === "HTTP 409") {
            loadProject(appState.session.id).catch((reloadError) => {
              console.error(
                "Failed to reload stale OpenPanels canvas",
                reloadError
              )
            })
            return
          }
          console.error("Failed to save OpenPanels canvas state", error)
        })
    }, 400)
    return () => window.clearTimeout(timer)
  }, [appState, canvasPanel, canvasSnapshot, loadProject, transport])

  useEffect(() => {
    if (!(appState && canvasPanel && selection)) return
    const timer = window.setTimeout(() => {
      saveSelectionState(
        transport,
        appState.session.id,
        canvasPanel.id,
        selection
      ).catch((error) => {
        console.error("Failed to save OpenPanels selection", error)
      })
    }, 300)
    return () => window.clearTimeout(timer)
  }, [appState, canvasPanel, selection, transport])

  if (!appState) {
    return (
      <main className="design-shell design-shell--status">
        <div className="op-boot-status">
          <div>
            {bootstrapError ? t`Failed to load canvas` : t`Loading canvas`}
          </div>
          {bootstrapError ? (
            <div className="op-boot-status__detail">{bootstrapError}</div>
          ) : null}
        </div>
      </main>
    )
  }

  const projectChrome = (
    <ProjectChrome
      currentSession={appState.session}
      onCreateProject={createProject}
      onDeleteProject={deleteProject}
      onRenameProject={renameProject}
      onSwitchProject={loadProject}
      sessions={sessions}
    />
  )

  return (
    <main
      className={`design-shell ${isTraceOpen ? "design-shell--trace-open" : ""}`}
    >
      <section className="design-shell__workspace">
        {appState.activePanelKind === "canvas" && canvasSnapshot ? (
          <CanvasPanel
            assetStore={assetStore}
            height="100vh"
            key={`${appState.session.id}:${canvasPanel?.id ?? "canvas"}`}
            onSelectionChange={setSelection}
            onSnapshotChange={saveSnapshot}
            snapshot={canvasSnapshot}
            snapshotVersion={snapshotLoadVersion}
            titleChromeContent={projectChrome}
          />
        ) : (
          <WikiPanel
            chromeContent={projectChrome}
            onReload={reloadCurrentProject}
            state={wikiStateFromAppState(appState)}
            transport={transport}
          />
        )}
        <BottomPanelTabs
          activePanelKind={appState.activePanelKind}
          onSwitchPanel={switchPanel}
          panels={appState.panels.map(({ panel }) => panel)}
        />
        <div className="op-status-cluster">
          {appState.buildInfo ? (
            <BuildVersionBadge
              info={appState.buildInfo}
              isChecking={updateAction === "checking"}
              onCheckUpdate={checkUpdateFromBadge}
              onUpdate={updateNow}
              status={updateStatus}
            />
          ) : null}
          <AgentToggleButton
            isOpen={isTraceOpen}
            onToggle={() => {
              if (!isTraceOpen) {
                setAgentPanelTab("tasks")
                setAgentTaskFilter("pending")
              }
              setIsTraceOpen((value) => !value)
            }}
            pendingCount={appState.pendingTaskCount ?? 0}
          />
        </div>
        <UpdatePrompt
          action={updateAction}
          errorMessage={updateError}
          onDismissError={dismissUpdateError}
          onRefresh={refreshUpdateNow}
          onRetryConnect={retryUpdateReconnect}
          onUpdate={updateNow}
          status={updateStatus}
        />
      </section>
      <AgentPanel
        activeTab={agentPanelTab}
        buildInfo={appState.buildInfo}
        isOpen={isTraceOpen}
        onTabChange={setAgentPanelTab}
        onTaskFilterChange={setAgentTaskFilter}
        taskFilter={agentTaskFilter}
        tasks={appState.tasks ?? []}
        transport={transport}
        workerStatus={appState.agentWorker}
      />
    </main>
  )
}
