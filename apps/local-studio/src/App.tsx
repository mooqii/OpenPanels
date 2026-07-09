import { useCallback, useEffect, useMemo, useState } from "react"
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
  BuildVersionBadge,
  TracePanel,
  TraceToggleButton,
} from "./components/trace/TracePanel"
import { UpdatePrompt } from "./components/update/UpdatePrompt"
import { WikiPanel } from "./components/wiki/WikiPanel"
import { ACTIVE_SESSION_STORAGE_KEY } from "./constants"
import {
  apiFetch,
  canvasSnapshotFromState,
  fetchActiveSessionId,
  fetchSessions,
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
  const [updateAction, setUpdateAction] = useState<
    "checking" | "downloading" | "installing" | null
  >(null)
  const [isTraceOpen, setIsTraceOpen] = useState(false)

  const loadProject = useCallback(
    async (sessionId?: string | null) => {
      setBootstrapError(null)
      const data = await loadBootstrap(transport, sessionId)
      const normalized = normalizeBootstrap(data)
      window.localStorage.setItem(
        ACTIVE_SESSION_STORAGE_KEY,
        normalized.session.id
      )
      setSelection(null)
      setAppState(normalized)
      const nextCanvasSnapshot = canvasSnapshotFromState(normalized)
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

  const refreshUpdateStatus = useCallback(async () => {
    setUpdateAction("checking")
    try {
      const status = await fetchUpdateStatus(transport)
      setUpdateStatus(status)
    } catch (error) {
      if (!isNotFoundError(error)) {
        console.error("Failed to check OpenPanels update status", error)
      }
    } finally {
      setUpdateAction(null)
    }
  }, [transport])

  useEffect(() => {
    refreshUpdateStatus()
  }, [refreshUpdateStatus])

  const downloadUpdate = useCallback(async () => {
    setUpdateAction("downloading")
    try {
      return await requestUpdateDownload(transport)
    } catch (error) {
      console.error("Failed to download OpenPanels update", error)
      return null
    } finally {
      setUpdateAction(null)
    }
  }, [transport])

  const installUpdate = useCallback(async () => {
    setUpdateAction("installing")
    try {
      await requestUpdateInstallRestart(transport)
      window.setTimeout(() => window.location.reload(), 1400)
    } catch (error) {
      console.error("Failed to install OpenPanels update", error)
      setUpdateAction(null)
    }
  }, [transport])

  const updateNow = useCallback(async () => {
    if (!(updateStatus?.updateAvailable || updateStatus?.readyToInstall)) {
      return
    }
    if (updateAction) return
    const downloaded = Boolean(
      updateStatus.downloaded || updateStatus.readyToInstall
    )
    if (downloaded) {
      installUpdate()
      return
    }
    const status = await downloadUpdate()
    if (!(status?.downloaded || status?.readyToInstall)) return
    setUpdateStatus(status)
    setUpdateAction("installing")
    try {
      await requestUpdateInstallRestart(transport)
      window.setTimeout(() => window.location.reload(), 1400)
    } catch (error) {
      console.error("Failed to install OpenPanels update", error)
      setUpdateAction(null)
    }
  }, [downloadUpdate, installUpdate, transport, updateAction, updateStatus])

  const checkUpdateFromBadge = useCallback(() => {
    if (!updateAction) {
      refreshUpdateStatus()
    }
  }, [refreshUpdateStatus, updateAction])

  const canvasPanel = useMemo(
    () =>
      appState?.panels.find(({ panel }) => panel.kind === "canvas")?.panel ??
      null,
    [appState]
  )
  const activeSessionId = appState?.session.id ?? null

  const assetStore = useMemo(() => {
    if (!(canvasPanel && activeSessionId)) return new DataUrlAssetStore()
    return new OpenPanelsBrowserAssetStore(
      transport.apiBase,
      activeSessionId,
      canvasPanel.id
    )
  }, [canvasPanel, activeSessionId, transport.apiBase])

  const saveSnapshot = useCallback((nextSnapshot: StoreSnapshot) => {
    setCanvasSnapshot(nextSnapshot)
    setAppState((current) =>
      current
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
    )
  }, [])

  const createProject = useCallback(async () => {
    const response = await apiFetch(transport.apiBase, "/api/projects", {
      method: "POST",
    })
    const data = (await response.json()) as BootstrapResponse
    window.localStorage.setItem(ACTIVE_SESSION_STORAGE_KEY, data.session.id)
    const normalized = normalizeBootstrap(data)
    setSelection(null)
    setAppState(normalized)
    const nextCanvasSnapshot = canvasSnapshotFromState(normalized)
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
                state: normalizedState,
              }
            : snapshot
        ),
        state: normalizedState,
      }
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
    const timer = window.setTimeout(() => {
      savePanelState(
        transport,
        appState.session.id,
        canvasPanel.id,
        canvasSnapshot
      ).catch((error) => {
        console.error("Failed to save OpenPanels canvas state", error)
      })
    }, 400)
    return () => window.clearTimeout(timer)
  }, [appState, canvasPanel, canvasSnapshot, transport])

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
            />
          ) : null}
          <TraceToggleButton
            isOpen={isTraceOpen}
            onToggle={() => setIsTraceOpen((value) => !value)}
          />
        </div>
        <UpdatePrompt
          action={updateAction}
          onRefresh={refreshUpdateStatus}
          onUpdate={updateNow}
          status={updateStatus}
        />
      </section>
      <TracePanel
        buildInfo={appState.buildInfo}
        isOpen={isTraceOpen}
        transport={transport}
      />
    </main>
  )
}
