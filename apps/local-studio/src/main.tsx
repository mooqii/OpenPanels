import { Button, Tabs } from "@heroui/react"
import {
  type Asset,
  type AssetStore,
  applyOpenPanelsTheme,
  CanvasMenu,
  CanvasPanel,
  type CanvasSelectionSnapshot,
  DataUrlAssetStore,
  detectOpenPanelsTheme,
  OPENPANELS_LOCALE_LABELS,
  OpenPanelsI18nProvider,
  type OpenPanelsLocale,
  OpenPanelsThemeProvider,
  type StoreSnapshot,
  useOpenPanelsI18n,
} from "@openpanels/canvas"
import type {
  OpenPanelsPanel,
  OpenPanelsPanelKind,
  OpenPanelsSession,
} from "@openpanels/protocol"
import {
  BookOpen,
  Edit3,
  ExternalLink,
  Eye,
  FilePlus,
  FileText,
  FolderOpen,
  MoreHorizontal,
  Palette,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Trash2,
  Upload,
  X,
  ZoomIn,
  ZoomOut,
} from "lucide-react"
import {
  type DragEvent,
  type ReactNode,
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

type OpenPanelsHostWindow = Window &
  typeof globalThis & {
    __OPENPANELS_API_BASE__?: string
    openai?: {
      rawToolResult?: {
        structuredContent?: {
          serverUrl?: string
        }
      }
      toolOutput?: {
        serverUrl?: string
      }
    }
  }

interface BootstrapResponse {
  activePanelId: string
  activePanelKind: OpenPanelsPanelKind
  panel: OpenPanelsPanel
  panels: PanelStateSnapshot[]
  session: OpenPanelsSession
  sessions?: OpenPanelsSession[]
  state: unknown
}

interface AppState extends BootstrapResponse {}

interface PanelStateSnapshot {
  panel: OpenPanelsPanel
  state: unknown
}

interface WikiState {
  activeRawDocumentId: string | null
  activeWikiPagePath: string | null
  activeWikiSpaceId: string | null
  rawDocuments: WikiRawDocument[]
  ruleSets: unknown[]
  schemaVersion: 2
  tasks: WikiTask[]
  wikiLanguage?: OpenPanelsLocale | null
  wikiSpaces: WikiSpace[]
}

interface WikiRawDocument {
  conversion: {
    error: string | null
    status: "failed" | "not_required" | "queued" | "converting" | "ready"
    taskId: string | null
    updatedAt: string
  }
  createdAt: string
  id: string
  ingestionByWikiSpace: Record<
    string,
    {
      error: string | null
      markdownVersion?: number
      status: string
      taskId: string | null
      updatedAt?: string
    }
  >
  markdownRef: string | null
  markdownVersion: number
  mimeType: string
  originalFileName: string
  originalRef: string
  sha256: string
  sizeBytes: number
  source: "agent" | "user"
  title: string
  updatedAt: string
}

interface WikiSpace {
  id: string
  pageIndex: WikiPageIndexItem[]
  title: string
}

interface WikiPageIndexItem {
  path: string
  summary: string
  title: string
  type: string
  updatedAt: string
}

interface WikiTask {
  error: string | null
  id: string
  status: string
  targetId: string
  type: string
  wikiSpaceId: string | null
}

type OriginalPreviewKind = "audio" | "image" | "pdf" | "video"

const WIKI_LANGUAGE_OPTIONS: OpenPanelsLocale[] = ["en", "zh-CN"]

type OpenPanelsTransport = {
  apiBase: string
  kind: "http"
}

function App({ transport }: { transport: OpenPanelsTransport }) {
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
  const isLocalSnapshotDirtyRef = useRef(false)
  const lastPersistedSnapshotJsonRef = useRef<string | null>(null)
  const lastPersistedBootstrapJsonRef = useRef<string | null>(null)

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
      isLocalSnapshotDirtyRef.current = false
      lastPersistedSnapshotJsonRef.current = nextCanvasSnapshot
        ? JSON.stringify(serializeSnapshot(nextCanvasSnapshot))
        : null
      lastPersistedBootstrapJsonRef.current =
        serializeBootstrapForCompare(normalized)
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

  useEffect(() => {
    if (!(appState && transport.kind === "http")) return
    const timer = window.setInterval(async () => {
      try {
        const activeSessionId = await fetchActiveSessionId(transport)
        if (activeSessionId && activeSessionId !== appState.session.id) {
          await loadProject(activeSessionId)
          return
        }
        if (!activeSessionId || isLocalSnapshotDirtyRef.current) {
          return
        }
        const data = await loadBootstrap(transport, activeSessionId)
        const normalized = normalizeBootstrap(data)
        const remoteBootstrapJson = serializeBootstrapForCompare(normalized)
        if (remoteBootstrapJson !== lastPersistedBootstrapJsonRef.current) {
          const nextCanvasSnapshot = canvasSnapshotFromState(normalized)
          setSelection(null)
          setAppState(normalized)
          setCanvasSnapshot(nextCanvasSnapshot)
          setSessions(data.sessions ?? (await fetchSessions(transport)))
          lastPersistedSnapshotJsonRef.current = nextCanvasSnapshot
            ? JSON.stringify(serializeSnapshot(nextCanvasSnapshot))
            : null
          lastPersistedBootstrapJsonRef.current = remoteBootstrapJson
          setSnapshotLoadVersion((version) => version + 1)
        }
      } catch (error) {
        console.error("Failed to sync OpenPanels active project", error)
      }
    }, 1500)
    return () => window.clearInterval(timer)
  }, [appState, loadProject, transport])

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
    isLocalSnapshotDirtyRef.current = true
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
    isLocalSnapshotDirtyRef.current = false
    lastPersistedSnapshotJsonRef.current = nextCanvasSnapshot
      ? JSON.stringify(serializeSnapshot(nextCanvasSnapshot))
      : null
    lastPersistedBootstrapJsonRef.current =
      serializeBootstrapForCompare(normalized)
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
      setSelection(null)
      setAppState((current) =>
        current && current.session.id === appState.session.id
          ? {
              ...current,
              activePanelId: data.activePanelId,
              activePanelKind: data.activePanelKind,
              panel: data.panel,
              panels: current.panels.map((snapshot) =>
                snapshot.panel.id === data.panel.id
                  ? {
                      panel: data.panel,
                      state: normalizePanelState(data.panel.kind, data.state),
                    }
                  : snapshot
              ),
              state: normalizePanelState(data.panel.kind, data.state),
            }
          : current
      )
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
      )
        .then(() => {
          isLocalSnapshotDirtyRef.current = false
          lastPersistedSnapshotJsonRef.current = JSON.stringify(
            serializeSnapshot(canvasSnapshot)
          )
          lastPersistedBootstrapJsonRef.current = appState
            ? serializeBootstrapForCompare(appState)
            : null
        })
        .catch((error) => {
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
    <main className="design-shell">
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
    </main>
  )
}

function WikiPanel({
  chromeContent,
  onReload,
  state,
  transport,
}: {
  chromeContent: ReactNode
  onReload: () => Promise<void>
  state: WikiState
  transport: OpenPanelsTransport
}) {
  const { locale, t } = useOpenPanelsI18n()
  const initialWikiLanguageRef = useRef<OpenPanelsLocale>(locale)
  const activeSpace =
    state.wikiSpaces.find((space) => space.id === state.activeWikiSpaceId) ??
    state.wikiSpaces[0]
  const wikiLanguage = isWikiLanguage(state.wikiLanguage)
    ? state.wikiLanguage
    : initialWikiLanguageRef.current
  const [markdownDialog, setMarkdownDialog] = useState<{
    content: string
    document: WikiRawDocument
    originalContent: string
  } | null>(null)
  const [pageDialog, setPageDialog] = useState<{
    content: string
    originalContent: string
    pagePath: string
    title: string
  } | null>(null)
  const [pendingDeleteDocument, setPendingDeleteDocument] =
    useState<WikiRawDocument | null>(null)
  const [originalPreviewDocument, setOriginalPreviewDocument] =
    useState<WikiRawDocument | null>(null)
  const [isBusy, setIsBusy] = useState(false)
  const [isRawDragActive, setIsRawDragActive] = useState(false)
  const rawDragDepthRef = useRef(0)
  const fileInputRef = useRef<HTMLInputElement | null>(null)

  const openMarkdown = useCallback(
    async (document: WikiRawDocument) => {
      const response = await apiFetch(
        transport.apiBase,
        `/api/wiki/raw-documents/${encodeURIComponent(document.id)}/markdown`
      )
      const data = (await response.json()) as { markdown: string }
      setMarkdownDialog({
        document,
        content: data.markdown ?? "",
        originalContent: data.markdown ?? "",
      })
    },
    [transport]
  )

  const saveMarkdown = useCallback(async () => {
    if (!markdownDialog) return
    if (markdownDialog.content === markdownDialog.originalContent) {
      setMarkdownDialog(null)
      return
    }
    setIsBusy(true)
    try {
      await apiFetch(
        transport.apiBase,
        `/api/wiki/raw-documents/${encodeURIComponent(markdownDialog.document.id)}/markdown`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            content: markdownDialog.content,
            expectedVersion: markdownDialog.document.markdownVersion,
          }),
        }
      )
      setMarkdownDialog(null)
      await onReload()
    } finally {
      setIsBusy(false)
    }
  }, [markdownDialog, onReload, transport])

  const extractMarkdown = useCallback(
    async (document: WikiRawDocument) => {
      if (!activeSpace) return
      setIsBusy(true)
      try {
        await apiFetch(
          transport.apiBase,
          `/api/wiki/raw-documents/${encodeURIComponent(document.id)}/extract?wikiSpaceId=${encodeURIComponent(activeSpace.id)}`,
          { method: "POST" }
        )
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [activeSpace, onReload, transport]
  )

  const reindexDocument = useCallback(
    async (document: WikiRawDocument) => {
      if (!activeSpace) return
      setIsBusy(true)
      try {
        await apiFetch(
          transport.apiBase,
          `/api/wiki/raw-documents/${encodeURIComponent(document.id)}/reindex?wikiSpaceId=${encodeURIComponent(activeSpace.id)}`,
          { method: "POST" }
        )
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [activeSpace, onReload, transport]
  )

  const deleteRawDocument = useCallback(
    async (document: WikiRawDocument) => {
      if (!activeSpace) return
      setIsBusy(true)
      try {
        await apiFetch(
          transport.apiBase,
          `/api/wiki/raw-documents/${encodeURIComponent(document.id)}?wikiSpaceId=${encodeURIComponent(activeSpace.id)}`,
          { method: "DELETE" }
        )
        setPendingDeleteDocument(null)
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [activeSpace, onReload, transport]
  )

  const openOriginalInNewWindow = useCallback(
    (document: WikiRawDocument) => {
      window.open(
        wikiRawOriginalUrl(transport.apiBase, document),
        "_blank",
        "noopener,noreferrer"
      )
    },
    [transport.apiBase]
  )

  const revealOriginal = useCallback(
    async (document: WikiRawDocument) => {
      setIsBusy(true)
      try {
        await apiFetch(
          transport.apiBase,
          `/api/wiki/raw-documents/${encodeURIComponent(document.id)}/reveal`,
          { method: "POST" }
        )
      } finally {
        setIsBusy(false)
      }
    },
    [transport.apiBase]
  )

  const addFiles = useCallback(
    async (files: FileList | null) => {
      if (!files?.length) return
      setIsBusy(true)
      try {
        for (const file of [...files]) {
          await apiFetch(transport.apiBase, "/api/wiki/raw-documents", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              dataUrl: await fileToDataUrl(file),
              fileName: file.name,
              mimeType: file.type || "application/octet-stream",
              title: titleFromFileName(file.name),
              source: "user",
              wikiSpaceId: activeSpace?.id,
            }),
          })
        }
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [activeSpace?.id, onReload, transport]
  )

  const handleRawDragEnter = useCallback((event: DragEvent<HTMLElement>) => {
    if (!event.dataTransfer.types.includes("Files")) return
    event.preventDefault()
    rawDragDepthRef.current += 1
    setIsRawDragActive(true)
  }, [])

  const handleRawDragOver = useCallback((event: DragEvent<HTMLElement>) => {
    if (!event.dataTransfer.types.includes("Files")) return
    event.preventDefault()
    event.dataTransfer.dropEffect = "copy"
  }, [])

  const handleRawDragLeave = useCallback((event: DragEvent<HTMLElement>) => {
    if (!event.dataTransfer.types.includes("Files")) return
    event.preventDefault()
    rawDragDepthRef.current = Math.max(0, rawDragDepthRef.current - 1)
    if (rawDragDepthRef.current === 0) {
      setIsRawDragActive(false)
    }
  }, [])

  const handleRawDrop = useCallback(
    async (event: DragEvent<HTMLElement>) => {
      if (!event.dataTransfer.types.includes("Files")) return
      event.preventDefault()
      rawDragDepthRef.current = 0
      setIsRawDragActive(false)
      await addFiles(event.dataTransfer.files)
    },
    [addFiles]
  )

  const createRawMarkdown = useCallback(async () => {
    const title = t`Untitled note`
    setIsBusy(true)
    try {
      await apiFetch(transport.apiBase, "/api/wiki/raw-documents", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          content: `# ${title}\n\n`,
          fileName: "untitled-note.md",
          mimeType: "text/markdown",
          title,
          source: "user",
          wikiSpaceId: activeSpace?.id,
        }),
      })
      await onReload()
    } finally {
      setIsBusy(false)
    }
  }, [activeSpace?.id, onReload, t, transport])

  const createWikiPage = useCallback(async () => {
    const pagePath = `topics/untitled-${Date.now().toString(36)}.md`
    const title = t`Untitled page`
    setIsBusy(true)
    try {
      await apiFetch(
        transport.apiBase,
        `/api/wiki/spaces/${encodeURIComponent(activeSpace?.id ?? "wiki:default")}/pages`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            pagePath,
            title,
            content: `---\ntitle: "${title}"\ntype: "topic"\nsummary: ""\ntags: []\nsourceDocumentIds: []\nupdatedAt: "${new Date().toISOString()}"\n---\n\n# ${title}\n\n`,
          }),
        }
      )
      await onReload()
    } finally {
      setIsBusy(false)
    }
  }, [activeSpace?.id, onReload, t, transport])

  const openWikiPage = useCallback(
    async (pagePath: string) => {
      const response = await apiFetch(
        transport.apiBase,
        `/api/wiki/spaces/${encodeURIComponent(activeSpace?.id ?? "wiki:default")}/pages/${pagePath
          .split("/")
          .map(encodeURIComponent)
          .join("/")}`
      )
      const data = (await response.json()) as { markdown: string }
      setPageDialog({
        pagePath,
        title: titleFromFileName(pagePath),
        content: data.markdown ?? "",
        originalContent: data.markdown ?? "",
      })
    },
    [activeSpace?.id, transport]
  )

  const saveWikiPage = useCallback(async () => {
    if (!(pageDialog && activeSpace)) return
    if (pageDialog.content === pageDialog.originalContent) {
      setPageDialog(null)
      return
    }
    setIsBusy(true)
    try {
      await apiFetch(
        transport.apiBase,
        `/api/wiki/spaces/${encodeURIComponent(activeSpace.id)}/pages/${pageDialog.pagePath
          .split("/")
          .map(encodeURIComponent)
          .join("/")}`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            title: pageDialog.title,
            content: pageDialog.content,
          }),
        }
      )
      setPageDialog(null)
      await onReload()
    } finally {
      setIsBusy(false)
    }
  }, [activeSpace, onReload, pageDialog, transport])

  const updateWikiLanguage = useCallback(
    async (language: OpenPanelsLocale) => {
      setIsBusy(true)
      try {
        const response = await apiFetch(
          transport.apiBase,
          "/api/wiki/language",
          {
            method: "PUT",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ language }),
          }
        )
        if (!response.ok) {
          if (response.status === 404) return
          throw new Error(`Failed to update wiki language: ${response.status}`)
        }
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [onReload, transport.apiBase]
  )

  return (
    <section className="op-wiki-panel">
      <header className="op-canvas-title">{chromeContent}</header>
      <div className="op-wiki-panel__surface">
        <div className="op-wiki-workbench">
          <aside
            className={
              isRawDragActive
                ? "op-wiki-column op-wiki-column--raw op-wiki-column--drop-active"
                : "op-wiki-column op-wiki-column--raw"
            }
            onDragEnter={handleRawDragEnter}
            onDragLeave={handleRawDragLeave}
            onDragOver={handleRawDragOver}
            onDrop={handleRawDrop}
          >
            <div className="op-wiki-drop-hint">{t`Drop files to upload`}</div>
            <div className="op-wiki-column__header">
              <div>
                <div className="op-wiki-panel__label">{t`Raw`}</div>
                <h2>{t`Raw Documents`}</h2>
              </div>
              <div className="op-wiki-actions">
                <button
                  aria-label={t`Upload document`}
                  className="op-wiki-icon-button"
                  disabled={isBusy}
                  onClick={() => fileInputRef.current?.click()}
                  type="button"
                >
                  <Upload size={16} />
                </button>
                <button
                  aria-label={t`New Markdown`}
                  className="op-wiki-icon-button"
                  disabled={isBusy}
                  onClick={createRawMarkdown}
                  type="button"
                >
                  <FilePlus size={16} />
                </button>
              </div>
              <input
                hidden
                multiple
                onChange={(event) => {
                  addFiles(event.currentTarget.files)
                  event.currentTarget.value = ""
                }}
                ref={fileInputRef}
                type="file"
              />
            </div>
            <div className="op-wiki-list">
              {state.rawDocuments.length ? (
                state.rawDocuments.map((document) => {
                  const previewKind = originalPreviewKind(document)
                  const hasMarkdown = Boolean(document.markdownRef)
                  const indexStatus = documentIndexStatus(
                    document,
                    activeSpace?.id
                  )
                  return (
                    <div
                      className={
                        hasMarkdown || previewKind
                          ? "op-wiki-list-item op-wiki-list-item--interactive"
                          : "op-wiki-list-item"
                      }
                      key={document.id}
                    >
                      <button
                        className="op-wiki-list-item__body"
                        disabled={!(hasMarkdown || previewKind)}
                        onClick={() => {
                          if (previewKind) {
                            setOriginalPreviewDocument(document)
                            return
                          }
                          openMarkdown(document)
                        }}
                        type="button"
                      >
                        <div>
                          <strong className="op-wiki-list-item__title">
                            {document.title}
                          </strong>
                          <span className="op-wiki-list-item__meta">
                            {document.originalFileName}
                          </span>
                        </div>
                      </button>
                      <div className="op-wiki-list-item__tools">
                        {hasMarkdown && indexStatus.kind !== "done" ? (
                          <WikiIndexStatus status={indexStatus} />
                        ) : null}
                        <WikiStatus document={document} />
                        <details
                          className="op-wiki-row-menu"
                          onMouseLeave={(event) => {
                            const nextTarget = event.relatedTarget
                            if (
                              nextTarget instanceof Node &&
                              event.currentTarget.contains(nextTarget)
                            ) {
                              return
                            }
                            event.currentTarget.removeAttribute("open")
                          }}
                        >
                          <summary
                            aria-label={t`Document actions`}
                            className="op-wiki-icon-button"
                          >
                            <MoreHorizontal size={16} />
                          </summary>
                          <div className="op-wiki-row-menu__popover">
                            <button
                              disabled={isBusy || !previewKind}
                              onClick={(event) => {
                                event.currentTarget
                                  .closest("details")
                                  ?.removeAttribute("open")
                                setOriginalPreviewDocument(document)
                              }}
                              title={
                                previewKind
                                  ? t`Preview original file`
                                  : t`Preview is not available for this file type`
                              }
                              type="button"
                            >
                              <Eye size={14} />
                              <span>{t`Preview original file`}</span>
                            </button>
                            <button
                              disabled={isBusy}
                              onClick={(event) => {
                                event.currentTarget
                                  .closest("details")
                                  ?.removeAttribute("open")
                                openOriginalInNewWindow(document)
                              }}
                              type="button"
                            >
                              <ExternalLink size={14} />
                              <span>{t`Open in new window`}</span>
                            </button>
                            <button
                              disabled={isBusy}
                              onClick={(event) => {
                                event.currentTarget
                                  .closest("details")
                                  ?.removeAttribute("open")
                                revealOriginal(document).catch((error) => {
                                  console.error(
                                    "Failed to reveal wiki raw document",
                                    error
                                  )
                                })
                              }}
                              type="button"
                            >
                              <FolderOpen size={14} />
                              <span>{t`Show in folder`}</span>
                            </button>
                            {hasMarkdown ? (
                              <button
                                disabled={isBusy}
                                onClick={(event) => {
                                  event.currentTarget
                                    .closest("details")
                                    ?.removeAttribute("open")
                                  reindexDocument(document).catch((error) => {
                                    console.error(
                                      "Failed to reindex wiki document",
                                      error
                                    )
                                  })
                                }}
                                type="button"
                              >
                                <RefreshCw size={14} />
                                <span className="op-wiki-row-menu__label">
                                  <span>{t`Reindex`}</span>
                                  <span
                                    className={`op-wiki-index-tag op-wiki-index-tag--${indexStatus.kind}`}
                                  >
                                    {t(indexStatus.label)}
                                  </span>
                                </span>
                              </button>
                            ) : (
                              <button
                                disabled={isBusy}
                                onClick={(event) => {
                                  event.currentTarget
                                    .closest("details")
                                    ?.removeAttribute("open")
                                  extractMarkdown(document).catch((error) => {
                                    console.error(
                                      "Failed to extract wiki raw document",
                                      error
                                    )
                                  })
                                }}
                                type="button"
                              >
                                <RefreshCw size={14} />
                                <span>{t`Re-extract`}</span>
                              </button>
                            )}
                            <button
                              className="op-wiki-row-menu__danger"
                              disabled={isBusy}
                              onClick={(event) => {
                                event.currentTarget
                                  .closest("details")
                                  ?.removeAttribute("open")
                                setPendingDeleteDocument(document)
                              }}
                              type="button"
                            >
                              <Trash2 size={14} />
                              <span>{t`Delete`}</span>
                            </button>
                          </div>
                        </details>
                      </div>
                    </div>
                  )
                })
              ) : (
                <div className="op-wiki-empty-inline">{t`No raw documents yet`}</div>
              )}
            </div>
          </aside>

          <section className="op-wiki-column op-wiki-column--structured">
            <div className="op-wiki-column__header">
              <div>
                <div className="op-wiki-panel__label">Wiki</div>
                <h2>
                  {activeSpace?.title
                    ? t(activeSpace.title)
                    : t`Structured Wiki`}
                </h2>
              </div>
              <button
                className="op-wiki-command-button"
                disabled={isBusy}
                onClick={createWikiPage}
                type="button"
              >
                <FilePlus size={15} />
                <span>{t`New Markdown`}</span>
              </button>
            </div>
            <div className="op-wiki-page-grid">
              {(activeSpace?.pageIndex.length
                ? activeSpace.pageIndex
                : [
                    {
                      path: "index.md",
                      title: t`Index`,
                      summary: "",
                      type: "overview",
                      updatedAt: "",
                    },
                  ]
              ).map((page) => (
                <button
                  className="op-wiki-page-row"
                  key={page.path}
                  onClick={() => openWikiPage(page.path)}
                  type="button"
                >
                  <span className="op-wiki-page-row__icon">
                    <BookOpen size={16} />
                  </span>
                  <span className="op-wiki-page-row__body">
                    <strong className="op-wiki-list-item__title">
                      {page.title ? t(page.title) : page.path}
                    </strong>
                    <span className="op-wiki-page-row__summary">
                      {page.summary || page.path}
                    </span>
                  </span>
                  <span className="op-wiki-page-row__side">
                    <span className="op-wiki-page-row__type">
                      {formatWikiPageType(page.type, t)}
                    </span>
                    <Edit3 className="op-wiki-page-row__edit" size={14} />
                  </span>
                </button>
              ))}
            </div>
            <div className="op-wiki-column__footer">
              <label className="op-wiki-language-select">
                <span>{t`Wiki language`}</span>
                <select
                  aria-label={t`Wiki language`}
                  disabled={isBusy}
                  onChange={(event) => {
                    updateWikiLanguage(
                      event.currentTarget.value as OpenPanelsLocale
                    ).catch(() => undefined)
                  }}
                  value={wikiLanguage}
                >
                  {WIKI_LANGUAGE_OPTIONS.map((language) => (
                    <option key={language} value={language}>
                      {OPENPANELS_LOCALE_LABELS[language]}
                    </option>
                  ))}
                </select>
              </label>
              {state.tasks.length ? (
                <div className="op-wiki-task-strip">
                  {state.tasks.slice(0, 4).map((task) => (
                    <span key={task.id}>
                      {formatWikiTaskType(task.type, t)} ·{" "}
                      {formatWikiTaskStatus(task.status, t)}
                    </span>
                  ))}
                </div>
              ) : null}
            </div>
          </section>
        </div>
      </div>

      {markdownDialog ? (
        <MarkdownDialog
          closeLabel={t`Close`}
          content={markdownDialog.content}
          isBusy={isBusy}
          onChange={(content) =>
            setMarkdownDialog((current) =>
              current ? { ...current, content } : current
            )
          }
          onClose={() => setMarkdownDialog(null)}
          onSave={saveMarkdown}
          saveLabel={t`Save Markdown`}
          title={markdownDialog.document.title}
          titleLabel={t`Markdown`}
        />
      ) : null}

      {pageDialog ? (
        <MarkdownDialog
          closeLabel={t`Close`}
          content={pageDialog.content}
          isBusy={isBusy}
          onChange={(content) =>
            setPageDialog((current) =>
              current ? { ...current, content } : current
            )
          }
          onClose={() => setPageDialog(null)}
          onSave={saveWikiPage}
          saveLabel={t`Save Markdown`}
          title={pageDialog.pagePath}
          titleLabel={t`Markdown`}
        />
      ) : null}

      {originalPreviewDocument ? (
        <OriginalPreviewDialog
          closeLabel={t`Close`}
          document={originalPreviewDocument}
          key={originalPreviewDocument.id}
          onClose={() => setOriginalPreviewDocument(null)}
          previewUrl={wikiRawOriginalUrl(
            transport.apiBase,
            originalPreviewDocument
          )}
          titleLabel={t`Original file`}
        />
      ) : null}

      {pendingDeleteDocument ? (
        <ConfirmDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Delete`}
          isBusy={isBusy}
          message={t`This raw document will be removed from the source library.`}
          onCancel={() => setPendingDeleteDocument(null)}
          onConfirm={() =>
            deleteRawDocument(pendingDeleteDocument).catch((error) => {
              console.error("Failed to delete wiki raw document", error)
            })
          }
          title={t`Delete document?`}
        />
      ) : null}
    </section>
  )
}

function WikiStatus({ document }: { document: WikiRawDocument }) {
  const { t } = useOpenPanelsI18n()
  if (document.conversion.status === "failed") {
    return (
      <span className="op-wiki-status op-wiki-status--failed">
        {t`Conversion failed`}
      </span>
    )
  }
  if (
    document.conversion.status === "queued" ||
    document.conversion.status === "converting"
  ) {
    return <span className="op-wiki-status">{t`Converting`}</span>
  }
  return (
    <span className="op-wiki-status op-wiki-status--ready">
      <FileText size={15} />
    </span>
  )
}

function WikiIndexStatus({
  status,
}: {
  status: ReturnType<typeof documentIndexStatus>
}) {
  const { t } = useOpenPanelsI18n()
  return (
    <span
      className={`op-wiki-list-index-status op-wiki-list-index-status--${status.kind}`}
    >
      {t(status.label)}
    </span>
  )
}

function formatWikiPageType(
  type: string,
  t: (input: TemplateStringsArray | string, ...values: unknown[]) => string
) {
  switch (type) {
    case "overview":
      return t`Overview`
    case "log":
      return t`Log`
    case "source":
      return t`Source`
    case "topic":
      return t`Topic`
    case "entity":
      return t`Entity`
    case "category":
      return t`Category`
    default:
      return type.replaceAll("_", " ") || t`Page`
  }
}

function formatWikiTaskType(
  type: string,
  t: (input: TemplateStringsArray | string, ...values: unknown[]) => string
) {
  switch (type) {
    case "convert_document_to_markdown":
      return t`Convert to Markdown`
    case "ingest_markdown_into_wiki":
      return t`Update wiki`
    case "rebuild_wiki_index":
      return t`Rebuild wiki index`
    case "lint_wiki":
      return t`Check wiki`
    default:
      return type.replaceAll("_", " ")
  }
}

function formatWikiTaskStatus(
  status: string,
  t: (input: TemplateStringsArray | string, ...values: unknown[]) => string
) {
  switch (status) {
    case "queued":
      return t`Queued`
    case "claimed":
      return t`Claimed`
    case "running":
      return t`Running`
    case "failed":
      return t`Failed`
    case "succeeded":
      return t`Succeeded`
    case "stale":
      return t`Stale`
    default:
      return status
  }
}

function documentIndexStatus(
  document: WikiRawDocument,
  wikiSpaceId: string | null | undefined
): { kind: "done" | "failed" | "pending" | "running"; label: string } {
  const ingestion = wikiSpaceId
    ? document.ingestionByWikiSpace[wikiSpaceId]
    : undefined
  if (ingestion?.status === "ingested") {
    return { kind: "done", label: "Indexed" }
  }
  if (ingestion?.status === "failed") {
    return { kind: "failed", label: "Index failed" }
  }
  if (ingestion?.status === "ingesting") {
    return { kind: "running", label: "Indexing" }
  }
  return { kind: "pending", label: "Pending index" }
}

function isWikiLanguage(language: unknown): language is OpenPanelsLocale {
  return language === "en" || language === "zh-CN"
}

function MarkdownDialog({
  content,
  isBusy,
  onChange,
  onClose,
  onSave,
  closeLabel,
  saveLabel,
  title,
  titleLabel,
}: {
  closeLabel: string
  content: string
  isBusy: boolean
  onChange: (content: string) => void
  onClose: () => void
  onSave: () => void
  saveLabel: string
  title: string
  titleLabel: string
}) {
  return (
    <div
      aria-modal="true"
      className="op-markdown-dialog"
      onClick={onClose}
      role="dialog"
    >
      <div
        className="op-markdown-dialog__panel"
        onClick={(event) => event.stopPropagation()}
      >
        <header>
          <div>
            <div className="op-wiki-panel__label">{titleLabel}</div>
            <h2>{title}</h2>
          </div>
          <div className="op-wiki-actions">
            <button
              aria-label={saveLabel}
              className="op-wiki-icon-button"
              disabled={isBusy}
              onClick={onSave}
              type="button"
            >
              <Save size={16} />
            </button>
            <button
              aria-label={closeLabel}
              className="op-wiki-icon-button"
              onClick={onClose}
              type="button"
            >
              <X size={16} />
            </button>
          </div>
        </header>
        <textarea
          className="op-markdown-dialog__editor"
          onChange={(event) => onChange(event.currentTarget.value)}
          value={content}
        />
      </div>
    </div>
  )
}

function OriginalPreviewDialog({
  closeLabel,
  document,
  onClose,
  previewUrl,
  titleLabel,
}: {
  closeLabel: string
  document: WikiRawDocument
  onClose: () => void
  previewUrl: string
  titleLabel: string
}) {
  const kind = originalPreviewKind(document)
  const [imageScale, setImageScale] = useState(1)

  if (!kind) return null

  if (kind === "image") {
    return (
      <div
        aria-modal="true"
        className="op-image-preview"
        onClick={onClose}
        role="dialog"
      >
        <div
          className="op-image-preview__stage"
          onWheel={(event) => {
            event.preventDefault()
            setImageScale((current) =>
              clampImageScale(current + (event.deltaY < 0 ? 0.12 : -0.12))
            )
          }}
        >
          <img
            alt={document.title}
            onClick={(event) => event.stopPropagation()}
            src={previewUrl}
            style={{ transform: `scale(${imageScale})` }}
          />
        </div>
        <div
          className="op-image-preview__controls"
          onClick={(event) => event.stopPropagation()}
        >
          <button
            aria-label="Zoom out"
            onClick={() =>
              setImageScale((current) => clampImageScale(current - 0.2))
            }
            type="button"
          >
            <ZoomOut size={16} />
          </button>
          <button
            aria-label="Zoom in"
            onClick={() =>
              setImageScale((current) => clampImageScale(current + 0.2))
            }
            type="button"
          >
            <ZoomIn size={16} />
          </button>
          <button aria-label={closeLabel} onClick={onClose} type="button">
            <X size={16} />
          </button>
        </div>
      </div>
    )
  }

  return (
    <div
      aria-modal="true"
      className="op-markdown-dialog"
      onClick={onClose}
      role="dialog"
    >
      <div
        className="op-markdown-dialog__panel op-original-preview-dialog__panel"
        onClick={(event) => event.stopPropagation()}
      >
        <header>
          <div>
            <div className="op-wiki-panel__label">{titleLabel}</div>
            <h2>{document.title}</h2>
            <p className="op-original-preview-dialog__meta">
              {[document.originalFileName, formatBytes(document.sizeBytes)]
                .filter(Boolean)
                .join(" · ")}
            </p>
          </div>
          <button
            aria-label={closeLabel}
            className="op-wiki-icon-button"
            onClick={onClose}
            type="button"
          >
            <X size={16} />
          </button>
        </header>
        <div className="op-original-preview-dialog__body">
          {kind === "pdf" ? (
            <iframe src={previewUrl} title={document.title} />
          ) : null}
          {kind === "audio" ? (
            // biome-ignore lint/a11y/useMediaCaption: Raw file previews do not have caption tracks.
            <audio controls src={previewUrl}>
              {document.originalFileName}
            </audio>
          ) : null}
          {kind === "video" ? (
            // biome-ignore lint/a11y/useMediaCaption: Raw file previews do not have caption tracks.
            <video controls src={previewUrl}>
              {document.originalFileName}
            </video>
          ) : null}
        </div>
      </div>
    </div>
  )
}

function ConfirmDialog({
  cancelLabel,
  confirmLabel,
  isBusy,
  message,
  onCancel,
  onConfirm,
  title,
}: {
  cancelLabel: string
  confirmLabel: string
  isBusy: boolean
  message: string
  onCancel: () => void
  onConfirm: () => void
  title: string
}) {
  return (
    <div
      aria-modal="true"
      className="op-confirm-dialog"
      onClick={onCancel}
      role="dialog"
    >
      <div
        className="op-confirm-dialog__panel"
        onClick={(event) => event.stopPropagation()}
      >
        <h2>{title}</h2>
        <p>{message}</p>
        <div className="op-confirm-dialog__actions">
          <button
            className="op-wiki-command-button"
            disabled={isBusy}
            onClick={onCancel}
            type="button"
          >
            {cancelLabel}
          </button>
          <button
            className="op-wiki-command-button op-wiki-command-button--danger"
            disabled={isBusy}
            onClick={onConfirm}
            type="button"
          >
            <Trash2 size={15} />
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  )
}

function ProjectChrome({
  currentSession,
  sessions,
  onCreateProject,
  onDeleteProject,
  onRenameProject,
  onSwitchProject,
}: {
  currentSession: OpenPanelsSession
  onCreateProject: () => void
  onDeleteProject: (sessionId: string) => void
  onRenameProject: (title: string) => void
  onSwitchProject: (sessionId: string) => void
  sessions: OpenPanelsSession[]
}) {
  return (
    <>
      <CanvasMenu />
      <ProjectTitleControl
        currentSession={currentSession}
        onCreateProject={onCreateProject}
        onDeleteProject={onDeleteProject}
        onRenameProject={onRenameProject}
        onSwitchProject={onSwitchProject}
        sessions={sessions}
      />
    </>
  )
}

function BottomPanelTabs({
  activePanelKind,
  panels,
  onSwitchPanel,
}: {
  activePanelKind: OpenPanelsPanelKind
  onSwitchPanel: (kind: OpenPanelsPanelKind) => void
  panels: OpenPanelsPanel[]
}) {
  const { t } = useOpenPanelsI18n()
  const visiblePanels = panels.filter(
    (panel) => panel.kind === "wiki" || panel.kind === "canvas"
  )
  return (
    <div className="op-panel-tabs">
      <Tabs
        className="op-panel-tabs__tabs"
        onSelectionChange={(key) =>
          onSwitchPanel(String(key) as OpenPanelsPanelKind)
        }
        selectedKey={activePanelKind}
      >
        <Tabs.ListContainer>
          <Tabs.List
            aria-label={t`Project panels`}
            className="op-panel-tabs__list"
          >
            {visiblePanels.map((panel, index) => (
              <Tabs.Tab
                className="op-panel-tabs__tab"
                id={panel.kind}
                key={panel.id}
              >
                {index > 0 ? <Tabs.Separator /> : null}
                {panel.kind === "wiki" ? (
                  <FileText size={15} strokeWidth={1.8} />
                ) : (
                  <Palette size={15} strokeWidth={1.8} />
                )}
                <span>{panel.kind === "wiki" ? t`Wiki` : t`Canvas`}</span>
                <Tabs.Indicator />
              </Tabs.Tab>
            ))}
          </Tabs.List>
        </Tabs.ListContainer>
      </Tabs>
    </div>
  )
}

function ProjectTitleControl({
  currentSession,
  sessions,
  onCreateProject,
  onDeleteProject,
  onRenameProject,
  onSwitchProject,
}: {
  currentSession: OpenPanelsSession
  onCreateProject: () => void
  onDeleteProject: (sessionId: string) => void
  onRenameProject: (title: string) => void
  onSwitchProject: (sessionId: string) => void
  sessions: OpenPanelsSession[]
}) {
  const { t } = useOpenPanelsI18n()
  const [isMenuOpen, setIsMenuOpen] = useState(false)
  const [isEditing, setIsEditing] = useState(false)
  const [pendingDeleteSession, setPendingDeleteSession] =
    useState<OpenPanelsSession | null>(null)
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

  const confirmDeleteProject = useCallback(() => {
    if (!pendingDeleteSession) return
    setIsMenuOpen(false)
    onDeleteProject(pendingDeleteSession.id)
    setPendingDeleteSession(null)
  }, [onDeleteProject, pendingDeleteSession])

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
              const canDelete = sessions.length > 1
              return (
                <div
                  className={
                    isActive
                      ? "op-project-title__menu-item op-project-title__menu-item--active"
                      : "op-project-title__menu-item"
                  }
                  key={session.id}
                >
                  <button
                    className="op-project-title__switch-button"
                    onClick={() => {
                      setIsMenuOpen(false)
                      if (!isActive) {
                        onSwitchProject(session.id)
                      }
                    }}
                    type="button"
                  >
                    <span>{session.title}</span>
                  </button>
                  <span
                    className="op-project-title__delete-wrap"
                    title={
                      canDelete
                        ? t`Delete project`
                        : t`Keep at least one project`
                    }
                  >
                    <button
                      aria-disabled={!canDelete}
                      aria-label={t`Delete project`}
                      className="op-project-title__delete-button"
                      onClick={(event) => {
                        event.stopPropagation()
                        if (!canDelete) return
                        setIsMenuOpen(false)
                        setPendingDeleteSession(session)
                      }}
                      type="button"
                    >
                      <Trash2 size={14} strokeWidth={1.8} />
                    </button>
                  </span>
                </div>
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

      {pendingDeleteSession ? (
        <div
          aria-labelledby="op-delete-project-title"
          aria-modal="true"
          className="op-project-title__confirm-backdrop"
          role="dialog"
        >
          <div className="op-project-title__confirm">
            <div
              className="op-project-title__confirm-title"
              id="op-delete-project-title"
            >
              {t`Delete project?`}
            </div>
            <div className="op-project-title__confirm-copy">
              {t`This project and its canvas data will be deleted.`}
            </div>
            <div className="op-project-title__confirm-name">
              {pendingDeleteSession.title}
            </div>
            <div className="op-project-title__confirm-actions">
              <button
                className="op-project-title__confirm-button"
                onClick={() => setPendingDeleteSession(null)}
                type="button"
              >
                {t`Cancel`}
              </button>
              <button
                className="op-project-title__confirm-button op-project-title__confirm-button--danger"
                onClick={confirmDeleteProject}
                type="button"
              >
                {t`Delete`}
              </button>
            </div>
          </div>
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

function normalizeBootstrap(data: BootstrapResponse): AppState {
  const panels =
    data.panels?.map((snapshot) => ({
      panel: snapshot.panel,
      state: normalizePanelState(snapshot.panel.kind, snapshot.state),
    })) ?? []
  const activePanel =
    panels.find(({ panel }) => panel.id === data.activePanelId)?.panel ??
    data.panel
  const activeState = normalizePanelState(activePanel.kind, data.state)
  const normalizedPanels = panels.some(
    ({ panel }) => panel.id === activePanel.id
  )
    ? panels.map((snapshot) =>
        snapshot.panel.id === activePanel.id
          ? { panel: activePanel, state: activeState }
          : snapshot
      )
    : [{ panel: activePanel, state: activeState }, ...panels]

  return {
    ...data,
    activePanelId: activePanel.id,
    activePanelKind: activePanel.kind,
    panel: activePanel,
    panels: normalizedPanels,
    state: activeState,
  }
}

function normalizePanelState(
  kind: OpenPanelsPanelKind,
  state: unknown
): unknown {
  if (kind === "canvas") {
    return normalizeSnapshot(state as StoreSnapshot)
  }
  if (kind === "wiki") {
    return isWikiState(state) ? state : emptyWikiState()
  }
  return state ?? {}
}

function canvasSnapshotFromState(appState: AppState): StoreSnapshot | null {
  const snapshot = appState.panels.find(
    ({ panel }) => panel.kind === "canvas"
  )?.state
  return snapshot ? normalizeSnapshot(snapshot as StoreSnapshot) : null
}

function wikiStateFromAppState(appState: AppState): WikiState {
  const state = appState.panels.find(
    ({ panel }) => panel.kind === "wiki"
  )?.state
  return isWikiState(state) ? state : emptyWikiState()
}

function emptyWikiState(): WikiState {
  return {
    schemaVersion: 2,
    rawDocuments: [],
    ruleSets: [],
    wikiSpaces: [],
    activeRawDocumentId: null,
    activeWikiSpaceId: null,
    activeWikiPagePath: null,
    tasks: [],
  }
}

function isWikiState(state: unknown): state is WikiState {
  return (
    typeof state === "object" &&
    state !== null &&
    (state as { schemaVersion?: unknown }).schemaVersion === 2 &&
    Array.isArray((state as { rawDocuments?: unknown }).rawDocuments) &&
    Array.isArray((state as { wikiSpaces?: unknown }).wikiSpaces) &&
    Array.isArray((state as { tasks?: unknown }).tasks)
  )
}

function serializeBootstrapForCompare(appState: AppState): string {
  return JSON.stringify({
    activePanelId: appState.activePanelId,
    activePanelKind: appState.activePanelKind,
    panelIds: appState.panels.map(({ panel }) => panel.id),
    session: appState.session,
    states: appState.panels.map(({ panel, state }) => ({
      id: panel.id,
      kind: panel.kind,
      state:
        panel.kind === "canvas"
          ? serializeSnapshot(normalizeSnapshot(state as StoreSnapshot))
          : state,
    })),
  })
}

function normalizeSnapshot(snapshot: StoreSnapshot): StoreSnapshot {
  if (!snapshot || typeof snapshot !== "object") {
    return {
      schema: { schemaVersion: 1, recordVersions: {} },
      store: {},
      selectedShapeIds: new Set(),
      currentPageId: null,
      openedGroupId: null,
    } as StoreSnapshot
  }
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

function titleFromFileName(fileName: string): string {
  const lastSlash = Math.max(
    fileName.lastIndexOf("/"),
    fileName.lastIndexOf("\\")
  )
  const base = lastSlash === -1 ? fileName : fileName.slice(lastSlash + 1)
  const dot = base.lastIndexOf(".")
  return dot > 0 ? base.slice(0, dot) : base
}

function wikiRawOriginalUrl(
  apiBase: string,
  document: Pick<WikiRawDocument, "id">
): string {
  return apiUrl(
    apiBase,
    `/api/wiki/raw-documents/${encodeURIComponent(document.id)}/original`
  ).toString()
}

function originalPreviewKind(
  document: Pick<WikiRawDocument, "mimeType" | "originalFileName">
): OriginalPreviewKind | null {
  const mimeType = (document.mimeType ?? "").toLowerCase()
  const extension = extensionFromFileName(document.originalFileName ?? "")
  if (
    mimeType.startsWith("image/") ||
    [".gif", ".jpeg", ".jpg", ".png", ".svg", ".webp"].includes(extension)
  ) {
    return "image"
  }
  if (mimeType === "application/pdf" || extension === ".pdf") {
    return "pdf"
  }
  if (
    mimeType.startsWith("audio/") ||
    [".aac", ".m4a", ".mp3", ".wav"].includes(extension)
  ) {
    return "audio"
  }
  if (
    mimeType.startsWith("video/") ||
    [".avi", ".mov", ".mp4", ".webm"].includes(extension)
  ) {
    return "video"
  }
  return null
}

function extensionFromFileName(fileName: string): string {
  const base = fileName.split(/[\\/]/).at(-1) ?? fileName
  const dot = base.lastIndexOf(".")
  return dot >= 0 ? base.slice(dot).toLowerCase() : ""
}

function formatBytes(sizeBytes: number): string {
  if (!Number.isFinite(sizeBytes) || sizeBytes < 0) return ""
  if (sizeBytes < 1024) return `${sizeBytes} B`
  const units = ["KB", "MB", "GB", "TB"]
  let size = sizeBytes / 1024
  let unitIndex = 0
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024
    unitIndex += 1
  }
  return `${size >= 10 ? size.toFixed(0) : size.toFixed(1)} ${units[unitIndex]}`
}

function clampImageScale(scale: number): number {
  return Math.min(4, Math.max(0.25, scale))
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

async function loadBootstrap(
  transport: OpenPanelsTransport,
  sessionId?: string | null
): Promise<BootstrapResponse> {
  const url = apiUrl(transport.apiBase, "/api/bootstrap")
  if (sessionId) {
    url.searchParams.set("sessionId", sessionId)
  }
  const response = await apiFetch(transport.apiBase, url)
  return (await response.json()) as BootstrapResponse
}

async function savePanelState(
  transport: OpenPanelsTransport,
  sessionId: string,
  panelId: string,
  snapshot: StoreSnapshot
) {
  await apiFetch(
    transport.apiBase,
    `/api/panels/${encodeURIComponent(sessionId)}/${encodeURIComponent(panelId)}/state`,
    {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(serializeSnapshot(snapshot)),
    }
  )
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
  await apiFetch(
    transport.apiBase,
    `/api/panels/${encodeURIComponent(sessionId)}/${encodeURIComponent(panelId)}/selection`,
    {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload),
    }
  )
}

function localHttpOrigin(): string | null {
  if (window.location.protocol === "http:") {
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

function currentTransport(): OpenPanelsTransport | null {
  const localOrigin = localHttpOrigin()
  if (localOrigin) return { apiBase: localOrigin, kind: "http" }

  const serverUrl = hostServerUrl()
  if (serverUrl) return { apiBase: serverUrl, kind: "http" }

  return null
}

function transportKey(transport: OpenPanelsTransport | null): string {
  if (!transport) return "none"
  return `http:${transport.apiBase}`
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
  const response = await apiFetch(transport.apiBase, "/api/sessions")
  return (await response.json()) as OpenPanelsSession[]
}

async function fetchActiveSessionId(transport: OpenPanelsTransport) {
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
