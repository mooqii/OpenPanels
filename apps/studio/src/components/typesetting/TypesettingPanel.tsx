import {
  Button,
  Input,
  Label,
  ListBox,
  Modal,
  Popover,
  Select,
  Tabs,
  Tooltip,
} from "@heroui/react"
import Image from "@tiptap/extension-image"
import { Markdown } from "@tiptap/markdown"
import {
  EditorContent,
  NodeViewWrapper,
  ReactNodeViewRenderer,
  useEditor,
  useEditorState,
} from "@tiptap/react"
import StarterKit from "@tiptap/starter-kit"
import {
  AlertCircle,
  ArrowLeft,
  Bold,
  ChevronLeft,
  ChevronRight,
  FileText,
  GripVertical,
  Image as ImageIcon,
  Italic,
  Link as LinkIcon,
  List,
  ListOrdered,
  LoaderCircle,
  PanelLeft,
  Plus,
  Quote,
  Redo2,
  Trash2,
  Undo2,
  X,
} from "lucide-react"
import {
  type DragEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import {
  apiFetch,
  apiJson,
  apiUrl,
  formatBytes,
  isTypesettingState,
  originalPreviewKind,
  savePanelState,
  tryOpenBrowserWindow,
  wikiRawOriginalUrl,
} from "../../lib/api"
import { formatRelativeOrDate } from "../../lib/date-time"
import {
  countTypesettingCharacters,
  createTypesettingPublication,
  groupTypesettingAssets,
  isTypesettingDocumentEmpty,
  mergeTypesettingConflict,
  moveTypesettingCover,
  parseTypesettingAssetDrag,
  plainTextToTypesettingContent,
  TYPESETTING_ASSET_DRAG_TYPE,
  TYPESETTING_AUTOSAVE_DELAY_MS,
  typesettingInsertPosition,
  typesettingTitleAfterDocumentInsert,
} from "../../lib/typesetting"
import type {
  MyOpenPanelsTransport,
  TypesettingCanvasAsset,
  TypesettingPublication,
  TypesettingPublicationImage,
  TypesettingState,
  WikiGeneratedDocument,
  WikiRawDocument,
  WikiState,
} from "../../types"
import { ConfirmDialog } from "../wiki/Dialogs"
import {
  GeneratedDocumentsEmpty,
  RawDocumentsEmpty,
} from "../wiki/DocumentModuleEmpty"
import { GeneratedDocumentMeta } from "../wiki/GeneratedDocumentMeta"
import { RawDocumentMeta } from "../wiki/RawDocumentMeta"
import {
  nextCollapsedLibraryModules,
  type TypesettingLibraryModule,
} from "./library-accordion"

type SaveStatus = "saved" | "saving" | "failed"
type AssetScope = "current" | "all"
const TYPESETTING_COVER_DRAG_TYPE = "application/x-myopenpanels-cover-index"
type DocumentPreview =
  | {
      content: string | null
      document: WikiRawDocument
      error: string | null
      format: "markdown"
      kind: "raw"
      loading: boolean
      source: "markdown" | "original"
    }
  | {
      content: string | null
      document: WikiGeneratedDocument
      error: string | null
      format: "markdown" | "text"
      kind: "generated"
      loading: boolean
    }

interface ImportedTypesettingAsset {
  assetRef: string
  fileName: string
  mimeType: string
  sourceAssetRef: string
  sourceCanvasPanelId: string
  sourceProjectId: string
  src: string
}

export function TypesettingPanel({
  chromeContent,
  onStateSaved,
  panelId,
  projectId,
  revision,
  state: initialState,
  transport,
  wiki,
}: {
  chromeContent: ReactNode
  onStateSaved: (state: TypesettingState, revision: number) => void
  panelId: string
  projectId: string
  revision: number
  state: TypesettingState
  transport: MyOpenPanelsTransport
  wiki: WikiState
}) {
  const { t } = useMyOpenPanelsI18n()
  const [state, setState] = useState(initialState)
  const [view, setView] = useState<"list" | "detail">("list")
  const [activePublicationId, setActivePublicationId] = useState<string | null>(
    null
  )
  const [pendingDelete, setPendingDelete] =
    useState<TypesettingPublication | null>(null)
  const [preview, setPreview] = useState<DocumentPreview | null>(null)
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("saved")
  const [saveError, setSaveError] = useState<string | null>(null)
  const [saveGeneration, setSaveGeneration] = useState(0)
  const [isLibraryOpen, setIsLibraryOpen] = useState(false)
  const stateRef = useRef(state)
  const revisionRef = useRef(revision)
  const dirtyIdsRef = useRef(new Set<string>())
  const deletedIdsRef = useRef(new Set<string>())
  const changeGenerationRef = useRef(0)
  const saveInFlightRef = useRef<Promise<void> | null>(null)
  const flushRef = useRef<() => Promise<void>>(async () => undefined)
  const insertDocumentRef = useRef<
    | ((title: string, content: string, format: "markdown" | "text") => void)
    | null
  >(null)

  useEffect(() => {
    stateRef.current = state
  }, [state])

  useEffect(() => {
    if (
      revision <= revisionRef.current ||
      dirtyIdsRef.current.size > 0 ||
      deletedIdsRef.current.size > 0
    ) {
      return
    }
    revisionRef.current = revision
    stateRef.current = initialState
    setState(initialState)
  }, [initialState, revision])

  const replaceState = useCallback(
    (
      next: TypesettingState,
      publicationId: string,
      options?: { deleted?: boolean }
    ) => {
      stateRef.current = next
      setState(next)
      dirtyIdsRef.current.add(publicationId)
      if (options?.deleted) deletedIdsRef.current.add(publicationId)
      else deletedIdsRef.current.delete(publicationId)
      changeGenerationRef.current += 1
      setSaveGeneration(changeGenerationRef.current)
      setSaveStatus("saving")
      setSaveError(null)
    },
    []
  )

  const updatePublication = useCallback(
    (
      publicationId: string,
      updater: (publication: TypesettingPublication) => TypesettingPublication
    ) => {
      const current = stateRef.current
      const publications = current.publications.map((publication) =>
        publication.id === publicationId ? updater(publication) : publication
      )
      replaceState({ ...current, publications }, publicationId)
    },
    [replaceState]
  )

  const flushSave = useCallback(async () => {
    if (saveInFlightRef.current) {
      await saveInFlightRef.current
      if (dirtyIdsRef.current.size > 0 || deletedIdsRef.current.size > 0) {
        await flushRef.current()
      }
      return
    }
    if (dirtyIdsRef.current.size === 0 && deletedIdsRef.current.size === 0) {
      return
    }

    const save = (async () => {
      let payloadState = stateRef.current
      let generation = changeGenerationRef.current
      try {
        let saved: { revision: number }
        try {
          saved = await savePanelState(
            transport,
            projectId,
            panelId,
            payloadState,
            revisionRef.current
          )
        } catch (error) {
          if (!(error instanceof Error && error.message === "HTTP 409")) {
            throw error
          }
          const remote = await apiJson<{
            revision: number
            state: unknown
          }>(
            transport.apiBase,
            `/api/projects/${encodeURIComponent(projectId)}/panels/${encodeURIComponent(panelId)}/state`
          )
          if (!isTypesettingState(remote.state)) {
            throw new Error("Invalid remote Typesetting state")
          }
          payloadState = mergeTypesettingConflict({
            deletedIds: deletedIdsRef.current,
            dirtyIds: dirtyIdsRef.current,
            local: stateRef.current,
            remote: remote.state,
          })
          generation = changeGenerationRef.current
          stateRef.current = payloadState
          setState(payloadState)
          saved = await savePanelState(
            transport,
            projectId,
            panelId,
            payloadState,
            remote.revision
          )
        }

        revisionRef.current = saved.revision
        onStateSaved(payloadState, saved.revision)
        if (changeGenerationRef.current === generation) {
          dirtyIdsRef.current.clear()
          deletedIdsRef.current.clear()
          setSaveStatus("saved")
        }
      } catch (error) {
        setSaveStatus("failed")
        setSaveError(String(error instanceof Error ? error.message : error))
      } finally {
        saveInFlightRef.current = null
      }
    })()
    saveInFlightRef.current = save
    await save
  }, [onStateSaved, panelId, projectId, transport])

  useEffect(() => {
    flushRef.current = flushSave
  }, [flushSave])

  useEffect(() => {
    if (saveStatus !== "saving") return
    const timer = window.setTimeout(() => {
      if (saveGeneration !== changeGenerationRef.current) return
      flushSave().catch(() => undefined)
    }, TYPESETTING_AUTOSAVE_DELAY_MS)
    return () => window.clearTimeout(timer)
  }, [flushSave, saveGeneration, saveStatus])

  useEffect(
    () => () => {
      flushRef.current().catch(() => undefined)
    },
    []
  )

  const createPublication = useCallback(() => {
    const timestamp = new Date().toISOString()
    const publication = createTypesettingPublication(
      `publication:${crypto.randomUUID()}`,
      timestamp
    )
    const next = {
      ...stateRef.current,
      publications: [publication, ...stateRef.current.publications],
    }
    replaceState(next, publication.id)
    setActivePublicationId(publication.id)
    setView("detail")
  }, [replaceState])

  const deletePublication = useCallback(
    (publication: TypesettingPublication) => {
      const next = {
        ...stateRef.current,
        publications: stateRef.current.publications.filter(
          (candidate) => candidate.id !== publication.id
        ),
      }
      replaceState(next, publication.id, { deleted: true })
      setPendingDelete(null)
      if (activePublicationId === publication.id) {
        setActivePublicationId(null)
        setView("list")
      }
    },
    [activePublicationId, replaceState]
  )

  const importAsset = useCallback(
    async (
      asset: TypesettingCanvasAsset
    ): Promise<TypesettingPublicationImage> => {
      const imported = await apiJson<ImportedTypesettingAsset>(
        transport.apiBase,
        `/api/projects/${encodeURIComponent(projectId)}/panels/${encodeURIComponent(panelId)}/assets/import`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ sourceAssetRef: asset.assetRef }),
        }
      )
      return {
        assetRef: imported.assetRef,
        fileName: imported.fileName,
        height: asset.height,
        mimeType: imported.mimeType || asset.mimeType,
        sourceAssetRef: imported.sourceAssetRef,
        sourceCanvasPanelId: imported.sourceCanvasPanelId,
        sourceProjectId: imported.sourceProjectId,
        src: imported.src,
        width: asset.width,
      }
    },
    [panelId, projectId, transport.apiBase]
  )

  const openRawDocument = useCallback(
    async (
      document: WikiRawDocument,
      requestedSource: "preferred" | "original" = "preferred"
    ) => {
      const source =
        requestedSource === "preferred" && document.markdownRef
          ? "markdown"
          : "original"
      const initial: DocumentPreview = {
        content: null,
        document,
        error: null,
        format: "markdown",
        kind: "raw",
        loading:
          source === "markdown" ||
          (source === "original" && originalPreviewKind(document) === "text"),
        source,
      }
      setPreview(initial)
      if (source === "original") {
        if (originalPreviewKind(document) !== "text") return
        try {
          const response = await apiFetch(
            transport.apiBase,
            `/api/wiki/raw-documents/${encodeURIComponent(document.id)}/original`
          )
          if (!response.ok) throw new Error(`HTTP ${response.status}`)
          const content = await response.text()
          setPreview((current) =>
            current?.kind === "raw" &&
            current.document.id === document.id &&
            current.source === "original"
              ? { ...current, content, loading: false }
              : current
          )
        } catch (error) {
          setPreview((current) =>
            current?.kind === "raw" &&
            current.document.id === document.id &&
            current.source === "original"
              ? {
                  ...current,
                  error: String(error instanceof Error ? error.message : error),
                  loading: false,
                }
              : current
          )
        }
        return
      }
      try {
        const data = await apiJson<{ markdown?: string }>(
          transport.apiBase,
          `/api/wiki/raw-documents/${encodeURIComponent(document.id)}/markdown`
        )
        setPreview((current) =>
          current?.kind === "raw" && current.document.id === document.id
            ? { ...current, content: data.markdown ?? "", loading: false }
            : current
        )
      } catch (error) {
        setPreview((current) =>
          current?.kind === "raw" && current.document.id === document.id
            ? {
                ...current,
                error: String(error instanceof Error ? error.message : error),
                loading: false,
              }
            : current
        )
      }
    },
    [transport.apiBase]
  )

  const openRawOriginal = useCallback(
    (document: WikiRawDocument) => {
      if (originalPreviewKind(document)) {
        openRawDocument(document, "original").catch((error) => {
          console.error("Failed to preview raw document", error)
        })
        return
      }
      if (
        tryOpenBrowserWindow(wikiRawOriginalUrl(transport.apiBase, document))
      ) {
        return
      }
      apiFetch(
        transport.apiBase,
        `/api/wiki/raw-documents/${encodeURIComponent(document.id)}/reveal`,
        { method: "POST" }
      ).catch((error) => {
        console.error("Failed to reveal raw document", error)
      })
    },
    [openRawDocument, transport.apiBase]
  )

  const openGeneratedDocument = useCallback(
    async (document: WikiGeneratedDocument) => {
      setPreview({
        content: null,
        document,
        error: null,
        format: document.format,
        kind: "generated",
        loading: true,
      })
      try {
        const data = await apiJson<{ content?: string }>(
          transport.apiBase,
          `/api/wiki/generated-documents/${encodeURIComponent(document.id)}`
        )
        setPreview((current) =>
          current?.kind === "generated" && current.document.id === document.id
            ? { ...current, content: data.content ?? "", loading: false }
            : current
        )
      } catch (error) {
        setPreview((current) =>
          current?.kind === "generated" && current.document.id === document.id
            ? {
                ...current,
                error: String(error instanceof Error ? error.message : error),
                loading: false,
              }
            : current
        )
      }
    },
    [transport.apiBase]
  )

  const activePublication = state.publications.find(
    (publication) => publication.id === activePublicationId
  )

  return (
    <section className="op-typesetting-panel">
      <header className="op-canvas-title">{chromeContent}</header>
      <div className="op-typesetting-workbench">
        {isLibraryOpen ? (
          <button
            aria-label={t`Close library`}
            className="op-typesetting-library-backdrop"
            onClick={() => setIsLibraryOpen(false)}
            type="button"
          />
        ) : null}
        <TypesettingLibrary
          className={isLibraryOpen ? "is-open" : ""}
          onClose={() => setIsLibraryOpen(false)}
          onOpenGenerated={openGeneratedDocument}
          onOpenRaw={openRawDocument}
          onOpenRawOriginal={openRawOriginal}
          projectId={projectId}
          transport={transport}
          wiki={wiki}
        />
        <main className="op-typesetting-main">
          {view === "detail" && activePublication ? (
            <PublicationDetail
              importAsset={importAsset}
              key={activePublication.id}
              onBack={() => {
                setView("list")
                setActivePublicationId(null)
              }}
              onDelete={() => setPendingDelete(activePublication)}
              onInsertHandlerChange={(handler) => {
                insertDocumentRef.current = handler
              }}
              onOpenLibrary={() => setIsLibraryOpen(true)}
              onRetrySave={() => flushSave().catch(() => undefined)}
              onUpdate={(updater) =>
                updatePublication(activePublication.id, updater)
              }
              publication={activePublication}
              saveError={saveError}
              saveStatus={saveStatus}
              transport={transport}
            />
          ) : (
            <PublicationList
              onCreate={createPublication}
              onOpen={(publication) => {
                setActivePublicationId(publication.id)
                setView("detail")
              }}
              onOpenLibrary={() => setIsLibraryOpen(true)}
              onRetrySave={() => flushSave().catch(() => undefined)}
              publications={state.publications}
              saveError={saveError}
              saveStatus={saveStatus}
              transport={transport}
            />
          )}
        </main>
      </div>

      {preview ? (
        <DocumentPreviewDialog
          activePublication={Boolean(activePublication)}
          onClose={() => setPreview(null)}
          onInsert={() => {
            if (preview.content === null) return
            insertDocumentRef.current?.(
              preview.document.title,
              preview.content,
              preview.format
            )
            setPreview(null)
          }}
          preview={preview}
          transport={transport}
        />
      ) : null}

      {pendingDelete ? (
        <ConfirmDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Delete`}
          isBusy={false}
          message={t`This publication project and its layout content will be removed.`}
          onCancel={() => setPendingDelete(null)}
          onConfirm={() => deletePublication(pendingDelete)}
          title={t`Delete publication project?`}
        />
      ) : null}
    </section>
  )
}

function TypesettingLibrary({
  className,
  onClose,
  onOpenGenerated,
  onOpenRaw,
  onOpenRawOriginal,
  projectId,
  transport,
  wiki,
}: {
  className: string
  onClose: () => void
  onOpenGenerated: (document: WikiGeneratedDocument) => void
  onOpenRaw: (document: WikiRawDocument) => void
  onOpenRawOriginal: (document: WikiRawDocument) => void
  projectId: string
  transport: MyOpenPanelsTransport
  wiki: WikiState
}) {
  const { t } = useMyOpenPanelsI18n()
  const [scope, setScope] = useState<AssetScope>("current")
  const [assets, setAssets] = useState<TypesettingCanvasAsset[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [collapsedLibraryModules, setCollapsedLibraryModules] = useState<
    Set<TypesettingLibraryModule>
  >(() => new Set())

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    const url = new URL(
      apiUrl(transport.apiBase, "/api/typesetting/canvas-assets")
    )
    url.searchParams.set("projectId", projectId)
    url.searchParams.set("scope", scope)
    apiJson<{ assets?: TypesettingCanvasAsset[] }>(transport.apiBase, url)
      .then((data) => {
        if (!cancelled) setAssets(data.assets ?? [])
      })
      .catch((loadError) => {
        if (!cancelled) {
          setAssets([])
          setError(
            String(loadError instanceof Error ? loadError.message : loadError)
          )
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [projectId, scope, transport.apiBase])

  const groups = groupTypesettingAssets(assets)
  const toggleLibraryModule = (module: TypesettingLibraryModule) => {
    setCollapsedLibraryModules((current) =>
      nextCollapsedLibraryModules(current, module)
    )
  }

  return (
    <aside className={`op-typesetting-library ${className}`}>
      <div className="op-typesetting-library__mobile-header">
        <strong>{t`Documents and assets`}</strong>
        <Button
          aria-label={t`Close library`}
          isIconOnly
          onPress={onClose}
          size="sm"
          variant="ghost"
        >
          <X size={16} />
        </Button>
      </div>
      <div className="op-typesetting-document-library">
        <LibraryModule
          isCollapsed={collapsedLibraryModules.has("raw")}
          isEmpty={wiki.rawDocuments.length === 0}
          onToggle={() => toggleLibraryModule("raw")}
          title={t`Raw Documents`}
        >
          {wiki.rawDocuments.length ? (
            wiki.rawDocuments.map((document) => (
              <div className="op-typesetting-document" key={document.id}>
                <button
                  aria-label={document.title}
                  className="op-raw-document-open"
                  onClick={() => onOpenRaw(document)}
                  type="button"
                />
                <FileText size={15} />
                <span className="op-raw-document-copy">
                  <strong>{document.title}</strong>
                  <RawDocumentMeta
                    document={document}
                    onOpenOriginal={() => onOpenRawOriginal(document)}
                  />
                </span>
              </div>
            ))
          ) : (
            <RawDocumentsEmpty />
          )}
        </LibraryModule>

        <LibraryModule
          isCollapsed={collapsedLibraryModules.has("generated")}
          isEmpty={wiki.generatedDocuments.length === 0}
          onToggle={() => toggleLibraryModule("generated")}
          title={t`Generated Documents`}
        >
          {wiki.generatedDocuments.length ? (
            wiki.generatedDocuments.map((document) => (
              <button
                className="op-typesetting-document"
                key={document.id}
                onClick={() => onOpenGenerated(document)}
                type="button"
              >
                <FileText size={15} />
                <span>
                  <strong>{document.title}</strong>
                  <GeneratedDocumentMeta
                    apiBase={transport.apiBase}
                    document={document}
                  />
                </span>
              </button>
            ))
          ) : (
            <GeneratedDocumentsEmpty />
          )}
        </LibraryModule>

        <LibraryModule
          className="op-typesetting-assets-module"
          isCollapsed={collapsedLibraryModules.has("assets")}
          onToggle={() => toggleLibraryModule("assets")}
          title={t`Assets`}
        >
          <Tabs
            className="op-typesetting-assets-tabs"
            onSelectionChange={(key) => setScope(String(key) as AssetScope)}
            selectedKey={scope}
          >
            <Tabs.ListContainer>
              <Tabs.List aria-label={t`Asset scope`}>
                <Tabs.Tab id="current">
                  {t`Current project`}
                  <Tabs.Indicator />
                </Tabs.Tab>
                <Tabs.Tab id="all">
                  {t`All projects`}
                  <Tabs.Indicator />
                </Tabs.Tab>
              </Tabs.List>
            </Tabs.ListContainer>
          </Tabs>
          <div className="op-typesetting-assets">
            {loading ? (
              <LibraryEmpty>
                <LoaderCircle className="op-spin" size={16} />
                {t`Loading assets`}
              </LibraryEmpty>
            ) : error ? (
              <LibraryEmpty>{t`Failed to load assets`}</LibraryEmpty>
            ) : groups.length ? (
              groups.map((group) => (
                <section
                  className="op-typesetting-asset-group"
                  key={group.projectId}
                >
                  {scope === "all" ? <h4>{group.projectTitle}</h4> : null}
                  <div className="op-typesetting-asset-grid">
                    {group.assets.map((asset) => (
                      <button
                        className="op-typesetting-asset"
                        draggable
                        key={asset.id}
                        onDragStart={(event) => {
                          event.dataTransfer.effectAllowed = "copy"
                          event.dataTransfer.setData(
                            TYPESETTING_ASSET_DRAG_TYPE,
                            JSON.stringify(asset)
                          )
                          event.dataTransfer.setData(
                            "text/uri-list",
                            apiUrl(transport.apiBase, asset.src).toString()
                          )
                        }}
                        title={asset.name}
                        type="button"
                      >
                        <img
                          alt={asset.name}
                          draggable={false}
                          src={apiUrl(transport.apiBase, asset.src).toString()}
                        />
                      </button>
                    ))}
                  </div>
                </section>
              ))
            ) : (
              <LibraryEmpty>{t`No Canvas images yet`}</LibraryEmpty>
            )}
          </div>
        </LibraryModule>
      </div>
    </aside>
  )
}

function LibraryModule({
  children,
  className = "",
  isCollapsed,
  isEmpty = false,
  onToggle,
  title,
}: {
  children: ReactNode
  className?: string
  isCollapsed: boolean
  isEmpty?: boolean
  onToggle: () => void
  title: string
}) {
  return (
    <section
      className={
        isCollapsed
          ? `is-collapsed op-typesetting-library-module ${className}`.trim()
          : `op-typesetting-library-module ${className}`.trim()
      }
    >
      <button
        aria-expanded={!isCollapsed}
        className="op-typesetting-library-module__header"
        onClick={onToggle}
        type="button"
      >
        <h3>{title}</h3>
      </button>
      <div
        className={
          isEmpty
            ? "is-empty op-typesetting-library-module__content"
            : "op-typesetting-library-module__content"
        }
      >
        {children}
      </div>
    </section>
  )
}

function LibraryEmpty({ children }: { children: ReactNode }) {
  return <div className="op-typesetting-library-empty">{children}</div>
}

function PublicationList({
  onCreate,
  onOpen,
  onOpenLibrary,
  onRetrySave,
  publications,
  saveError,
  saveStatus,
  transport,
}: {
  onCreate: () => void
  onOpen: (publication: TypesettingPublication) => void
  onOpenLibrary: () => void
  onRetrySave: () => void
  publications: TypesettingPublication[]
  saveError: string | null
  saveStatus: SaveStatus
  transport: MyOpenPanelsTransport
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000)
    return () => window.clearInterval(timer)
  }, [])

  return (
    <div className="op-typesetting-list-view">
      <div className="op-typesetting-view-header">
        <Button
          aria-label={t`Open library`}
          className="op-typesetting-mobile-library-button"
          isIconOnly
          onPress={onOpenLibrary}
          size="sm"
          variant="ghost"
        >
          <PanelLeft size={17} />
        </Button>
        <div>
          <h1>{t`Publication content`}</h1>
          <p>{t`Manage titles, covers, and details for the current publication content.`}</p>
        </div>
        <SaveIndicator
          error={saveError}
          onRetry={onRetrySave}
          status={saveStatus}
        />
        <Button onPress={onCreate} size="sm" variant="primary">
          <Plus size={15} />
          {t`New`}
        </Button>
      </div>

      {publications.length ? (
        <div className="op-typesetting-publication-list">
          {publications.map((publication) => (
            <div
              className="op-typesetting-publication-row"
              key={publication.id}
            >
              <button
                className="op-typesetting-publication-row__body"
                onClick={() => onOpen(publication)}
                type="button"
              >
                <span className="op-typesetting-publication-row__cover">
                  {publication.covers[0] ? (
                    <img
                      alt=""
                      src={apiUrl(
                        transport.apiBase,
                        publication.covers[0].src
                      ).toString()}
                    />
                  ) : (
                    <ImageIcon size={18} />
                  )}
                </span>
                <span className="op-typesetting-publication-row__text">
                  <strong>
                    {publication.title.trim() || t`Untitled publication`}
                  </strong>
                  <small className="op-typesetting-publication-row__meta">
                    <span>
                      {publication.covers.length.toLocaleString(locale)}{" "}
                      {publication.covers.length === 1
                        ? t`cover image`
                        : t`cover images`}
                    </span>
                    <span>
                      {countTypesettingCharacters(
                        publication.content
                      ).toLocaleString(locale)}{" "}
                      {t`characters`}
                    </span>
                    <span>
                      {formatRelativeOrDate(publication.updatedAt, locale, now)}
                    </span>
                  </small>
                </span>
              </button>
            </div>
          ))}
        </div>
      ) : (
        <div className="op-typesetting-list-empty">
          {t`No publication projects yet`}
        </div>
      )}
    </div>
  )
}

function PublicationDetail({
  importAsset,
  onBack,
  onDelete,
  onInsertHandlerChange,
  onOpenLibrary,
  onRetrySave,
  onUpdate,
  publication,
  saveError,
  saveStatus,
  transport,
}: {
  importAsset: (
    asset: TypesettingCanvasAsset
  ) => Promise<TypesettingPublicationImage>
  onBack: () => void
  onDelete: () => void
  onInsertHandlerChange: (
    handler: (
      title: string,
      content: string,
      format: "markdown" | "text"
    ) => void
  ) => void
  onOpenLibrary: () => void
  onRetrySave: () => void
  onUpdate: (
    updater: (publication: TypesettingPublication) => TypesettingPublication
  ) => void
  publication: TypesettingPublication
  saveError: string | null
  saveStatus: SaveStatus
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const [assetError, setAssetError] = useState<string | null>(null)
  const [coverDropActive, setCoverDropActive] = useState(false)
  const [draggedCoverIndex, setDraggedCoverIndex] = useState<number | null>(
    null
  )
  const editorRef = useRef<ReturnType<typeof useEditor>>(null)
  const lastInsertPositionRef = useRef<number | null>(null)
  const publicationRef = useRef(publication)
  publicationRef.current = publication

  const imageExtension = useMemo(
    () => createTypesettingImageExtension(transport.apiBase),
    [transport.apiBase]
  )
  const extensions = useMemo(
    () => [
      StarterKit.configure({
        codeBlock: false,
        heading: { levels: [1, 2, 3] },
        horizontalRule: false,
        link: {
          autolink: true,
          defaultProtocol: "https",
          openOnClick: "whenNotEditable",
          protocols: ["http", "https", "mailto"],
        },
        strike: false,
        underline: false,
      }),
      imageExtension,
      Markdown,
    ],
    [imageExtension]
  )

  const editor = useEditor({
    content: publication.content,
    extensions,
    editorProps: {
      attributes: {
        class: "op-typesetting-editor__content",
      },
      handleDrop: (view, event) => {
        if (!event.dataTransfer) return false
        const asset = parseTypesettingAssetDrag(event.dataTransfer)
        if (!asset) return false
        event.preventDefault()
        const position = view.posAtCoords({
          left: event.clientX,
          top: event.clientY,
        })?.pos
        importAsset(asset)
          .then((image) => {
            const currentEditor = editorRef.current
            if (!currentEditor) return
            const target = typesettingInsertPosition(
              currentEditor.state.doc.content.size,
              position ?? null
            )
            currentEditor
              .chain()
              .focus()
              .insertContentAt(target, {
                type: "image",
                attrs: {
                  alt: image.fileName,
                  assetRef: image.assetRef,
                  height: image.height,
                  src: image.src,
                  title: image.fileName,
                  width: image.width,
                },
              })
              .run()
          })
          .catch((error) => {
            setAssetError(
              String(error instanceof Error ? error.message : error)
            )
          })
        return true
      },
    },
    onSelectionUpdate: ({ editor: currentEditor }) => {
      lastInsertPositionRef.current = currentEditor.state.selection.to
    },
    onUpdate: ({ editor: currentEditor }) => {
      onUpdate((current) => ({
        ...current,
        content: currentEditor.getJSON(),
        updatedAt: new Date().toISOString(),
      }))
    },
  })
  editorRef.current = editor

  const insertDocument = useCallback(
    (title: string, content: string, format: "markdown" | "text") => {
      if (!editor) return
      const position = typesettingInsertPosition(
        editor.state.doc.content.size,
        lastInsertPositionRef.current
      )
      if (!publicationRef.current.title.trim()) {
        onUpdate((current) => ({
          ...current,
          title: typesettingTitleAfterDocumentInsert(current.title, title),
          updatedAt: new Date().toISOString(),
        }))
      }
      const value =
        format === "markdown" ? content : plainTextToTypesettingContent(content)
      editor
        .chain()
        .focus()
        .insertContentAt(
          position,
          value,
          format === "markdown" ? { contentType: "markdown" } : undefined
        )
        .run()
      lastInsertPositionRef.current = editor.state.selection.to
    },
    [editor, onUpdate]
  )

  useEffect(() => {
    onInsertHandlerChange(insertDocument)
  }, [insertDocument, onInsertHandlerChange])

  const dropCover = useCallback(
    async (event: DragEvent<HTMLElement>) => {
      const asset = parseTypesettingAssetDrag(event.dataTransfer)
      if (!asset) return
      event.preventDefault()
      setCoverDropActive(false)
      setAssetError(null)
      try {
        const image = await importAsset(asset)
        onUpdate((current) => ({
          ...current,
          covers: [...current.covers, image],
          updatedAt: new Date().toISOString(),
        }))
      } catch (error) {
        setAssetError(String(error instanceof Error ? error.message : error))
      }
    },
    [importAsset, onUpdate]
  )

  return (
    <div className="op-typesetting-detail-view">
      <div className="op-typesetting-view-header op-typesetting-detail-header">
        <Button
          aria-label={t`Back to publication projects`}
          isIconOnly
          onPress={onBack}
          size="sm"
          variant="ghost"
        >
          <ArrowLeft size={17} />
        </Button>
        <Button
          aria-label={t`Open library`}
          className="op-typesetting-mobile-library-button"
          isIconOnly
          onPress={onOpenLibrary}
          size="sm"
          variant="ghost"
        >
          <PanelLeft size={17} />
        </Button>
        <div className="op-typesetting-detail-header__title">
          <strong>{publication.title.trim() || t`Untitled publication`}</strong>
          <small>{formatPublicationTime(publication.updatedAt)}</small>
        </div>
        <SaveIndicator
          error={saveError}
          onRetry={onRetrySave}
          status={saveStatus}
        />
        <Button
          aria-label={t`Delete publication project`}
          isIconOnly
          onPress={onDelete}
          size="sm"
          variant="ghost"
        >
          <Trash2 size={15} />
        </Button>
      </div>

      <div className="op-typesetting-detail-scroll">
        <div className="op-typesetting-field">
          <Label>{t`Title`}</Label>
          <Input
            aria-label={t`Title`}
            fullWidth
            onChange={(event) => {
              const title = event.currentTarget.value
              onUpdate((current) => ({
                ...current,
                title,
                updatedAt: new Date().toISOString(),
              }))
            }}
            placeholder={t`Untitled publication`}
            value={publication.title}
          />
        </div>

        <section className="op-typesetting-section">
          <div className="op-typesetting-section__heading">
            <div>
              <span>{t`Covers`}</span>
              <small>{t`The first image is used in the project list.`}</small>
            </div>
          </div>
          <div
            className={
              coverDropActive
                ? "is-active op-typesetting-cover-zone"
                : "op-typesetting-cover-zone"
            }
            onDragLeave={() => setCoverDropActive(false)}
            onDragOver={(event) => {
              if (
                !event.dataTransfer.types.includes(TYPESETTING_ASSET_DRAG_TYPE)
              ) {
                return
              }
              event.preventDefault()
              event.dataTransfer.dropEffect = "copy"
              setCoverDropActive(true)
            }}
            onDrop={(event) => {
              dropCover(event).catch(() => undefined)
            }}
          >
            {publication.covers.length ? (
              <div className="op-typesetting-covers">
                {publication.covers.map((cover, index) => (
                  <div
                    className="op-typesetting-cover"
                    draggable
                    key={cover.assetRef}
                    onDragEnd={() => setDraggedCoverIndex(null)}
                    onDragOver={(event) => {
                      if (
                        draggedCoverIndex === null ||
                        !event.dataTransfer.types.includes(
                          TYPESETTING_COVER_DRAG_TYPE
                        )
                      ) {
                        return
                      }
                      event.preventDefault()
                      event.dataTransfer.dropEffect = "move"
                    }}
                    onDragStart={(event) => {
                      setDraggedCoverIndex(index)
                      event.dataTransfer.effectAllowed = "move"
                      event.dataTransfer.setData(
                        TYPESETTING_COVER_DRAG_TYPE,
                        String(index)
                      )
                    }}
                    onDrop={(event) => {
                      const rawIndex = event.dataTransfer.getData(
                        TYPESETTING_COVER_DRAG_TYPE
                      )
                      if (!rawIndex) return
                      const from = Number(rawIndex)
                      if (!Number.isInteger(from)) return
                      event.preventDefault()
                      event.stopPropagation()
                      onUpdate((current) => ({
                        ...current,
                        covers: moveTypesettingCover(
                          current.covers,
                          from,
                          index
                        ),
                        updatedAt: new Date().toISOString(),
                      }))
                      setDraggedCoverIndex(null)
                    }}
                  >
                    <img
                      alt={cover.fileName}
                      draggable={false}
                      src={apiUrl(transport.apiBase, cover.src).toString()}
                    />
                    <span className="op-typesetting-cover__grip">
                      <GripVertical size={14} />
                    </span>
                    <div className="op-typesetting-cover__actions">
                      <Button
                        aria-label={t`Move cover left`}
                        isDisabled={index === 0}
                        isIconOnly
                        onPress={() =>
                          onUpdate((current) => ({
                            ...current,
                            covers: moveTypesettingCover(
                              current.covers,
                              index,
                              index - 1
                            ),
                            updatedAt: new Date().toISOString(),
                          }))
                        }
                        size="sm"
                        variant="ghost"
                      >
                        <ChevronLeft size={14} />
                      </Button>
                      <Button
                        aria-label={t`Move cover right`}
                        isDisabled={index === publication.covers.length - 1}
                        isIconOnly
                        onPress={() =>
                          onUpdate((current) => ({
                            ...current,
                            covers: moveTypesettingCover(
                              current.covers,
                              index,
                              index + 1
                            ),
                            updatedAt: new Date().toISOString(),
                          }))
                        }
                        size="sm"
                        variant="ghost"
                      >
                        <ChevronRight size={14} />
                      </Button>
                      <Button
                        aria-label={t`Remove cover`}
                        isIconOnly
                        onPress={() =>
                          onUpdate((current) => ({
                            ...current,
                            covers: current.covers.filter(
                              (candidate) =>
                                candidate.assetRef !== cover.assetRef
                            ),
                            updatedAt: new Date().toISOString(),
                          }))
                        }
                        size="sm"
                        variant="ghost"
                      >
                        <Trash2 size={14} />
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="op-typesetting-drop-empty">
                <span>{t`Drag images from the asset library to add covers.`}</span>
              </div>
            )}
          </div>
        </section>

        <section className="op-typesetting-section op-typesetting-content-section">
          <div className="op-typesetting-section__heading">
            <div>
              <span>{t`Content details`}</span>
              <small>{t`Rich text content is saved automatically.`}</small>
            </div>
          </div>
          <div className="op-typesetting-editor">
            <TypesettingToolbar editor={editor} />
            <div className="op-typesetting-editor__body">
              {editor && isTypesettingDocumentEmpty(editor.getJSON()) ? (
                <div className="op-typesetting-editor__empty">
                  <span>{t`Open a document from the library and insert it here.`}</span>
                </div>
              ) : null}
              <EditorContent editor={editor} />
            </div>
          </div>
        </section>
        {assetError ? (
          <div className="op-typesetting-inline-error" role="alert">
            <AlertCircle size={15} />
            <span>{assetError}</span>
            <Button
              aria-label={t`Dismiss`}
              isIconOnly
              onPress={() => setAssetError(null)}
              size="sm"
              variant="ghost"
            >
              <X size={14} />
            </Button>
          </div>
        ) : null}
      </div>
    </div>
  )
}

function TypesettingToolbar({
  editor,
}: {
  editor: ReturnType<typeof useEditor>
}) {
  const { t } = useMyOpenPanelsI18n()
  const [isLinkOpen, setIsLinkOpen] = useState(false)
  const [linkValue, setLinkValue] = useState("")
  const state = useEditorState({
    editor,
    selector: ({ editor: current }) => ({
      block: current?.isActive("heading", { level: 1 })
        ? "h1"
        : current?.isActive("heading", { level: 2 })
          ? "h2"
          : current?.isActive("heading", { level: 3 })
            ? "h3"
            : "p",
      bold: current?.isActive("bold") ?? false,
      bulletList: current?.isActive("bulletList") ?? false,
      canRedo: current?.can().redo() ?? false,
      canUndo: current?.can().undo() ?? false,
      italic: current?.isActive("italic") ?? false,
      link: current?.isActive("link") ?? false,
      orderedList: current?.isActive("orderedList") ?? false,
      quote: current?.isActive("blockquote") ?? false,
    }),
  })
  if (!editor) return <div className="op-typesetting-toolbar" />

  const applyLink = () => {
    const href = safeEditorLink(linkValue)
    if (!href) return
    editor.chain().focus().extendMarkRange("link").setLink({ href }).run()
    setIsLinkOpen(false)
  }

  return (
    <div className="op-typesetting-toolbar">
      <Select
        aria-label={t`Text style`}
        className="w-24 shrink-0"
        onChange={(key) => {
          const value = String(key)
          if (value === "p") editor.chain().focus().setParagraph().run()
          else {
            editor
              .chain()
              .focus()
              .toggleHeading({ level: Number(value.slice(1)) as 1 | 2 | 3 })
              .run()
          }
        }}
        selectionMode="single"
        value={state?.block ?? "p"}
        variant="secondary"
      >
        <Select.Trigger>
          <Select.Value />
          <Select.Indicator />
        </Select.Trigger>
        <Select.Popover>
          <ListBox>
            <ListBox.Item id="p" textValue={t`Paragraph`}>
              {t`Paragraph`}
            </ListBox.Item>
            <ListBox.Item id="h1" textValue="H1">
              H1
            </ListBox.Item>
            <ListBox.Item id="h2" textValue="H2">
              H2
            </ListBox.Item>
            <ListBox.Item id="h3" textValue="H3">
              H3
            </ListBox.Item>
          </ListBox>
        </Select.Popover>
      </Select>
      <ToolbarButton
        active={state?.bold}
        label={t`Bold`}
        onPress={() => editor.chain().focus().toggleBold().run()}
      >
        <Bold size={16} />
      </ToolbarButton>
      <ToolbarButton
        active={state?.italic}
        label={t`Italic`}
        onPress={() => editor.chain().focus().toggleItalic().run()}
      >
        <Italic size={16} />
      </ToolbarButton>
      <ToolbarButton
        active={state?.bulletList}
        label={t`Bullet list`}
        onPress={() => editor.chain().focus().toggleBulletList().run()}
      >
        <List size={16} />
      </ToolbarButton>
      <ToolbarButton
        active={state?.orderedList}
        label={t`Ordered list`}
        onPress={() => editor.chain().focus().toggleOrderedList().run()}
      >
        <ListOrdered size={16} />
      </ToolbarButton>
      <ToolbarButton
        active={state?.quote}
        label={t`Block quote`}
        onPress={() => editor.chain().focus().toggleBlockquote().run()}
      >
        <Quote size={16} />
      </ToolbarButton>
      <Popover
        isOpen={isLinkOpen}
        onOpenChange={(isOpen) => {
          setIsLinkOpen(isOpen)
          if (isOpen) {
            setLinkValue(editor.getAttributes("link").href ?? "")
          }
        }}
      >
        <ToolbarButton
          active={state?.link}
          label={t`Link`}
          onPress={() => undefined}
        >
          <LinkIcon size={16} />
        </ToolbarButton>
        <Popover.Content placement="bottom start">
          <Popover.Dialog className="w-[min(360px,calc(100vw-40px))]">
            <div className="flex items-center gap-2">
              <Input
                aria-label={t`Link URL`}
                autoFocus
                className="min-w-30 flex-1"
                onChange={(event) => setLinkValue(event.currentTarget.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") applyLink()
                }}
                placeholder="https://"
                value={linkValue}
              />
              <Button onPress={applyLink} size="sm" variant="primary">
                {t`Apply`}
              </Button>
              {state?.link ? (
                <Button
                  onPress={() => {
                    editor.chain().focus().unsetLink().run()
                    setIsLinkOpen(false)
                  }}
                  size="sm"
                  variant="ghost"
                >
                  {t`Remove`}
                </Button>
              ) : null}
            </div>
          </Popover.Dialog>
        </Popover.Content>
      </Popover>
      <span className="op-typesetting-toolbar__spacer" />
      <ToolbarButton
        disabled={!state?.canUndo}
        label={t`Undo`}
        onPress={() => editor.chain().focus().undo().run()}
      >
        <Undo2 size={16} />
      </ToolbarButton>
      <ToolbarButton
        disabled={!state?.canRedo}
        label={t`Redo`}
        onPress={() => editor.chain().focus().redo().run()}
      >
        <Redo2 size={16} />
      </ToolbarButton>
    </div>
  )
}

function ToolbarButton({
  active = false,
  children,
  disabled = false,
  label,
  onPress,
}: {
  active?: boolean
  children: ReactNode
  disabled?: boolean
  label: string
  onPress: () => void
}) {
  return (
    <Tooltip closeDelay={0} delay={300}>
      <Button
        aria-label={label}
        isDisabled={disabled}
        isIconOnly
        onPress={onPress}
        size="sm"
        variant={active ? "primary" : "ghost"}
      >
        {children}
      </Button>
      <Tooltip.Content placement="bottom">{label}</Tooltip.Content>
    </Tooltip>
  )
}

function SaveIndicator({
  error,
  onRetry,
  status,
}: {
  error: string | null
  onRetry: () => void
  status: SaveStatus
}) {
  const { t } = useMyOpenPanelsI18n()
  if (status === "saved") return null
  if (status === "failed") {
    return (
      <button
        className="op-typesetting-save op-typesetting-save--failed"
        onClick={onRetry}
        title={error ?? t`Retry save`}
        type="button"
      >
        <AlertCircle size={13} />
        {t`Save failed`}
      </button>
    )
  }
  return (
    <span className="op-typesetting-save op-typesetting-save--saving">
      <LoaderCircle className="op-spin" size={13} />
      {t`Saving`}
    </span>
  )
}

function DocumentPreviewDialog({
  activePublication,
  onClose,
  onInsert,
  preview,
  transport,
}: {
  activePublication: boolean
  onClose: () => void
  onInsert: () => void
  preview: DocumentPreview
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const rawPreviewKind =
    preview.kind === "raw" && preview.source === "original"
      ? originalPreviewKind(preview.document)
      : null
  const canInsert =
    activePublication &&
    !preview.loading &&
    !preview.error &&
    preview.content !== null

  return (
    <Modal.Backdrop isOpen onOpenChange={(open) => !open && onClose()}>
      <Modal.Container size="cover">
        <Modal.Dialog className="op-typesetting-preview">
          <Modal.CloseTrigger aria-label={t`Close`} />
          <Modal.Header>
            <div>
              <div className="op-wiki-panel__label">
                {preview.kind === "raw"
                  ? preview.source === "markdown"
                    ? t`Markdown`
                    : t`Original file`
                  : t`Generated Documents`}
              </div>
              <Modal.Heading>{preview.document.title}</Modal.Heading>
              {preview.kind === "raw" ? (
                <p>
                  {[
                    preview.document.originalFileName,
                    formatBytes(preview.document.sizeBytes),
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </p>
              ) : null}
              {preview.kind === "raw" && !preview.document.markdownRef ? (
                <p>{t`Convert this document to Markdown before inserting it.`}</p>
              ) : null}
            </div>
            <Button
              isDisabled={!canInsert}
              onPress={onInsert}
              size="sm"
              variant="primary"
            >
              {t`Insert into content details`}
            </Button>
          </Modal.Header>
          <Modal.Body>
            <div className="op-typesetting-preview__body">
              {preview.loading ? (
                <div className="op-typesetting-preview__status">
                  <LoaderCircle className="op-spin" size={18} />
                  {t`Loading document`}
                </div>
              ) : preview.error ? (
                <div className="op-typesetting-preview__status">
                  <AlertCircle size={18} />
                  {t`Failed to load document`}
                </div>
              ) : preview.kind === "raw" && rawPreviewKind === "text" ? (
                <pre>{preview.content ?? ""}</pre>
              ) : preview.kind === "raw" &&
                rawPreviewKind &&
                rawPreviewKind !== "text" ? (
                <RawDocumentMedia
                  document={preview.document}
                  kind={rawPreviewKind}
                  src={wikiRawOriginalUrl(transport.apiBase, preview.document)}
                />
              ) : preview.content !== null ? (
                <pre>{preview.content}</pre>
              ) : (
                <div className="op-typesetting-preview__status">
                  {preview.kind === "raw" && !preview.document.markdownRef
                    ? t`Convert this document to Markdown before inserting it.`
                    : t`Preview is not available for this file type`}
                </div>
              )}
            </div>
          </Modal.Body>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}

function RawDocumentMedia({
  document,
  kind,
  src,
}: {
  document: WikiRawDocument
  kind: Exclude<ReturnType<typeof originalPreviewKind>, "text" | null>
  src: string
}) {
  if (kind === "image") return <img alt={document.title} src={src} />
  if (kind === "pdf") return <iframe src={src} title={document.title} />
  if (kind === "audio") {
    return (
      // biome-ignore lint/a11y/useMediaCaption: Source documents do not include caption tracks.
      <audio controls src={src}>
        {document.originalFileName}
      </audio>
    )
  }
  return (
    // biome-ignore lint/a11y/useMediaCaption: Source documents do not include caption tracks.
    <video controls src={src}>
      {document.originalFileName}
    </video>
  )
}

function createTypesettingImageExtension(apiBase: string) {
  return Image.extend({
    addAttributes() {
      return {
        ...this.parent?.(),
        assetRef: {
          default: null,
          parseHTML: (element) => element.getAttribute("data-asset-ref"),
          renderHTML: (attributes) =>
            attributes.assetRef
              ? { "data-asset-ref": String(attributes.assetRef) }
              : {},
        },
      }
    },
    addNodeView() {
      return ReactNodeViewRenderer(({ node, selected }) => (
        <NodeViewWrapper
          className={
            selected
              ? "is-selected op-typesetting-editor-image"
              : "op-typesetting-editor-image"
          }
          contentEditable={false}
        >
          <img
            alt={node.attrs.alt ?? ""}
            src={
              typeof node.attrs.src === "string" &&
              node.attrs.src.startsWith("/")
                ? apiUrl(apiBase, node.attrs.src).toString()
                : node.attrs.src
            }
          />
        </NodeViewWrapper>
      ))
    },
  }).configure({
    allowBase64: false,
    inline: false,
    resize: {
      alwaysPreserveAspectRatio: true,
      enabled: true,
      minHeight: 80,
      minWidth: 80,
    },
  })
}

function safeEditorLink(value: string): string | null {
  const trimmed = value.trim()
  if (!trimmed) return null
  if (trimmed.startsWith("#")) return trimmed
  const candidate = /^[a-zA-Z][a-zA-Z\d+.-]*:/.test(trimmed)
    ? trimmed
    : `https://${trimmed}`
  try {
    const url = new URL(candidate)
    return ["http:", "https:", "mailto:"].includes(url.protocol)
      ? candidate
      : null
  } catch {
    return null
  }
}

function formatPublicationTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date)
}
