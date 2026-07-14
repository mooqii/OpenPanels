import { Button } from "@heroui/react"
import { ExternalLink } from "lucide-react"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  CanvasPanel,
  type CanvasSelectionSnapshot,
  DataUrlAssetStore,
  type StoreSnapshot,
  useMyOpenPanelsI18n,
} from "./canvas"
import {
  BottomPanelTabs,
  MyOpenPanelsBrowserAssetStore,
  ProjectChrome,
} from "./components/project/ProjectChrome"
import { PublishingPanel } from "./components/publishing/PublishingPanel"
import {
  AgentPanel,
  type AgentPanelTab,
  AgentToggleButton,
  BuildVersionBadge,
  type TaskFilter,
} from "./components/trace/TracePanel"
import { TypesettingPanel } from "./components/typesetting/TypesettingPanel"
import {
  type StudioRuntimeState,
  StudioRuntimeStatus,
} from "./components/update/StudioRuntimeStatus"
import { UpdatePrompt } from "./components/update/UpdatePrompt"
import { WikiPanel } from "./components/wiki/WikiPanel"
import { ACTIVE_PROJECT_STORAGE_KEY } from "./constants"
import {
  apiFetch,
  apiUrl,
  canvasRevisionFromState,
  canvasSnapshotFromState,
  completeSelectionMaterialization,
  fetchActiveProjectId,
  fetchProjects,
  fetchSelectionMaterializationRequest,
  fetchSelectionState,
  fetchStudioHealth,
  fetchUpdateStatus,
  isNotFoundError,
  loadBootstrap,
  normalizeBootstrap,
  normalizePanelState,
  normalizeSnapshot,
  requestUpdateDownload,
  requestUpdateInstallRestart,
  saveCanvasPanelState,
  saveSelectionState,
  typesettingRevisionFromAppState,
  typesettingStateFromAppState,
  wikiStateFromAppState,
  writingStateFromAppState,
} from "./lib/api"
import { mergeLiveProjectBootstrap, sameSelectedShapeIds } from "./lib/app-sync"
import {
  externalBrowserPath,
  shouldShowOpenInBrowserPrompt,
} from "./lib/browser-context"
import {
  flushBeforeRuntimeReload,
  RUNTIME_RECONNECT_NOTICE_MS,
  RUNTIME_RELOAD_MARKER,
  runtimeConnectionDecision,
  runtimePollDelay,
  runtimeVersionDecision,
} from "./lib/studio-runtime"
import type {
  MyOpenPanelsPanel,
  MyOpenPanelsPanelKind,
  MyOpenPanelsProject,
} from "./protocol"
import type {
  AgentOperation,
  AppState,
  BootstrapResponse,
  MyOpenPanelsTransport,
  MyOpenPanelsUpdateStatus,
  TypesettingState,
} from "./types"

type UpdateAction =
  | "checking"
  | "downloading"
  | "installing"
  | "restarting"
  | "failed"
  | null

export function App({ transport }: { transport: MyOpenPanelsTransport }) {
  const { t } = useMyOpenPanelsI18n()
  const [appState, setAppState] = useState<AppState | null>(null)
  const [bootstrapError, setBootstrapError] = useState<string | null>(null)
  const [canvasSnapshot, setCanvasSnapshot] = useState<StoreSnapshot | null>(
    null
  )
  const [selection, setSelection] = useState<CanvasSelectionSnapshot | null>(
    null
  )
  const [projects, setProjects] = useState<MyOpenPanelsProject[]>([])
  const [snapshotLoadVersion, setSnapshotLoadVersion] = useState(0)
  const [wikiSelectionVersion, setWikiSelectionVersion] = useState(0)
  const [updateStatus, setUpdateStatus] =
    useState<MyOpenPanelsUpdateStatus | null>(null)
  const [updateAction, setUpdateAction] = useState<UpdateAction>(null)
  const [updateError, setUpdateError] = useState<string | null>(null)
  const [loadedRuntimeVersion, setLoadedRuntimeVersion] = useState<
    string | null
  >(null)
  const [runtimeState, setRuntimeState] =
    useState<StudioRuntimeState>("connected")
  const [isTraceOpen, setIsTraceOpen] = useState(false)
  const [agentPanelTab, setAgentPanelTab] = useState<AgentPanelTab>("tasks")
  const [agentTaskFilter, setAgentTaskFilter] = useState<TaskFilter>("pending")
  const [operationNotice, setOperationNotice] = useState<AgentOperation | null>(
    null
  )
  const appStateRef = useRef<AppState | null>(null)
  const canvasSnapshotRef = useRef<StoreSnapshot | null>(null)
  const canvasRevisionRef = useRef(0)
  const canvasSaveGenerationRef = useRef(0)
  const canvasDirtyRef = useRef(false)
  const selectionRef = useRef<CanvasSelectionSnapshot | null>(null)
  const selectionMaterializerRef = useRef<(() => string | null) | null>(null)
  const materializationInFlightRef = useRef<string | null>(null)
  const runtimeContextIdRef = useRef<string | null>(null)
  const skipNextCanvasSaveRef = useRef(false)
  const operationStatusesRef = useRef<Map<string, string> | null>(null)
  const showOpenInBrowserPrompt = shouldShowOpenInBrowserPrompt(
    window.navigator.userAgent
  )

  const openAgentTaskList = useCallback((filter: TaskFilter) => {
    setAgentPanelTab("tasks")
    setAgentTaskFilter(filter)
    setIsTraceOpen(true)
  }, [])

  const openInDefaultBrowser = useCallback(async () => {
    await apiFetch(transport.apiBase, "/api/studio/open-browser", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ path: externalBrowserPath(window.location) }),
    })
  }, [transport.apiBase])

  useEffect(() => {
    appStateRef.current = appState
  }, [appState])

  useEffect(() => {
    selectionRef.current = selection
  }, [selection])

  const updateLocalSelection = useCallback((next: CanvasSelectionSnapshot) => {
    const currentIds = selectionRef.current?.selectedShapeIds ?? []
    if (sameSelectedShapeIds(currentIds, next.selectedShapeIds)) return
    selectionRef.current = next
    setSelection(next)
  }, [])

  useEffect(() => {
    const version = appState?.buildInfo?.version
    if (version && !loadedRuntimeVersion) {
      setLoadedRuntimeVersion(version)
    }
  }, [appState?.buildInfo?.version, loadedRuntimeVersion])

  useEffect(() => {
    if (!appState) return
    const next = new Map(
      (appState.agentOperations ?? []).map((operation) => [
        operation.id,
        operation.status,
      ])
    )
    const previous = operationStatusesRef.current
    operationStatusesRef.current = next
    if (!previous) return
    const completed = (appState.agentOperations ?? []).find(
      (operation) =>
        previous.get(operation.id) === "active" &&
        (operation.status === "completed" || operation.status === "failed")
    )
    if (!completed) return
    setOperationNotice(completed)
    const timer = window.setTimeout(() => setOperationNotice(null), 6000)
    return () => window.clearTimeout(timer)
  }, [appState])

  useEffect(() => {
    canvasSnapshotRef.current = canvasSnapshot
  }, [canvasSnapshot])

  const loadProject = useCallback(
    async (projectId?: string | null) => {
      setBootstrapError(null)
      const data = await loadBootstrap(transport, projectId)
      const normalized = normalizeBootstrap(data)
      window.localStorage.setItem(
        ACTIVE_PROJECT_STORAGE_KEY,
        normalized.project.id
      )
      const nextCanvasSnapshot = canvasSnapshotFromState(normalized)
      appStateRef.current = normalized
      canvasSnapshotRef.current = nextCanvasSnapshot
      canvasRevisionRef.current = canvasRevisionFromState(normalized)
      canvasDirtyRef.current = false
      skipNextCanvasSaveRef.current = true
      setSelection(null)
      setAppState(normalized)
      setCanvasSnapshot(nextCanvasSnapshot)
      setSnapshotLoadVersion((version) => version + 1)
      setProjects(data.projects ?? (await fetchProjects(transport)))
    },
    [transport]
  )

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const projectId =
        transport.kind === "http" ? await fetchActiveProjectId(transport) : null
      if (cancelled) return
      await loadProject(projectId)
    })().catch((error) => {
      console.error("Failed to bootstrap MyOpenPanels", error)
      setBootstrapError(String(error?.message || error))
    })
    return () => {
      cancelled = true
    }
  }, [loadProject, transport])

  const activeAppProjectId = appState?.project.id

  useEffect(() => {
    if (!(activeAppProjectId && transport.kind === "http")) return
    let syncing = false
    const timer = window.setInterval(async () => {
      if (syncing) return
      syncing = true
      try {
        const activeProjectId = await fetchActiveProjectId(transport)
        if (activeProjectId && activeProjectId !== activeAppProjectId) {
          await loadProject(activeProjectId)
        }
      } catch (error) {
        console.error("Failed to sync MyOpenPanels active project", error)
      } finally {
        syncing = false
      }
    }, 5000)
    return () => window.clearInterval(timer)
  }, [activeAppProjectId, loadProject, transport])

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
          const data = await loadBootstrap(transport, current.project.id)
          if (cancelled) return

          const latest = appStateRef.current ?? current
          const merged = mergeLiveProjectBootstrap({
            current: latest,
            currentCanvasRevision: canvasRevisionRef.current,
            currentCanvasSnapshot: canvasSnapshotRef.current,
            remote: data,
          })
          canvasRevisionRef.current = merged.canvasRevision
          if (data.projects) {
            setProjects(data.projects)
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
        console.error("Failed to sync MyOpenPanels project changes", error)
      } finally {
        syncing = false
      }
    }

    const syncFocus = async () => {
      try {
        const activeProjectId = await fetchActiveProjectId(transport)
        const currentProjectId = appStateRef.current?.project.id
        if (activeProjectId && activeProjectId !== currentProjectId) {
          await loadProject(activeProjectId)
          return
        }
        await syncProject()
      } catch (error) {
        console.error("Failed to sync MyOpenPanels focus", error)
      }
    }

    const syncSelection = async (change: {
      panelId?: string | null
      projectId?: string | null
    }) => {
      const current = appStateRef.current
      if (!current) return
      const canvas = current.panels.find(
        ({ panel }) => panel.kind === "canvas"
      )?.panel
      const changedPanel = current.panels.find(
        ({ panel }) => panel.id === change.panelId
      )?.panel
      if (changedPanel?.kind === "wiki" || changedPanel?.kind === "writing") {
        setWikiSelectionVersion((version) => version + 1)
        return
      }
      if (
        !canvas ||
        (change.projectId && change.projectId !== current.project.id) ||
        (change.panelId && change.panelId !== canvas.id)
      )
        return
      try {
        const remote = await fetchSelectionState(
          transport,
          current.project.id,
          canvas.id
        )
        const currentIds = selectionRef.current?.selectedShapeIds ?? []
        const nextIds = remote.selection.selectedShapeIds
        if (sameSelectedShapeIds(currentIds, nextIds)) return
        selectionRef.current = remote.selection
        setSelection(remote.selection)
      } catch (error) {
        console.error("Failed to sync MyOpenPanels selection", error)
      }
    }

    const eventsUrl = apiUrl(transport.apiBase, "/api/events")
    if (activeAppProjectId)
      eventsUrl.searchParams.set("projectId", activeAppProjectId)
    const source = new EventSource(eventsUrl.toString())
    source.addEventListener("project", (event) => {
      const change = JSON.parse((event as MessageEvent<string>).data) as {
        kind?: string
        panelId?: string | null
        projectId?: string | null
      }
      if (change.kind === "panel_selection") {
        syncSelection(change)
        return
      }
      if (change.kind === "focus") {
        syncFocus()
        return
      }
      syncProject()
    })
    source.addEventListener("open", () => {
      window.dispatchEvent(new Event("myopenpanels:runtime-check"))
    })
    const targetStatusTimer = window.setInterval(syncProject, 15_000)
    return () => {
      cancelled = true
      window.clearInterval(targetStatusTimer)
      source.close()
    }
  }, [activeAppProjectId, loadProject, transport])

  const refreshUpdateStatus = useCallback(
    async (options?: { refresh?: boolean }) => {
      setUpdateAction("checking")
      setUpdateError(null)
      try {
        const status = await fetchUpdateStatus(transport, options)
        setUpdateStatus(status)
      } catch (error) {
        if (!isNotFoundError(error)) {
          console.error("Failed to check MyOpenPanels update status", error)
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

  const downloadUpdate = useCallback(async () => {
    setUpdateAction("downloading")
    setUpdateError(null)
    try {
      const status = await requestUpdateDownload(transport)
      setUpdateAction((current) => (current === "downloading" ? null : current))
      return status
    } catch (error) {
      console.error("Failed to download MyOpenPanels update", error)
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
      if (!result.restarting) {
        setUpdateAction(null)
        await refreshUpdateStatus({ refresh: true })
        return
      }
      setUpdateAction("restarting")
      window.dispatchEvent(new Event("myopenpanels:runtime-check"))
    } catch (error) {
      console.error("Failed to install MyOpenPanels update", error)
      setUpdateError(
        error instanceof Error && error.message
          ? error.message
          : "更新安装失败。请稍后重试，或让 agent 重新打开 MyOpenPanels 面板。"
      )
      setUpdateAction("failed")
    }
  }, [refreshUpdateStatus, transport])

  const retryUpdateReconnect = useCallback(() => {
    setUpdateAction("restarting")
    setUpdateError(null)
    setRuntimeState("reconnecting")
    window.dispatchEvent(new Event("myopenpanels:runtime-check"))
  }, [])

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
  const activeProjectId = appState?.project.id ?? null
  const canvasPanelId = canvasPanel?.id ?? null

  const assetStore = useMemo(() => {
    if (!(canvasPanelId && activeProjectId)) return new DataUrlAssetStore()
    return new MyOpenPanelsBrowserAssetStore(
      transport.apiBase,
      activeProjectId,
      canvasPanelId
    )
  }, [activeProjectId, canvasPanelId, transport.apiBase])

  const saveSnapshot = useCallback((nextSnapshot: StoreSnapshot) => {
    canvasSaveGenerationRef.current += 1
    canvasDirtyRef.current = true
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
    window.localStorage.setItem(ACTIVE_PROJECT_STORAGE_KEY, data.project.id)
    const normalized = normalizeBootstrap(data)
    const nextCanvasSnapshot = canvasSnapshotFromState(normalized)
    appStateRef.current = normalized
    canvasSnapshotRef.current = nextCanvasSnapshot
    canvasRevisionRef.current = canvasRevisionFromState(normalized)
    canvasDirtyRef.current = false
    skipNextCanvasSaveRef.current = true
    setSelection(null)
    setAppState(normalized)
    setCanvasSnapshot(nextCanvasSnapshot)
    setSnapshotLoadVersion((version) => version + 1)
    setProjects(data.projects ?? (await fetchProjects(transport)))
  }, [transport])

  const renameProject = useCallback(
    async (title: string) => {
      if (!appState) return
      const response = await apiFetch(
        transport.apiBase,
        `/api/projects/${encodeURIComponent(appState.project.id)}`,
        {
          method: "PATCH",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ title }),
        }
      )
      const data = (await response.json()) as { project: MyOpenPanelsProject }
      setAppState((current) =>
        current && current.project.id === data.project.id
          ? { ...current, project: data.project }
          : current
      )
      setProjects((current) =>
        current.map((project) =>
          project.id === data.project.id ? data.project : project
        )
      )
    },
    [appState, transport]
  )

  const deleteProject = useCallback(
    async (projectId: string) => {
      if (!appState || projects.length <= 1) return
      const response = await apiFetch(
        transport.apiBase,
        `/api/projects/${encodeURIComponent(projectId)}`,
        { method: "DELETE" }
      )
      const data = (await response.json()) as {
        activeProjectId: string
        deletedProjectId: string
        projects: MyOpenPanelsProject[]
      }
      if (projectId === appState.project.id) {
        await loadProject(data.activeProjectId)
        return
      }
      setProjects(data.projects)
    },
    [appState, loadProject, projects.length, transport]
  )

  const switchPanel = useCallback(
    async (kind: MyOpenPanelsPanelKind) => {
      if (!(appState && kind !== appState.activePanelKind)) return
      const response = await apiFetch(transport.apiBase, "/api/active-panel", {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          projectId: appState.project.id,
          kind,
        }),
      })
      const data = (await response.json()) as {
        activePanelId: string
        activePanelKind: MyOpenPanelsPanelKind
        panel: MyOpenPanelsPanel
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
        current && current.project.id === appState.project.id
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
    if (!appState?.project.id) return
    await loadProject(appState.project.id)
  }, [appState?.project.id, loadProject])

  const flushCanvasSave = useCallback(async () => {
    if (!canvasDirtyRef.current) return
    const current = appStateRef.current
    const snapshot = canvasSnapshotRef.current
    const panel = current?.panels.find(
      ({ panel }) => panel.kind === "canvas"
    )?.panel
    if (!(current && snapshot && panel)) return

    const generation = canvasSaveGenerationRef.current
    const payload = await saveCanvasPanelState(
      transport,
      current.project.id,
      panel.id,
      snapshot,
      canvasRevisionRef.current
    )
    canvasRevisionRef.current = payload.revision
    if (canvasSaveGenerationRef.current === generation) {
      canvasDirtyRef.current = false
    }
  }, [transport])

  const handleTypesettingStateSaved = useCallback(
    (savedState: TypesettingState, savedRevision: number) => {
      setAppState((current) => {
        if (!current) return current
        const next = {
          ...current,
          panels: current.panels.map((snapshot) =>
            snapshot.panel.kind === "typesetting"
              ? {
                  ...snapshot,
                  revision: savedRevision,
                  state: savedState,
                }
              : snapshot
          ),
          revision:
            current.activePanelKind === "typesetting"
              ? savedRevision
              : current.revision,
          state:
            current.activePanelKind === "typesetting"
              ? savedState
              : current.state,
        }
        appStateRef.current = next
        return next
      })
    },
    []
  )

  useEffect(() => {
    if (!(appState && canvasPanel && canvasSnapshot)) return
    if (skipNextCanvasSaveRef.current) {
      skipNextCanvasSaveRef.current = false
      canvasDirtyRef.current = false
      return
    }
    const timer = window.setTimeout(() => {
      flushCanvasSave().catch((error) => {
        if (error instanceof Error && error.message === "HTTP 409") {
          loadProject(appState.project.id).catch((reloadError) => {
            console.error(
              "Failed to reload stale MyOpenPanels canvas",
              reloadError
            )
          })
          return
        }
        console.error("Failed to save MyOpenPanels canvas state", error)
      })
    }, 400)
    return () => window.clearTimeout(timer)
  }, [appState, canvasPanel, canvasSnapshot, flushCanvasSave, loadProject])

  useEffect(() => {
    if (!(appState && canvasPanel && selection)) return
    const timer = window.setTimeout(() => {
      saveSelectionState(
        transport,
        appState.project.id,
        canvasPanel.id,
        selection
      ).catch((error) => {
        console.error("Failed to save MyOpenPanels selection", error)
      })
    }, 300)
    return () => window.clearTimeout(timer)
  }, [appState, canvasPanel, selection, transport])

  useEffect(() => {
    if (!(appState && canvasPanel && appState.activePanelKind === "canvas"))
      return
    let cancelled = false
    const poll = async () => {
      if (cancelled || materializationInFlightRef.current) return
      try {
        const request = await fetchSelectionMaterializationRequest(
          transport,
          appState.project.id,
          canvasPanel.id
        )
        if (!request) return
        const current = selectionRef.current
        const materialize = selectionMaterializerRef.current
        if (!(current && materialize)) return
        if (
          request.selectedShapeIds.length !== current.selectedShapeIds.length ||
          request.selectedShapeIds.some(
            (shapeId) => !current.selectedShapeIds.includes(shapeId)
          )
        )
          return
        materializationInFlightRef.current = request.requestId
        const imageDataUrl = materialize()
        if (!imageDataUrl) return
        await completeSelectionMaterialization(
          transport,
          appState.project.id,
          canvasPanel.id,
          request.requestId,
          imageDataUrl
        )
      } catch (error) {
        console.error("Failed to materialize MyOpenPanels selection", error)
      } finally {
        materializationInFlightRef.current = null
      }
    }
    const timer = window.setInterval(poll, 250)
    poll()
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [appState, canvasPanel, transport])

  useEffect(() => {
    if (!loadedRuntimeVersion) return
    let cancelled = false
    let checking = false
    let disconnectedAt: number | null = null
    let noticeTimer: number | null = null
    let pollTimer: number | null = null
    let reloadRequested = false

    const clearNoticeTimer = () => {
      if (noticeTimer !== null) {
        window.clearTimeout(noticeTimer)
        noticeTimer = null
      }
    }
    const schedule = () => {
      if (cancelled || reloadRequested) return
      pollTimer = window.setTimeout(
        checkRuntime,
        runtimePollDelay(document.hidden)
      )
    }
    const checkRuntime = async () => {
      if (cancelled || checking || reloadRequested) return
      checking = true
      try {
        const health = await fetchStudioHealth(transport, { timeoutMs: 900 })
        if (!health.ok) throw new Error("Studio is not healthy")
        if (
          runtimeContextIdRef.current &&
          health.contextId !== runtimeContextIdRef.current
        ) {
          setRuntimeState("failed")
          return
        }
        runtimeContextIdRef.current ??= health.contextId

        disconnectedAt = null
        clearNoticeTimer()
        let attemptedVersion: string | null = null
        try {
          attemptedVersion = window.sessionStorage.getItem(
            RUNTIME_RELOAD_MARKER
          )
        } catch {
          // Storage can be unavailable in constrained embedded browsers.
        }
        const decision = runtimeVersionDecision({
          attemptedVersion,
          loadedVersion: loadedRuntimeVersion,
          serverVersion: health.version,
        })
        if (decision === "current") {
          setRuntimeState("connected")
          if (attemptedVersion === loadedRuntimeVersion) {
            try {
              window.sessionStorage.removeItem(RUNTIME_RELOAD_MARKER)
            } catch {
              // Storage can be unavailable in constrained embedded browsers.
            }
          }
          return
        }
        if (decision === "stale") {
          setRuntimeState("failed")
          setUpdateError(
            "页面仍在使用旧版资源。请确认 Studio 已升级后重新连接。"
          )
          setUpdateAction("failed")
          return
        }

        setRuntimeState("switching")
        await flushBeforeRuntimeReload({
          flush: flushCanvasSave,
          isDirty: () => canvasDirtyRef.current,
        })
        try {
          window.sessionStorage.setItem(RUNTIME_RELOAD_MARKER, health.version)
        } catch {
          // Reload still works without loop protection when storage is unavailable.
        }
        reloadRequested = true
        window.location.reload()
      } catch {
        const now = Date.now()
        if (disconnectedAt === null) {
          disconnectedAt = now
          clearNoticeTimer()
          noticeTimer = window.setTimeout(() => {
            if (!cancelled && disconnectedAt !== null) {
              setRuntimeState("reconnecting")
            }
          }, RUNTIME_RECONNECT_NOTICE_MS)
        }
        if (runtimeConnectionDecision(disconnectedAt, now) === "failed") {
          clearNoticeTimer()
          setRuntimeState("failed")
          if (updateAction === "restarting") {
            setUpdateError("新版 Studio 没有在预期时间内恢复。")
            setUpdateAction("failed")
          }
        }
      } finally {
        checking = false
        schedule()
      }
    }
    const requestCheck = () => {
      if (pollTimer !== null) window.clearTimeout(pollTimer)
      pollTimer = null
      checkRuntime()
    }

    window.addEventListener("focus", requestCheck)
    window.addEventListener("online", requestCheck)
    window.addEventListener("myopenpanels:runtime-check", requestCheck)
    document.addEventListener("visibilitychange", requestCheck)
    checkRuntime()
    return () => {
      cancelled = true
      clearNoticeTimer()
      if (pollTimer !== null) window.clearTimeout(pollTimer)
      window.removeEventListener("focus", requestCheck)
      window.removeEventListener("online", requestCheck)
      window.removeEventListener("myopenpanels:runtime-check", requestCheck)
      document.removeEventListener("visibilitychange", requestCheck)
    }
  }, [flushCanvasSave, loadedRuntimeVersion, transport, updateAction])

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
      currentProject={appState.project}
      onCreateProject={createProject}
      onDeleteProject={deleteProject}
      onRenameProject={renameProject}
      onSwitchProject={loadProject}
      projects={projects}
    />
  )

  return (
    <main
      className={`design-shell ${isTraceOpen ? "design-shell--trace-open" : ""}`}
    >
      <section className="design-shell__workspace">
        {showOpenInBrowserPrompt ? (
          <Button
            className="op-open-browser-prompt"
            onPress={() => {
              openInDefaultBrowser().catch((error) => {
                console.error(
                  "Failed to open MyOpenPanels in the default browser",
                  error
                )
              })
            }}
            size="sm"
            variant="secondary"
          >
            <ExternalLink size={14} strokeWidth={1.8} />
            <span>{t`Open in browser`}</span>
          </Button>
        ) : null}
        {appState.activePanelKind === "canvas" && canvasSnapshot ? (
          <CanvasPanel
            assetStore={assetStore}
            height="100vh"
            key={`${appState.project.id}:${canvasPanel?.id ?? "canvas"}`}
            onSelectionChange={updateLocalSelection}
            onSelectionMaterializerChange={(materialize) => {
              selectionMaterializerRef.current = materialize
            }}
            onSnapshotChange={saveSnapshot}
            selectedShapeIds={selection?.selectedShapeIds ?? []}
            snapshot={canvasSnapshot}
            snapshotVersion={snapshotLoadVersion}
            titleChromeContent={projectChrome}
          />
        ) : appState.activePanelKind === "typesetting" ? (
          <TypesettingPanel
            chromeContent={projectChrome}
            key={`${appState.project.id}:typesetting`}
            onStateSaved={handleTypesettingStateSaved}
            panelId={appState.panel.id}
            projectId={appState.project.id}
            revision={typesettingRevisionFromAppState(appState)}
            state={typesettingStateFromAppState(appState)}
            transport={transport}
            wiki={wikiStateFromAppState(appState)}
          />
        ) : appState.activePanelKind === "publishing" ? (
          <PublishingPanel
            chromeContent={projectChrome}
            key={`${appState.project.id}:publishing`}
          />
        ) : (
          <WikiPanel
            chromeContent={projectChrome}
            key={`${appState.project.id}:${appState.activePanelKind}`}
            onOpenAgentTasks={openAgentTaskList}
            onReload={reloadCurrentProject}
            selectionVersion={wikiSelectionVersion}
            state={wikiStateFromAppState(appState)}
            transport={transport}
            writing={
              appState.activePanelKind === "writing"
                ? {
                    state: writingStateFromAppState(appState),
                    tasks: appState.tasks ?? [],
                  }
                : undefined
            }
          />
        )}
        <BottomPanelTabs
          activePanelKind={appState.activePanelKind}
          onSwitchPanel={switchPanel}
          panels={appState.panels.map(({ panel }) => panel)}
        />
        {operationNotice ? (
          <div
            className={`op-operation-notice${
              operationNotice.status === "failed"
                ? "op-operation-notice--failed"
                : ""
            }`}
            role="status"
          >
            <strong>
              {operationNotice.status === "completed"
                ? t`Agent work completed`
                : t`Agent work failed`}
            </strong>
            <span>
              {operationNotice.projectTitle ?? operationNotice.projectId}
              {" · "}
              {operationNotice.panelTitle ?? operationNotice.panelKind}
            </span>
          </div>
        ) : null}
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
        {updateAction ? null : (
          <StudioRuntimeStatus
            onRetry={retryUpdateReconnect}
            state={runtimeState}
          />
        )}
      </section>
      <AgentPanel
        activeTab={agentPanelTab}
        buildInfo={appState.buildInfo}
        isOpen={isTraceOpen}
        onClose={() => setIsTraceOpen(false)}
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
