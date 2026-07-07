import { Button } from "@heroui/react"
import {
  type Asset,
  type AssetStore,
  applyOpenPanelsTheme,
  CanvasPanel,
  type CanvasSelectionSnapshot,
  DataUrlAssetStore,
  detectOpenPanelsTheme,
  OpenPanelsI18nProvider,
  OpenPanelsThemeProvider,
  type StoreSnapshot,
  useOpenPanelsI18n,
} from "@openpanels/canvas"
import type { OpenPanelsPanel, OpenPanelsSession } from "@openpanels/protocol"
import { Check, Pencil, Plus } from "lucide-react"
import {
  StrictMode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { createRoot } from "react-dom/client"
import "./styles.css"

applyOpenPanelsTheme(detectOpenPanelsTheme())

const ACTIVE_SESSION_STORAGE_KEY = "openpanels.activeSessionId"
const APP_TOOL_GET_CANVAS_STATE = "openpanels_app_get_canvas_state_v016"
const APP_TOOL_SAVE_PANEL_STATE = "openpanels_app_save_panel_state_v016"
const APP_TOOL_SAVE_SELECTION_STATE = "openpanels_app_save_selection_state_v016"
const APP_TOOL_READ_ASSET = "openpanels_app_read_panel_asset_v016"

type OpenPanelsHostWindow = Window &
  typeof globalThis & {
    __OPENPANELS_API_BASE__?: string
    __OPENPANELS_BOOTSTRAP__?: BootstrapResponse
    __OPENPANELS_PROJECT_DIR__?: string
    __OPENPANELS_STORAGE_DIR__?: string
    openPanelsMcp?: {
      callServerTool?: (
        request: { arguments?: Record<string, unknown>; name: string },
        options?: { timeoutMs?: number }
      ) => Promise<{
        content?: Array<{ text?: string; type: string }>
        isError?: boolean
        structuredContent?: unknown
      }>
    }
    openai?: {
      rawToolResult?: {
        structuredContent?: {
          bootstrap?: BootstrapResponse
          serverUrl?: string
          projectDir?: string
        }
      }
      toolOutput?: {
        bootstrap?: BootstrapResponse
        projectDir?: string
        serverUrl?: string
      }
    }
  }

interface BootstrapResponse {
  panel: OpenPanelsPanel
  session: OpenPanelsSession
  sessions?: OpenPanelsSession[]
  state: StoreSnapshot
}

interface AppState extends BootstrapResponse {}

type OpenPanelsTransport =
  | {
      apiBase: string
      kind: "http"
    }
  | {
      kind: "mcp"
      projectDir?: string
    }
  | {
      apiBase?: string
      bootstrap: BootstrapResponse
      kind: "embedded"
      projectDir?: string
    }

function App({ transport }: { transport: OpenPanelsTransport }) {
  const { t } = useOpenPanelsI18n()
  const [appState, setAppState] = useState<AppState | null>(null)
  const [bootstrapError, setBootstrapError] = useState<string | null>(null)
  const [snapshot, setSnapshot] = useState<StoreSnapshot | null>(null)
  const [selection, setSelection] = useState<CanvasSelectionSnapshot | null>(
    null
  )
  const [sessions, setSessions] = useState<OpenPanelsSession[]>([])

  const loadProject = useCallback(
    async (sessionId?: string | null) => {
      setBootstrapError(null)
      const data = await loadBootstrap(transport, sessionId)
      const normalized = {
        ...data,
        state: normalizeSnapshot(data.state),
      }
      window.localStorage.setItem(
        ACTIVE_SESSION_STORAGE_KEY,
        normalized.session.id
      )
      setSelection(null)
      setAppState(normalized)
      setSnapshot(normalized.state)
      setSessions(data.sessions ?? (await fetchSessions(transport)))
    },
    [transport]
  )

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const sessionId =
        transport.kind === "http"
          ? window.localStorage.getItem(ACTIVE_SESSION_STORAGE_KEY)
          : null
      if (cancelled) return
      await loadProject(sessionId)
    })().catch((error) => {
      console.error("Failed to bootstrap OpenPanels", error)
      setBootstrapError(String(error?.message || error))
    })
    return () => {
      cancelled = true
    }
  }, [loadProject, transport.kind])

  useEffect(() => {
    if (!(appState && transport.kind === "http")) return
    const timer = window.setInterval(async () => {
      try {
        const activeSessionId = await fetchActiveSessionId(transport)
        if (activeSessionId && activeSessionId !== appState.session.id) {
          await loadProject(activeSessionId)
        }
      } catch (error) {
        console.error("Failed to sync OpenPanels active project", error)
      }
    }, 1000)
    return () => window.clearInterval(timer)
  }, [appState, loadProject, transport])

  const assetStore = useMemo(() => {
    if (!appState) return new DataUrlAssetStore()
    if (transport.kind === "embedded" && transport.apiBase) {
      return new OpenPanelsBrowserAssetStore(
        transport.apiBase,
        appState.session.id,
        appState.panel.id
      )
    }
    if (transport.kind !== "http") return new DataUrlAssetStore()
    return new OpenPanelsBrowserAssetStore(
      transport.apiBase,
      appState.session.id,
      appState.panel.id
    )
  }, [appState, transport])

  const saveSnapshot = useCallback((nextSnapshot: StoreSnapshot) => {
    setSnapshot(nextSnapshot)
  }, [])

  const createProject = useCallback(async () => {
    if (transport.kind !== "http") {
      await loadProject(null)
      return
    }
    const response = await apiFetch(transport.apiBase, "/api/projects", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ title: t`Untitled` }),
    })
    const data = (await response.json()) as BootstrapResponse
    window.localStorage.setItem(ACTIVE_SESSION_STORAGE_KEY, data.session.id)
    const normalized = {
      ...data,
      state: normalizeSnapshot(data.state),
    }
    setSelection(null)
    setAppState(normalized)
    setSnapshot(normalized.state)
    setSessions(data.sessions ?? (await fetchSessions(transport)))
  }, [loadProject, t, transport])

  const renameProject = useCallback(
    async (title: string) => {
      if (!appState) return
      if (transport.kind !== "http") return
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

  useEffect(() => {
    if (!(appState && snapshot)) return
    const timer = window.setTimeout(() => {
      savePanelState(
        transport,
        appState.session.id,
        appState.panel.id,
        snapshot
      ).catch((error) => {
        console.error("Failed to save OpenPanels canvas state", error)
      })
    }, 400)
    return () => window.clearTimeout(timer)
  }, [appState, snapshot, transport])

  useEffect(() => {
    if (!(appState && selection)) return
    const timer = window.setTimeout(() => {
      saveSelectionState(
        transport,
        appState.session.id,
        appState.panel.id,
        selection
      ).catch((error) => {
        console.error("Failed to save OpenPanels selection", error)
      })
    }, 300)
    return () => window.clearTimeout(timer)
  }, [appState, selection, transport])

  if (!(appState && snapshot)) {
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

  return (
    <main className="design-shell">
      <CanvasPanel
        assetStore={assetStore}
        height="100vh"
        key={`${appState.session.id}:${appState.panel.id}`}
        onSelectionChange={setSelection}
        onSnapshotChange={saveSnapshot}
        snapshot={snapshot}
        titleContent={
          <ProjectTitleControl
            currentSession={appState.session}
            onCreateProject={createProject}
            onRenameProject={renameProject}
            onSwitchProject={loadProject}
            sessions={sessions}
          />
        }
      />
    </main>
  )
}

function ProjectTitleControl({
  currentSession,
  sessions,
  onCreateProject,
  onRenameProject,
  onSwitchProject,
}: {
  currentSession: OpenPanelsSession
  onCreateProject: () => void
  onRenameProject: (title: string) => void
  onSwitchProject: (sessionId: string) => void
  sessions: OpenPanelsSession[]
}) {
  const { t } = useOpenPanelsI18n()
  const [isMenuOpen, setIsMenuOpen] = useState(false)
  const [isEditing, setIsEditing] = useState(false)
  const [draftTitle, setDraftTitle] = useState(currentSession.title)
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const clearCloseTimer = useCallback(() => {
    if (closeTimerRef.current) {
      clearTimeout(closeTimerRef.current)
      closeTimerRef.current = null
    }
  }, [])

  const openMenu = useCallback(() => {
    clearCloseTimer()
    setIsMenuOpen(true)
  }, [clearCloseTimer])

  const scheduleCloseMenu = useCallback(() => {
    clearCloseTimer()
    closeTimerRef.current = setTimeout(() => {
      setIsMenuOpen(false)
      closeTimerRef.current = null
    }, 180)
  }, [clearCloseTimer])

  useEffect(() => {
    if (!isEditing) {
      setDraftTitle(currentSession.title)
    }
  }, [currentSession.title, isEditing])

  useEffect(() => clearCloseTimer, [clearCloseTimer])

  const commitTitle = useCallback(() => {
    const nextTitle = draftTitle.trim()
    setIsEditing(false)
    setIsMenuOpen(false)
    if (nextTitle && nextTitle !== currentSession.title) {
      onRenameProject(nextTitle)
    } else {
      setDraftTitle(currentSession.title)
    }
  }, [currentSession.title, draftTitle, onRenameProject])

  if (isEditing) {
    return (
      <div className="op-project-title op-project-title--editing">
        <input
          aria-label={t`Project name`}
          autoFocus
          className="op-project-title__input"
          onBlur={commitTitle}
          onChange={(event) => setDraftTitle(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault()
              commitTitle()
            }
            if (event.key === "Escape") {
              event.preventDefault()
              setDraftTitle(currentSession.title)
              setIsEditing(false)
              setIsMenuOpen(false)
            }
          }}
          value={draftTitle}
        />
      </div>
    )
  }

  return (
    <div
      className="op-project-title"
      onMouseEnter={openMenu}
      onMouseLeave={scheduleCloseMenu}
    >
      <button
        className="op-project-title__trigger"
        onClick={() => setIsMenuOpen((open) => !open)}
        type="button"
      >
        <span>{currentSession.title}</span>
      </button>
      <Button
        aria-label={t`Rename project`}
        className="op-project-title__edit-button"
        isIconOnly
        onPress={() => {
          setIsMenuOpen(false)
          setIsEditing(true)
        }}
        size="sm"
        variant="ghost"
      >
        <Pencil size={14} strokeWidth={1.8} />
      </Button>

      {isMenuOpen ? (
        <div className="op-project-title__menu">
          <div className="op-project-title__menu-header">{t`Projects`}</div>
          <div className="op-project-title__menu-list">
            {sessions.map((session) => {
              const isActive = session.id === currentSession.id
              return (
                <button
                  className="op-project-title__menu-item"
                  key={session.id}
                  onClick={() => {
                    setIsMenuOpen(false)
                    if (!isActive) {
                      onSwitchProject(session.id)
                    }
                  }}
                  type="button"
                >
                  <span>{session.title}</span>
                  {isActive ? <Check size={14} /> : null}
                </button>
              )
            })}
          </div>
          <button
            className="op-project-title__menu-item op-project-title__menu-item--create"
            onClick={() => {
              setIsMenuOpen(false)
              onCreateProject()
            }}
            type="button"
          >
            <Plus size={14} />
            <span>{t`New project`}</span>
          </button>
        </div>
      ) : null}
    </div>
  )
}

class OpenPanelsBrowserAssetStore implements AssetStore {
  private readonly apiBase: string
  private readonly panelId: string
  private readonly sessionId: string

  constructor(apiBase: string, sessionId: string, panelId: string) {
    this.apiBase = apiBase
    this.sessionId = sessionId
    this.panelId = panelId
  }

  async upload(_asset: Partial<Asset>, file: File) {
    const dataUrl = await fileToDataUrl(file)
    const response = await apiFetch(
      this.apiBase,
      `/api/panels/${encodeURIComponent(this.sessionId)}/${encodeURIComponent(this.panelId)}/assets`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          dataUrl,
          fileName: file.name || "image.png",
          mimeType: file.type || "image/png",
        }),
      }
    )
    return (await response.json()) as {
      meta?: Record<string, unknown>
      mimeType?: string
      src: string
    }
  }

  resolve(asset: Asset): string {
    if (!("src" in asset.props)) return ""
    const src = asset.props.src
    if (typeof src !== "string" || !src.startsWith("/")) return src
    return apiUrl(this.apiBase, src).toString()
  }

  download(asset: Asset): Promise<string> {
    return Promise.resolve(this.resolve(asset))
  }
}

function normalizeSnapshot(snapshot: StoreSnapshot): StoreSnapshot {
  const selectedShapeIds = Array.isArray(snapshot.selectedShapeIds)
    ? new Set(snapshot.selectedShapeIds)
    : snapshot.selectedShapeIds instanceof Set
      ? snapshot.selectedShapeIds
      : new Set<string>()
  return {
    ...snapshot,
    selectedShapeIds,
  } as StoreSnapshot
}

function serializeSnapshot(snapshot: StoreSnapshot) {
  return {
    ...snapshot,
    selectedShapeIds: [...snapshot.selectedShapeIds],
  }
}

function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as string)
    reader.onerror = reject
    reader.readAsDataURL(file)
  })
}

function apiUrl(apiBase: string, path: string | URL): URL {
  if (path instanceof URL) return path
  return new URL(path, normalizedApiBase(apiBase))
}

function normalizedApiBase(apiBase: string): string {
  return apiBase.endsWith("/") ? apiBase : `${apiBase}/`
}

function apiFetch(
  apiBase: string,
  path: string | URL,
  init?: RequestInit
): Promise<Response> {
  return fetch(apiUrl(apiBase, path), init)
}

async function callOpenPanelsTool<T>(
  transport: OpenPanelsTransport,
  name: string,
  args: Record<string, unknown> = {}
): Promise<T> {
  if (transport.kind === "http") {
    throw new Error("OpenPanels MCP transport is not active.")
  }
  const hostWindow = window as OpenPanelsHostWindow
  const callServerTool = hostWindow.openPanelsMcp?.callServerTool
  if (typeof callServerTool !== "function") {
    throw new Error("OpenPanels host tool bridge is unavailable.")
  }
  const result = await callServerTool({
    name,
    arguments: {
      projectDir: transport.projectDir,
      ...args,
    },
  })
  if (result?.isError) {
    const message = result.content?.find((item) => item.type === "text")?.text
    throw new Error(message || `OpenPanels tool failed: ${name}`)
  }
  if (result?.structuredContent !== undefined) {
    return result.structuredContent as T
  }
  const text = result?.content?.find((item) => item.type === "text")?.text
  if (text) {
    return JSON.parse(text) as T
  }
  return result as T
}

async function loadBootstrap(
  transport: OpenPanelsTransport,
  sessionId?: string | null
): Promise<BootstrapResponse> {
  if (transport.kind === "http") {
    const url = apiUrl(transport.apiBase, "/api/bootstrap")
    if (sessionId) {
      url.searchParams.set("sessionId", sessionId)
    }
    const response = await apiFetch(transport.apiBase, url)
    return (await response.json()) as BootstrapResponse
  }

  if (transport.kind === "embedded") {
    return transport.bootstrap
  }

  const data = await callOpenPanelsTool<BootstrapResponse>(
    transport,
    APP_TOOL_GET_CANVAS_STATE,
    sessionId ? { sessionId } : {}
  )
  return {
    ...data,
    state: await hydrateMcpAssets(transport, data.state),
  }
}

async function hydrateMcpAssets(
  transport: OpenPanelsTransport,
  snapshot: StoreSnapshot
): Promise<StoreSnapshot> {
  const store =
    snapshot.store && typeof snapshot.store === "object" ? snapshot.store : {}
  const hydratedStore = { ...store }
  const entries = Object.entries(hydratedStore)

  await Promise.all(
    entries.map(async ([id, record]) => {
      if (
        !record ||
        typeof record !== "object" ||
        record.typeName !== "asset" ||
        record.type !== "image"
      ) {
        return
      }
      const assetRef = record.meta?.assetRef
      if (typeof assetRef !== "string" || !assetRef) return
      const src = record.props?.src
      if (typeof src === "string" && src.startsWith("data:")) return

      const asset = await callOpenPanelsTool<{ base64: string }>(
        transport,
        APP_TOOL_READ_ASSET,
        { assetRef }
      )
      const mimeType =
        typeof record.props?.mimeType === "string"
          ? record.props.mimeType
          : "image/png"
      hydratedStore[id] = {
        ...record,
        props: {
          ...record.props,
          src: `data:${mimeType};base64,${asset.base64}`,
        },
      }
    })
  )

  return {
    ...snapshot,
    store: hydratedStore,
  }
}

async function savePanelState(
  transport: OpenPanelsTransport,
  sessionId: string,
  panelId: string,
  snapshot: StoreSnapshot
) {
  if (transport.kind === "http") {
    await apiFetch(
      transport.apiBase,
      `/api/panels/${encodeURIComponent(sessionId)}/${encodeURIComponent(panelId)}/state`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(serializeSnapshot(snapshot)),
      }
    )
    return
  }

  if (transport.kind === "embedded" && transport.apiBase) {
    await apiFetch(
      transport.apiBase,
      `/api/panels/${encodeURIComponent(sessionId)}/${encodeURIComponent(panelId)}/state`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(serializeSnapshot(snapshot)),
      }
    )
    return
  }

  if (transport.kind === "embedded" && !transport.projectDir) return

  await callOpenPanelsTool(transport, APP_TOOL_SAVE_PANEL_STATE, {
    panelId,
    sessionId,
    state: serializeSnapshot(snapshot),
  })
}

async function saveSelectionState(
  transport: OpenPanelsTransport,
  sessionId: string,
  panelId: string,
  selection: CanvasSelectionSnapshot
) {
  const payload = {
    imageDataUrl: selection.imageDataUrl,
    selection: {
      assetRef: selection.assetRef,
      selectedShapeIds: selection.selectedShapeIds,
      selectedShapes: selection.selectedShapes,
    },
  }
  if (transport.kind === "http") {
    await apiFetch(
      transport.apiBase,
      `/api/panels/${encodeURIComponent(sessionId)}/${encodeURIComponent(panelId)}/selection`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      }
    )
    return
  }

  if (transport.kind === "embedded" && transport.apiBase) {
    await apiFetch(
      transport.apiBase,
      `/api/panels/${encodeURIComponent(sessionId)}/${encodeURIComponent(panelId)}/selection`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      }
    )
    return
  }

  if (transport.kind === "embedded" && !transport.projectDir) return

  await callOpenPanelsTool(transport, APP_TOOL_SAVE_SELECTION_STATE, {
    panelId,
    sessionId,
    ...payload,
  })
}

function localHttpOrigin(): string | null {
  if (
    window.location.protocol === "http:" &&
    ["127.0.0.1", "localhost"].includes(window.location.hostname)
  ) {
    return window.location.origin
  }
  return null
}

function hostServerUrl(): string | null {
  const hostWindow = window as OpenPanelsHostWindow
  return (
    hostWindow.__OPENPANELS_API_BASE__ ??
    hostWindow.openai?.toolOutput?.serverUrl ??
    hostWindow.openai?.rawToolResult?.structuredContent?.serverUrl ??
    null
  )
}

function hostProjectDir(): string | undefined {
  const hostWindow = window as OpenPanelsHostWindow
  return (
    hostWindow.__OPENPANELS_PROJECT_DIR__ ??
    hostWindow.openai?.toolOutput?.projectDir ??
    hostWindow.openai?.rawToolResult?.structuredContent?.projectDir
  )
}

function hostBootstrap(): BootstrapResponse | undefined {
  const hostWindow = window as OpenPanelsHostWindow
  return (
    hostWindow.__OPENPANELS_BOOTSTRAP__ ??
    hostWindow.openai?.toolOutput?.bootstrap ??
    hostWindow.openai?.rawToolResult?.structuredContent?.bootstrap
  )
}

function currentTransport(): OpenPanelsTransport | null {
  const localOrigin = localHttpOrigin()
  if (localOrigin) return { apiBase: localOrigin, kind: "http" }

  const hostWindow = window as OpenPanelsHostWindow
  const projectDir = hostProjectDir()
  const bootstrap = hostBootstrap()
  const serverUrl = hostServerUrl()
  if (bootstrap) {
    return {
      apiBase: serverUrl ?? undefined,
      bootstrap,
      kind: "embedded",
      projectDir,
    }
  }

  if (serverUrl) return { apiBase: serverUrl, kind: "http" }

  if (
    projectDir &&
    typeof hostWindow.openPanelsMcp?.callServerTool === "function"
  ) {
    return { kind: "mcp", projectDir }
  }

  return null
}

function transportKey(transport: OpenPanelsTransport | null): string {
  if (!transport) return "none"
  return transport.kind === "http"
    ? `http:${transport.apiBase}`
    : `${transport.kind}:${transport.projectDir ?? ""}:${transport.kind === "embedded" ? `${transport.bootstrap.session.id}:${transport.apiBase ?? ""}` : ""}`
}

function useOpenPanelsTransport(): OpenPanelsTransport | null {
  const [transport, setTransport] = useState(() => currentTransport())

  useEffect(() => {
    if (transport) return
    const syncTransport = () => {
      const nextTransport = currentTransport()
      if (nextTransport) {
        setTransport(nextTransport)
      }
    }
    const timer = window.setInterval(syncTransport, 100)
    window.addEventListener("message", syncTransport)
    window.addEventListener("openai:set_globals", syncTransport)
    syncTransport()
    return () => {
      window.clearInterval(timer)
      window.removeEventListener("message", syncTransport)
      window.removeEventListener("openai:set_globals", syncTransport)
    }
  }, [transport])

  return transport
}

function AppBootstrap() {
  const transport = useOpenPanelsTransport()

  if (!transport) {
    return (
      <main className="design-shell design-shell--status">
        <div className="op-boot-status">Loading canvas</div>
      </main>
    )
  }

  return <App key={transportKey(transport)} transport={transport} />
}

async function fetchSessions(transport: OpenPanelsTransport) {
  if (transport.kind !== "http") return []
  const response = await apiFetch(transport.apiBase, "/api/sessions")
  return (await response.json()) as OpenPanelsSession[]
}

async function fetchActiveSessionId(transport: OpenPanelsTransport) {
  if (transport.kind !== "http") return null
  const response = await apiFetch(transport.apiBase, "/api/active-session")
  const data = (await response.json()) as { sessionId?: string | null }
  return data.sessionId ?? null
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <OpenPanelsI18nProvider>
      <OpenPanelsThemeProvider>
        <AppBootstrap />
      </OpenPanelsThemeProvider>
    </OpenPanelsI18nProvider>
  </StrictMode>
)
