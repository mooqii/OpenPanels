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

interface BootstrapResponse {
  panel: OpenPanelsPanel
  session: OpenPanelsSession
  sessions?: OpenPanelsSession[]
  state: StoreSnapshot
}

interface AppState extends BootstrapResponse {}

function App() {
  const { t } = useOpenPanelsI18n()
  const [appState, setAppState] = useState<AppState | null>(null)
  const [snapshot, setSnapshot] = useState<StoreSnapshot | null>(null)
  const [selection, setSelection] = useState<CanvasSelectionSnapshot | null>(
    null
  )
  const [sessions, setSessions] = useState<OpenPanelsSession[]>([])

  const loadProject = useCallback(async (sessionId?: string | null) => {
    const url = new URL("/api/bootstrap", window.location.origin)
    if (sessionId) {
      url.searchParams.set("sessionId", sessionId)
    }
    const response = await fetch(url)
    const data = (await response.json()) as BootstrapResponse
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
    setSessions(data.sessions ?? (await fetchSessions()))
  }, [])

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const sessionId = window.localStorage.getItem(ACTIVE_SESSION_STORAGE_KEY)
      if (cancelled) return
      await loadProject(sessionId)
    })().catch((error) => {
      console.error("Failed to bootstrap OpenPanels", error)
    })
    return () => {
      cancelled = true
    }
  }, [loadProject])

  const assetStore = useMemo(() => {
    if (!appState) return new DataUrlAssetStore()
    return new OpenPanelsBrowserAssetStore(
      appState.session.id,
      appState.panel.id
    )
  }, [appState])

  const saveSnapshot = useCallback((nextSnapshot: StoreSnapshot) => {
    setSnapshot(nextSnapshot)
  }, [])

  const createProject = useCallback(async () => {
    const response = await fetch("/api/projects", {
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
    setSessions(data.sessions ?? (await fetchSessions()))
  }, [t])

  const renameProject = useCallback(
    async (title: string) => {
      if (!appState) return
      const response = await fetch(
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
    [appState]
  )

  useEffect(() => {
    if (!(appState && snapshot)) return
    const timer = window.setTimeout(() => {
      fetch(
        `/api/panels/${encodeURIComponent(appState.session.id)}/${encodeURIComponent(appState.panel.id)}/state`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(serializeSnapshot(snapshot)),
        }
      ).catch((error) => {
        console.error("Failed to save OpenPanels canvas state", error)
      })
    }, 400)
    return () => window.clearTimeout(timer)
  }, [appState, snapshot])

  useEffect(() => {
    if (!(appState && selection)) return
    const timer = window.setTimeout(() => {
      fetch(
        `/api/panels/${encodeURIComponent(appState.session.id)}/${encodeURIComponent(appState.panel.id)}/selection`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            selection: {
              selectedShapeIds: selection.selectedShapeIds,
              selectedShapes: selection.selectedShapes,
            },
            imageDataUrl: selection.imageDataUrl,
          }),
        }
      ).catch((error) => {
        console.error("Failed to save OpenPanels selection", error)
      })
    }, 300)
    return () => window.clearTimeout(timer)
  }, [appState, selection])

  if (!(appState && snapshot)) {
    return <main className="design-shell" />
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
  private readonly panelId: string
  private readonly sessionId: string

  constructor(sessionId: string, panelId: string) {
    this.sessionId = sessionId
    this.panelId = panelId
  }

  async upload(_asset: Partial<Asset>, file: File) {
    const dataUrl = await fileToDataUrl(file)
    const response = await fetch(
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
    return "src" in asset.props ? asset.props.src : ""
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

async function fetchSessions() {
  const response = await fetch("/api/sessions")
  return (await response.json()) as OpenPanelsSession[]
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <OpenPanelsI18nProvider>
      <OpenPanelsThemeProvider>
        <App />
      </OpenPanelsThemeProvider>
    </OpenPanelsI18nProvider>
  </StrictMode>
)
