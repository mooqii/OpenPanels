import {
  Button,
  Checkbox,
  Dropdown,
  Header,
  Surface,
  Tabs,
  Tooltip,
} from "@heroui/react"
import {
  ChevronDown,
  ChevronRight,
  ExternalLink,
  Eye,
  FileOutput,
  FileText,
  Folder,
  FolderOpen,
  Info,
  MoreHorizontal,
  PanelLeft,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Send,
  Sparkles,
  Trash2,
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
  fileToDataUrl,
  originalPreviewKind,
  titleFromFileName,
  tryOpenBrowserWindow,
  wikiRawOriginalUrl,
} from "../../lib/api"
import {
  activeWritingSkillIds,
  toggleWritingSkillSelection,
  writingSkillSelectionError,
} from "../../lib/writing"
import type {
  AgentSkillListing,
  MyOpenPanelsTransport,
  ProjectTask,
  WikiGeneratedDocument,
  WikiRawDocument,
  WikiState,
  WritingState,
} from "../../types"
import {
  ConfirmDialog,
  GeneratedDocumentDialog,
  MarkdownDialog,
  OriginalPreviewDialog,
  RenameDocumentDialog,
} from "./Dialogs"
import {
  GeneratedDocumentsEmpty,
  RawDocumentsEmpty,
} from "./DocumentModuleEmpty"
import { GeneratedDocumentMeta } from "./GeneratedDocumentMeta"
import { documentIndexStatus, WikiIndexStatus, WikiStatus } from "./helpers"
import {
  nextCollapsedModules,
  serializeWikiCollapsedModules,
  WIKI_COLLAPSED_MODULES_STORAGE_KEY,
  type WikiModule,
  wikiCollapsedModulesFromStorage,
} from "./module-collapse"
import { buildWikiPageTree, type WikiPageTreeNode } from "./page-tree"
import { RawDocumentMeta } from "./RawDocumentMeta"
import { WikiPageMeta } from "./WikiPageMeta"

const DEFAULT_WIKI_AGENT_SKILL_ID = "karpathy-llm-wiki"
const DEFAULT_ZH_WIKI_AGENT_SKILL_ID = "karpathy-llm-wiki-zh"
interface WikiAgentSelection {
  isWikiSelected: boolean
  selectedGeneratedDocumentIds: string[]
  selectedRawDocumentIds: string[]
}

export function WikiPanel({
  chromeContent,
  onOpenAgentTasks,
  onReload,
  selectionVersion,
  state,
  transport,
  writing,
}: {
  chromeContent: ReactNode
  onOpenAgentTasks: (filter: "active" | "pending") => void
  onReload: () => Promise<void>
  selectionVersion: number
  state: WikiState
  transport: MyOpenPanelsTransport
  writing?: {
    state: WritingState
    tasks: ProjectTask[]
  }
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const selectionPath = writing
    ? "/api/writing/selection"
    : "/api/wiki/selection"
  const activeSpace =
    state.wikiSpaces.find((space) => space.id === state.activeWikiSpaceId) ??
    state.wikiSpaces[0]
  const wikiAgentSkillId = state.wikiAgentSkillId || DEFAULT_WIKI_AGENT_SKILL_ID
  const [agentSkills, setAgentSkills] = useState<AgentSkillListing[]>([])
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
  const [pendingDeleteGeneratedDocument, setPendingDeleteGeneratedDocument] =
    useState<WikiGeneratedDocument | null>(null)
  const [pendingRenameGeneratedDocument, setPendingRenameGeneratedDocument] =
    useState<WikiGeneratedDocument | null>(null)
  const [generatedDocumentDialog, setGeneratedDocumentDialog] = useState<{
    content: string
    document: WikiGeneratedDocument
  } | null>(null)
  const [originalPreviewDocument, setOriginalPreviewDocument] =
    useState<WikiRawDocument | null>(null)
  const [isBusy, setIsBusy] = useState(false)
  const [retryingGeneratedDocumentId, setRetryingGeneratedDocumentId] =
    useState<string | null>(null)
  const [generatedDocumentRetryError, setGeneratedDocumentRetryError] =
    useState<string | null>(null)
  const [isSelectionBusy, setIsSelectionBusy] = useState(true)
  const [agentSelection, setAgentSelection] = useState<WikiAgentSelection>({
    isWikiSelected: false,
    selectedGeneratedDocumentIds: [],
    selectedRawDocumentIds: [],
  })
  const [isRawDragActive, setIsRawDragActive] = useState(false)
  const [isDocumentLibraryOpen, setIsDocumentLibraryOpen] = useState(false)
  const [collapsedWikiFolders, setCollapsedWikiFolders] = useState<Set<string>>(
    () => new Set()
  )
  const [wikiCollapsedModules, setWikiCollapsedModules] = useState<
    Set<WikiModule>
  >(() =>
    wikiCollapsedModulesFromStorage(
      typeof window === "undefined"
        ? null
        : window.localStorage.getItem(WIKI_COLLAPSED_MODULES_STORAGE_KEY)
    )
  )
  const [writingCollapsedModules, setWritingCollapsedModules] = useState<
    Set<WikiModule>
  >(() => new Set())
  const collapsedModules = writing
    ? writingCollapsedModules
    : wikiCollapsedModules
  const rawDragDepthRef = useRef(0)
  const fileInputRef = useRef<HTMLInputElement | null>(null)
  const wikiPageTree = useMemo(
    () =>
      buildWikiPageTree(
        activeSpace?.pageIndex.length
          ? activeSpace.pageIndex
          : [
              {
                path: "index.md",
                summary: "",
                title: "Index",
                type: "overview",
                updatedAt: "",
              },
            ]
      ),
    [activeSpace?.pageIndex]
  )

  const toggleWikiFolder = useCallback((folderPath: string) => {
    setCollapsedWikiFolders((current) => {
      const next = new Set(current)
      if (next.has(folderPath)) next.delete(folderPath)
      else next.add(folderPath)
      return next
    })
  }, [])

  useEffect(() => {
    window.localStorage.setItem(
      WIKI_COLLAPSED_MODULES_STORAGE_KEY,
      serializeWikiCollapsedModules(wikiCollapsedModules)
    )
  }, [wikiCollapsedModules])

  const toggleModule = useCallback(
    (module: WikiModule) => {
      if (writing) {
        setWritingCollapsedModules((current) =>
          nextCollapsedModules(current, module, true)
        )
        return
      }
      setWikiCollapsedModules((current) =>
        nextCollapsedModules(current, module, false)
      )
    },
    [writing]
  )

  const moduleHeaderToggle = (module: WikiModule, title: string) => {
    const isCollapsed = collapsedModules.has(module)
    const isAccordionModule = writing
      ? module === "structured" || module === "raw"
      : module === "raw" || module === "generated"
    return (
      <button
        aria-expanded={!isCollapsed}
        aria-label={`${isCollapsed ? t`Expand module` : t`Collapse module`}: ${title}`}
        className={
          isAccordionModule
            ? "op-wiki-column__header-toggle op-wiki-column__header-toggle--accordion"
            : "op-wiki-column__header-toggle"
        }
        onClick={() => toggleModule(module)}
        type="button"
      />
    )
  }

  const moduleInfo = (label: string, description: string) => (
    <Tooltip closeDelay={0} delay={0}>
      <Button
        aria-label={`${t`About module`}: ${label}`}
        className="op-wiki-module-info"
        isIconOnly
        size="sm"
        variant="ghost"
      >
        <Info size={15} />
      </Button>
      <Tooltip.Content className="op-wiki-module-tooltip" placement="bottom">
        {description}
      </Tooltip.Content>
    </Tooltip>
  )

  const renderWikiPageNodes = (nodes: WikiPageTreeNode[]): ReactNode => (
    <div className="op-wiki-tree-list">
      {nodes.map((node) => {
        if (node.kind === "folder") {
          const isCollapsed = collapsedWikiFolders.has(node.path)
          return (
            <div className="op-wiki-tree-folder" key={node.path}>
              <button
                aria-expanded={!isCollapsed}
                aria-label={`${isCollapsed ? t`Expand folder` : t`Collapse folder`}: ${node.name}`}
                className="op-wiki-tree-folder__row"
                onClick={() => toggleWikiFolder(node.path)}
                type="button"
              >
                <ChevronRight
                  className="op-wiki-tree-folder__chevron"
                  data-expanded={!isCollapsed || undefined}
                  size={15}
                />
                {isCollapsed ? <Folder size={16} /> : <FolderOpen size={16} />}
                <strong>{node.name}</strong>
              </button>
              {!isCollapsed && (
                <div className="op-wiki-tree-children">
                  {renderWikiPageNodes(node.children)}
                </div>
              )}
            </div>
          )
        }

        return (
          <button
            className="op-wiki-page-row"
            key={node.page.path}
            onClick={() => openWikiPage(node.page.path)}
            type="button"
          >
            <span className="op-wiki-page-row__icon">
              <FileText size={16} />
            </span>
            <span className="op-wiki-page-row__body">
              <strong className="op-wiki-list-item__title">
                {node.fileName}
              </strong>
              <WikiPageMeta
                apiBase={transport.apiBase}
                page={node.page}
                wikiSpaceId={activeSpace?.id ?? "wiki:default"}
              />
            </span>
          </button>
        )
      })}
    </div>
  )

  useEffect(() => {
    let isCancelled = false
    apiFetch(transport.apiBase, "/api/agent/skills")
      .then(async (response) => {
        const data = (await response.json()) as { skills?: AgentSkillListing[] }
        if (!isCancelled) {
          setAgentSkills(
            (data.skills ?? []).filter(
              (item) =>
                item.skill.appliesTo.includes("wiki") &&
                item.skill.taskTypes.length > 0
            )
          )
        }
      })
      .catch((error) => {
        if (!isCancelled) {
          console.error("Failed to load wiki agent skills", error)
        }
      })
    return () => {
      isCancelled = true
    }
  }, [transport.apiBase])

  useEffect(() => {
    let isCancelled = false
    setIsSelectionBusy(true)
    apiFetch(transport.apiBase, `${selectionPath}?version=${selectionVersion}`)
      .then(async (response) => {
        const data = (await response.json()) as {
          selection?: Partial<WikiAgentSelection>
        }
        if (!isCancelled) {
          setAgentSelection({
            isWikiSelected: Boolean(data.selection?.isWikiSelected),
            selectedGeneratedDocumentIds:
              data.selection?.selectedGeneratedDocumentIds ?? [],
            selectedRawDocumentIds:
              data.selection?.selectedRawDocumentIds ?? [],
          })
        }
      })
      .catch((error) => {
        if (!isCancelled) {
          console.error("Failed to load Wiki agent selection", error)
        }
      })
      .finally(() => {
        if (!isCancelled) setIsSelectionBusy(false)
      })
    return () => {
      isCancelled = true
    }
  }, [selectionPath, selectionVersion, transport.apiBase])

  const updateAgentSelection = useCallback(
    async (next: WikiAgentSelection) => {
      const previous = agentSelection
      setAgentSelection(next)
      setIsSelectionBusy(true)
      try {
        const response = await apiFetch(transport.apiBase, selectionPath, {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(next),
        })
        const data = (await response.json()) as {
          selection?: Partial<WikiAgentSelection>
        }
        setAgentSelection({
          isWikiSelected: Boolean(data.selection?.isWikiSelected),
          selectedGeneratedDocumentIds:
            data.selection?.selectedGeneratedDocumentIds ?? [],
          selectedRawDocumentIds: data.selection?.selectedRawDocumentIds ?? [],
        })
      } catch (error) {
        setAgentSelection(previous)
        throw error
      } finally {
        setIsSelectionBusy(false)
      }
    },
    [agentSelection, selectionPath, transport.apiBase]
  )

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

  const openOriginalInNewWindow = useCallback(
    (document: WikiRawDocument) => {
      if (
        tryOpenBrowserWindow(wikiRawOriginalUrl(transport.apiBase, document))
      ) {
        return
      }
      revealOriginal(document).catch((error) => {
        console.error("Failed to reveal wiki raw document", error)
      })
    },
    [revealOriginal, transport.apiBase]
  )

  const openRawOriginal = useCallback(
    (document: WikiRawDocument) => {
      if (originalPreviewKind(document)) {
        setOriginalPreviewDocument(document)
        return
      }
      openOriginalInNewWindow(document)
    },
    [openOriginalInNewWindow]
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

  const updateWikiAgentSkill = useCallback(
    async (agentSkillId: string) => {
      setIsBusy(true)
      try {
        const response = await apiFetch(
          transport.apiBase,
          "/api/wiki/agent-skill",
          {
            method: "PUT",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ agentSkillId }),
          }
        )
        if (!response.ok) {
          if (response.status === 404) return
          throw new Error(
            `Failed to update wiki agent skill: ${response.status}`
          )
        }
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [onReload, transport.apiBase]
  )

  useEffect(() => {
    if (state.wikiAgentSkillConfigured || agentSkills.length === 0) return
    const defaultSkillId =
      locale === "zh-CN"
        ? DEFAULT_ZH_WIKI_AGENT_SKILL_ID
        : DEFAULT_WIKI_AGENT_SKILL_ID
    if (!agentSkills.some((item) => item.skill.id === defaultSkillId)) return
    updateWikiAgentSkill(defaultSkillId).catch((error) => {
      console.error("Failed to set the locale-aware Wiki agent skill", error)
    })
  }, [
    agentSkills,
    locale,
    state.wikiAgentSkillConfigured,
    updateWikiAgentSkill,
  ])

  const openGeneratedDocument = useCallback(
    async (document: WikiGeneratedDocument) => {
      const response = await apiFetch(
        transport.apiBase,
        `/api/wiki/generated-documents/${encodeURIComponent(document.id)}`
      )
      const data = (await response.json()) as { content?: string }
      setGeneratedDocumentDialog({
        content: data.content ?? "",
        document,
      })
    },
    [transport.apiBase]
  )

  const publishGeneratedDocument = useCallback(
    async (document: WikiGeneratedDocument) => {
      setIsBusy(true)
      try {
        await apiFetch(
          transport.apiBase,
          `/api/wiki/generated-documents/${encodeURIComponent(document.id)}/publish`,
          {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ wikiSpaceId: activeSpace?.id }),
          }
        )
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [activeSpace?.id, onReload, transport.apiBase]
  )

  const renameGeneratedDocument = useCallback(
    async (document: WikiGeneratedDocument, title: string) => {
      setIsBusy(true)
      try {
        await apiFetch(
          transport.apiBase,
          `/api/wiki/generated-documents/${encodeURIComponent(document.id)}`,
          {
            method: "PUT",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ title }),
          }
        )
        setPendingRenameGeneratedDocument(null)
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [onReload, transport.apiBase]
  )

  const deleteGeneratedDocument = useCallback(
    async (document: WikiGeneratedDocument) => {
      setIsBusy(true)
      try {
        await apiFetch(
          transport.apiBase,
          `/api/wiki/generated-documents/${encodeURIComponent(document.id)}`,
          { method: "DELETE" }
        )
        setPendingDeleteGeneratedDocument(null)
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [onReload, transport.apiBase]
  )

  const retryGeneratedDocument = useCallback(
    async (document: WikiGeneratedDocument) => {
      setRetryingGeneratedDocumentId(document.id)
      setGeneratedDocumentRetryError(null)
      try {
        await apiJson(
          transport.apiBase,
          `/api/wiki/generated-documents/${encodeURIComponent(document.id)}/retry`,
          { method: "POST" }
        )
        await onReload()
      } catch (error) {
        console.error("Failed to retry generated document", error)
        setGeneratedDocumentRetryError(document.id)
      } finally {
        setRetryingGeneratedDocumentId(null)
      }
    },
    [onReload, transport.apiBase]
  )

  return (
    <section
      className={writing ? "op-wiki-panel op-writing-panel" : "op-wiki-panel"}
    >
      <header className="op-canvas-title">{chromeContent}</header>
      <Surface className="op-wiki-panel__surface" variant="default">
        <div
          className={
            writing
              ? "op-wiki-workbench op-writing-workbench"
              : "op-wiki-workbench"
          }
        >
          {(() => {
            const rawDocumentsModule = (
              <aside
                className={
                  collapsedModules.has("raw")
                    ? "op-wiki-column op-wiki-column--raw op-wiki-column--collapsed"
                    : isRawDragActive
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
                  {moduleHeaderToggle("raw", t`Raw Documents`)}
                  <div className="op-wiki-column__title">
                    <h2>{t`Raw Documents`}</h2>
                    {moduleInfo(
                      t`Raw Documents`,
                      t`Source files live here. Added content is converted to Markdown and indexed into the Wiki. Selecting a document lets the agent discover it and load its content when needed.`
                    )}
                  </div>
                  <div className="op-wiki-actions">
                    <Button
                      aria-label={t`Upload document`}
                      className="op-wiki-add-button"
                      isDisabled={isBusy}
                      isIconOnly
                      onPress={() => fileInputRef.current?.click()}
                      size="sm"
                      variant="ghost"
                    >
                      <Plus size={16} />
                    </Button>
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
                <div
                  className={
                    state.rawDocuments.length === 0
                      ? "op-wiki-column__content op-wiki-list op-wiki-list--empty"
                      : "op-wiki-column__content op-wiki-list"
                  }
                >
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
                          className="op-wiki-list-item op-wiki-list-item--interactive"
                          key={document.id}
                        >
                          <Checkbox
                            aria-label={`${t`Select for agent context`}: ${document.title}`}
                            className="op-wiki-selection-checkbox op-wiki-selection-checkbox--document"
                            isDisabled={isSelectionBusy}
                            isSelected={agentSelection.selectedRawDocumentIds.includes(
                              document.id
                            )}
                            onChange={(isSelected) => {
                              const selectedRawDocumentIds = isSelected
                                ? [
                                    ...agentSelection.selectedRawDocumentIds,
                                    document.id,
                                  ]
                                : agentSelection.selectedRawDocumentIds.filter(
                                    (documentId) => documentId !== document.id
                                  )
                              updateAgentSelection({
                                ...agentSelection,
                                selectedRawDocumentIds,
                              }).catch((error) => {
                                console.error(
                                  "Failed to update raw document selection",
                                  error
                                )
                              })
                            }}
                            variant="secondary"
                          >
                            <Checkbox.Content>
                              <Checkbox.Control>
                                <Checkbox.Indicator />
                              </Checkbox.Control>
                            </Checkbox.Content>
                          </Checkbox>
                          <div className="op-wiki-list-item__body">
                            <button
                              aria-label={document.title}
                              className="op-raw-document-open"
                              onClick={() => {
                                if (hasMarkdown) {
                                  openMarkdown(document).catch((error) => {
                                    console.error(
                                      "Failed to open wiki markdown",
                                      error
                                    )
                                  })
                                  return
                                }
                                openRawOriginal(document)
                              }}
                              type="button"
                            />
                            <div className="op-raw-document-copy">
                              <strong className="op-wiki-list-item__title">
                                {document.title}
                              </strong>
                              <div className="op-raw-document-subtitle">
                                <RawDocumentMeta
                                  document={document}
                                  onOpenOriginal={() =>
                                    openRawOriginal(document)
                                  }
                                />
                                {hasMarkdown && indexStatus.kind !== "done" ? (
                                  <WikiIndexStatus
                                    onOpenTasks={onOpenAgentTasks}
                                    status={indexStatus}
                                  />
                                ) : null}
                                <WikiStatus
                                  document={document}
                                  onOpenTasks={onOpenAgentTasks}
                                />
                              </div>
                            </div>
                          </div>
                          <div className="op-wiki-list-item__tools">
                            <Dropdown>
                              <Dropdown.Trigger>
                                <Button
                                  aria-label={t`Document actions`}
                                  isIconOnly
                                  size="sm"
                                  variant="ghost"
                                >
                                  <MoreHorizontal size={16} />
                                </Button>
                              </Dropdown.Trigger>
                              <Dropdown.Popover>
                                <Dropdown.Menu
                                  disabledKeys={[
                                    ...(isBusy
                                      ? [
                                          "preview",
                                          "open",
                                          "reveal",
                                          "sync",
                                          "delete",
                                        ]
                                      : []),
                                    ...(previewKind ? [] : ["preview"]),
                                  ]}
                                  onAction={(key) => {
                                    switch (key) {
                                      case "preview":
                                        setOriginalPreviewDocument(document)
                                        break
                                      case "open":
                                        openOriginalInNewWindow(document)
                                        break
                                      case "reveal":
                                        revealOriginal(document).catch(
                                          (error) => {
                                            console.error(
                                              "Failed to reveal wiki raw document",
                                              error
                                            )
                                          }
                                        )
                                        break
                                      case "sync":
                                        ;(hasMarkdown
                                          ? reindexDocument(document)
                                          : extractMarkdown(document)
                                        ).catch((error) => {
                                          console.error(
                                            hasMarkdown
                                              ? "Failed to reindex wiki document"
                                              : "Failed to extract wiki raw document",
                                            error
                                          )
                                        })
                                        break
                                      case "delete":
                                        setPendingDeleteDocument(document)
                                        break
                                      default:
                                        break
                                    }
                                  }}
                                >
                                  <Dropdown.Item id="preview">
                                    <Eye size={14} />
                                    <span>{t`Preview original file`}</span>
                                  </Dropdown.Item>
                                  <Dropdown.Item id="open">
                                    <ExternalLink size={14} />
                                    <span>{t`Open in new window`}</span>
                                  </Dropdown.Item>
                                  <Dropdown.Item id="reveal">
                                    <FolderOpen size={14} />
                                    <span>{t`Show in folder`}</span>
                                  </Dropdown.Item>
                                  <Dropdown.Item id="sync">
                                    <RefreshCw size={14} />
                                    <span>
                                      {hasMarkdown ? t`Reindex` : t`Re-extract`}
                                    </span>
                                  </Dropdown.Item>
                                  <Dropdown.Item
                                    className="text-danger"
                                    id="delete"
                                  >
                                    <Trash2 size={14} />
                                    <span>{t`Delete`}</span>
                                  </Dropdown.Item>
                                </Dropdown.Menu>
                              </Dropdown.Popover>
                            </Dropdown>
                          </div>
                        </div>
                      )
                    })
                  ) : (
                    <RawDocumentsEmpty />
                  )}
                </div>
              </aside>
            )
            const generatedDocumentsModule = (
              <section
                className={
                  collapsedModules.has("generated")
                    ? "op-wiki-column--collapsed op-wiki-column op-wiki-column--generated"
                    : "op-wiki-column op-wiki-column--generated"
                }
              >
                <div className="op-wiki-column__header">
                  {moduleHeaderToggle("generated", t`Generated Documents`)}
                  <div className="op-wiki-column__title">
                    <h2>{t`Generated Documents`}</h2>
                    {moduleInfo(
                      t`Generated Documents`,
                      t`Drafts created by agents live here before they become source material. Agents can create and edit these documents. Selecting a document lets the agent discover it and load its latest content when needed.`
                    )}
                  </div>
                </div>
                <div
                  className={
                    state.generatedDocuments.length === 0
                      ? "op-wiki-list op-wiki-column__content op-wiki-list--empty"
                      : "op-wiki-list op-wiki-column__content"
                  }
                >
                  {state.generatedDocuments.length ? (
                    state.generatedDocuments.map((document) => {
                      const isGenerating =
                        document.generation?.status === "generating"
                      const generationFailed =
                        document.generation?.status === "failed"
                      return (
                        <div
                          className="op-wiki-list-item op-wiki-list-item--interactive"
                          key={document.id}
                        >
                          <Checkbox
                            aria-label={`${t`Select for agent context`}: ${document.title}`}
                            className="op-wiki-selection-checkbox op-wiki-selection-checkbox--document"
                            isDisabled={isSelectionBusy}
                            isSelected={agentSelection.selectedGeneratedDocumentIds.includes(
                              document.id
                            )}
                            onChange={(isSelected) => {
                              const selectedGeneratedDocumentIds = isSelected
                                ? [
                                    ...agentSelection.selectedGeneratedDocumentIds,
                                    document.id,
                                  ]
                                : agentSelection.selectedGeneratedDocumentIds.filter(
                                    (documentId) => documentId !== document.id
                                  )
                              updateAgentSelection({
                                ...agentSelection,
                                selectedGeneratedDocumentIds,
                              }).catch((error) => {
                                console.error(
                                  "Failed to update generated document selection",
                                  error
                                )
                              })
                            }}
                            variant="secondary"
                          >
                            <Checkbox.Content>
                              <Checkbox.Control>
                                <Checkbox.Indicator />
                              </Checkbox.Control>
                            </Checkbox.Content>
                          </Checkbox>
                          <button
                            className="op-wiki-list-item__body"
                            disabled={isGenerating}
                            onClick={() => {
                              openGeneratedDocument(document).catch((error) => {
                                console.error(
                                  "Failed to open generated document",
                                  error
                                )
                              })
                            }}
                            type="button"
                          >
                            <div>
                              <strong className="op-wiki-list-item__title">
                                {document.title}
                              </strong>
                              <GeneratedDocumentMeta
                                apiBase={transport.apiBase}
                                document={document}
                              />
                            </div>
                          </button>
                          <div className="op-wiki-list-item__tools">
                            {isGenerating ? (
                              <span className="op-generated-document-status">
                                <RefreshCw className="op-wiki-spin" size={14} />
                                {t`Generating`}
                              </span>
                            ) : null}
                            {generationFailed ? (
                              <Tooltip closeDelay={0} delay={0}>
                                <Button
                                  aria-label={t`Generation failed. Click to retry`}
                                  className="op-generated-document-retry"
                                  isIconOnly
                                  onPress={() =>
                                    retryGeneratedDocument(document)
                                  }
                                  size="sm"
                                  variant="secondary"
                                >
                                  {retryingGeneratedDocumentId ===
                                  document.id ? (
                                    <RefreshCw
                                      className="op-wiki-spin"
                                      size={14}
                                    />
                                  ) : (
                                    <RotateCcw size={14} />
                                  )}
                                </Button>
                                <Tooltip.Content
                                  placement="top"
                                  shouldFlip={false}
                                >
                                  {generatedDocumentRetryError === document.id
                                    ? t`Retry failed. Ask the Agent to generate it again.`
                                    : t`Generation failed. Click to retry`}
                                </Tooltip.Content>
                              </Tooltip>
                            ) : null}
                            <Dropdown>
                              <Dropdown.Trigger>
                                <Button
                                  aria-label={t`Document actions`}
                                  isIconOnly
                                  size="sm"
                                  variant="ghost"
                                >
                                  <MoreHorizontal size={16} />
                                </Button>
                              </Dropdown.Trigger>
                              <Dropdown.Popover>
                                <Dropdown.Menu
                                  disabledKeys={[
                                    ...(isBusy
                                      ? ["publish", "rename", "delete"]
                                      : []),
                                  ]}
                                  onAction={(key) => {
                                    if (key === "publish") {
                                      publishGeneratedDocument(document).catch(
                                        (error) => {
                                          console.error(
                                            "Failed to publish generated document",
                                            error
                                          )
                                        }
                                      )
                                    } else if (key === "rename") {
                                      setPendingRenameGeneratedDocument(
                                        document
                                      )
                                    } else if (key === "delete") {
                                      setPendingDeleteGeneratedDocument(
                                        document
                                      )
                                    }
                                  }}
                                >
                                  <Dropdown.Item id="publish">
                                    <FileOutput size={14} />
                                    <span>
                                      {document.publishHistory.length
                                        ? t`Add latest version to raw documents`
                                        : t`Add to raw documents`}
                                    </span>
                                  </Dropdown.Item>
                                  <Dropdown.Item id="rename">
                                    <Pencil size={14} />
                                    <span>{t`Rename`}</span>
                                  </Dropdown.Item>
                                  <Dropdown.Item
                                    className="text-danger"
                                    id="delete"
                                  >
                                    <Trash2 size={14} />
                                    <span>{t`Delete`}</span>
                                  </Dropdown.Item>
                                </Dropdown.Menu>
                              </Dropdown.Popover>
                            </Dropdown>
                          </div>
                        </div>
                      )
                    })
                  ) : (
                    <GeneratedDocumentsEmpty />
                  )}
                </div>
              </section>
            )
            const structuredWikiModule = (
              <section
                className={
                  collapsedModules.has("structured")
                    ? "op-wiki-column--collapsed op-wiki-column op-wiki-column--structured"
                    : "op-wiki-column op-wiki-column--structured"
                }
              >
                <div className="op-wiki-column__header">
                  {moduleHeaderToggle(
                    "structured",
                    activeSpace?.title || t`Wiki`
                  )}
                  <div className="op-wiki-column__title">
                    {writing ? null : (
                      <Button
                        aria-label={t`Open document library`}
                        className="op-wiki-mobile-library-button"
                        isIconOnly
                        onPress={() => setIsDocumentLibraryOpen(true)}
                        size="sm"
                        variant="ghost"
                      >
                        <PanelLeft size={17} />
                      </Button>
                    )}
                    <Checkbox
                      aria-label={t`Select Wiki for agent context`}
                      className="op-wiki-selection-checkbox"
                      isDisabled={isSelectionBusy}
                      isSelected={agentSelection.isWikiSelected}
                      onChange={(isWikiSelected) => {
                        updateAgentSelection({
                          ...agentSelection,
                          isWikiSelected,
                        }).catch((error) => {
                          console.error(
                            "Failed to update Wiki selection",
                            error
                          )
                        })
                      }}
                      variant="secondary"
                    >
                      <Checkbox.Content>
                        <Checkbox.Control>
                          <Checkbox.Indicator />
                        </Checkbox.Control>
                      </Checkbox.Content>
                    </Checkbox>
                    <h2>
                      {activeSpace?.title ? t(activeSpace.title) : t`Wiki`}
                    </h2>
                    {moduleInfo(
                      activeSpace?.title || t`Wiki`,
                      t`Structured knowledge pages generated from your sources live here. Agents can search and update this Wiki. Selecting it lets the agent discover the Wiki and load relevant pages when needed.`
                    )}
                  </div>
                </div>
                <div className="op-wiki-page-tree op-wiki-column__content">
                  {renderWikiPageNodes(wikiPageTree)}
                </div>
                <div className="op-wiki-agent-skill-footer">
                  <Dropdown>
                    <Dropdown.Trigger>
                      <Button
                        className="op-wiki-agent-skill-trigger"
                        isDisabled={isBusy || agentSkills.length === 0}
                        size="sm"
                        variant="ghost"
                      >
                        <span>
                          {agentSkills.find(
                            (item) => item.skill.id === wikiAgentSkillId
                          )?.skill.title ?? wikiAgentSkillId}
                        </span>
                        <ChevronDown size={14} />
                      </Button>
                    </Dropdown.Trigger>
                    <Dropdown.Popover
                      className="op-wiki-agent-skill-popover"
                      placement="top start"
                      shouldFlip={false}
                    >
                      <Dropdown.Menu
                        aria-label={t`Wiki generation method`}
                        onAction={(key) => {
                          updateWikiAgentSkill(String(key)).catch(
                            () => undefined
                          )
                        }}
                        selectedKeys={[wikiAgentSkillId]}
                        selectionMode="single"
                      >
                        <Dropdown.Section>
                          <Header className="op-wiki-agent-skill-menu-header">
                            {t`Wiki generation method`}
                          </Header>
                          {agentSkills.map((item) => (
                            <Dropdown.Item
                              id={item.skill.id}
                              key={item.skill.id}
                            >
                              {item.skill.title}
                            </Dropdown.Item>
                          ))}
                        </Dropdown.Section>
                      </Dropdown.Menu>
                    </Dropdown.Popover>
                  </Dropdown>
                </div>
              </section>
            )

            if (writing) {
              return (
                <>
                  {isDocumentLibraryOpen ? (
                    <button
                      aria-label={t`Close document library`}
                      className="op-writing-source-library-backdrop"
                      onClick={() => setIsDocumentLibraryOpen(false)}
                      type="button"
                    />
                  ) : null}
                  <div
                    className={
                      isDocumentLibraryOpen
                        ? "is-open op-writing-source-library"
                        : "op-writing-source-library"
                    }
                  >
                    <div className="op-writing-source-library__mobile-header">
                      <strong>{t`Document library`}</strong>
                      <Button
                        aria-label={t`Close document library`}
                        isIconOnly
                        onPress={() => setIsDocumentLibraryOpen(false)}
                        size="sm"
                        variant="ghost"
                      >
                        <X size={17} />
                      </Button>
                    </div>
                    {structuredWikiModule}
                    {rawDocumentsModule}
                  </div>
                  {generatedDocumentsModule}
                </>
              )
            }

            return (
              <>
                {isDocumentLibraryOpen ? (
                  <button
                    aria-label={t`Close document library`}
                    className="op-wiki-document-library-backdrop"
                    onClick={() => setIsDocumentLibraryOpen(false)}
                    type="button"
                  />
                ) : null}
                <div
                  className={
                    isDocumentLibraryOpen
                      ? "is-open op-wiki-document-library"
                      : "op-wiki-document-library"
                  }
                >
                  <div className="op-wiki-document-library__mobile-header">
                    <strong>{t`Document library`}</strong>
                    <Button
                      aria-label={t`Close document library`}
                      isIconOnly
                      onPress={() => setIsDocumentLibraryOpen(false)}
                      size="sm"
                      variant="ghost"
                    >
                      <X size={17} />
                    </Button>
                  </div>
                  {rawDocumentsModule}
                  {generatedDocumentsModule}
                </div>
                {structuredWikiModule}
              </>
            )
          })()}

          {writing ? (
            <WritingComposer
              documents={state.generatedDocuments}
              isSelectionBusy={isSelectionBusy}
              onOpenLibrary={() => setIsDocumentLibraryOpen(true)}
              onReload={onReload}
              rawDocuments={state.rawDocuments}
              selection={agentSelection}
              state={writing.state}
              tasks={writing.tasks}
              transport={transport}
            />
          ) : null}
        </div>
      </Surface>

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

      {generatedDocumentDialog ? (
        <GeneratedDocumentDialog
          closeLabel={t`Close`}
          content={generatedDocumentDialog.content}
          onClose={() => setGeneratedDocumentDialog(null)}
          title={generatedDocumentDialog.document.title}
          titleLabel={
            generatedDocumentDialog.document.format === "markdown"
              ? t`Markdown`
              : t`Plain text`
          }
        />
      ) : null}

      {pendingRenameGeneratedDocument ? (
        <RenameDocumentDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Rename`}
          isBusy={isBusy}
          onCancel={() => setPendingRenameGeneratedDocument(null)}
          onConfirm={(title) =>
            renameGeneratedDocument(
              pendingRenameGeneratedDocument,
              title
            ).catch((error) => {
              console.error("Failed to rename generated document", error)
            })
          }
          title={t`Rename generated document`}
          value={pendingRenameGeneratedDocument.title}
        />
      ) : null}

      {pendingDeleteGeneratedDocument ? (
        <ConfirmDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Delete`}
          isBusy={isBusy}
          message={t`This generated document will be removed. Published raw documents will be kept.`}
          onCancel={() => setPendingDeleteGeneratedDocument(null)}
          onConfirm={() =>
            deleteGeneratedDocument(pendingDeleteGeneratedDocument).catch(
              (error) => {
                console.error("Failed to delete generated document", error)
              }
            )
          }
          title={t`Delete generated document?`}
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

function WritingComposer({
  documents,
  isSelectionBusy,
  onOpenLibrary,
  onReload,
  rawDocuments,
  selection,
  state,
  tasks,
  transport,
}: {
  documents: WikiGeneratedDocument[]
  isSelectionBusy: boolean
  onOpenLibrary: () => void
  onReload: () => Promise<void>
  rawDocuments: WikiRawDocument[]
  selection: WikiAgentSelection
  state: WritingState
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const [draft, setDraft] = useState(state.draft)
  const [mode, setMode] = useState<WritingState["mode"]>(state.mode)
  const [refinementName, setRefinementName] = useState(state.refinementName)
  const [targetId, setTargetId] = useState<string | null>(
    state.targetGeneratedDocumentId
  )
  const [selectedCreateWritingSkillIds, setSelectedCreateWritingSkillIds] =
    useState(state.selectedCreateWritingSkillIds)
  const [selectedRevisionWritingSkillId, setSelectedRevisionWritingSkillId] =
    useState<string | null>(state.selectedRevisionWritingSkillId)
  const selectedWritingSkillIds = activeWritingSkillIds(
    mode,
    selectedCreateWritingSkillIds,
    selectedRevisionWritingSkillId
  )
  const [writingSkills, setWritingSkills] = useState<AgentSkillListing[]>([])
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const writingTasks = useMemo(() => {
    const taskType =
      mode === "refine" ? "refine_writing_skill" : "generate_document"
    return tasks.filter(
      (task) => task.queue === "writing" && task.type === taskType
    )
  }, [mode, tasks])
  const refinementTaskVersion = useMemo(
    () =>
      tasks
        .filter(
          (task) =>
            task.queue === "writing" && task.type === "refine_writing_skill"
        )
        .map((task) => `${task.id}:${task.status}:${task.updatedAt}`)
        .join("|"),
    [tasks]
  )

  useEffect(() => {
    if (targetId && !documents.some((document) => document.id === targetId)) {
      setTargetId(null)
    }
  }, [documents, targetId])

  useEffect(() => {
    let isCancelled = false
    apiJson<{
      skills?: AgentSkillListing[]
    }>(
      transport.apiBase,
      `/api/writing/skills?taskVersion=${encodeURIComponent(refinementTaskVersion)}`
    )
      .then((data) => {
        if (!isCancelled) setWritingSkills(data.skills ?? [])
      })
      .catch((skillError) => {
        if (!isCancelled) {
          console.error("Failed to load Writing Skills", skillError)
        }
      })
    return () => {
      isCancelled = true
    }
  }, [refinementTaskVersion, transport.apiBase])

  useEffect(() => {
    const timer = window.setTimeout(() => {
      apiJson(transport.apiBase, "/api/writing/draft", {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          draft,
          mode,
          refinementName,
          selectedCreateWritingSkillIds,
          selectedRevisionWritingSkillId,
          targetGeneratedDocumentId: mode === "revise" ? targetId : null,
        }),
      }).catch((saveError) => {
        console.error("Failed to save Writing draft", saveError)
      })
    }, 500)
    return () => window.clearTimeout(timer)
  }, [
    draft,
    mode,
    refinementName,
    selectedCreateWritingSkillIds,
    selectedRevisionWritingSkillId,
    targetId,
    transport.apiBase,
  ])

  const skillSelectionError = writingSkillSelectionError(
    mode,
    selectedWritingSkillIds
  )
  const hasValidSkillSelection = skillSelectionError === null
  const selectedRawDocuments = rawDocuments.filter((document) =>
    selection.selectedRawDocumentIds.includes(document.id)
  )
  const selectedGeneratedDocuments = documents.filter((document) =>
    selection.selectedGeneratedDocumentIds.includes(document.id)
  )
  const unreadySourceCount =
    selectedRawDocuments.filter((document) => !document.markdownRef).length +
    selectedGeneratedDocuments.filter(
      (document) =>
        document.generation !== undefined &&
        document.generation.status !== "completed"
    ).length
  const selectedSourceCount =
    selectedRawDocuments.length + selectedGeneratedDocuments.length

  const submit = useCallback(async () => {
    if (
      !(
        draft.trim() &&
        hasValidSkillSelection &&
        (mode !== "revise" || targetId)
      )
    ) {
      return
    }
    setIsSubmitting(true)
    setError(null)
    try {
      await apiJson(transport.apiBase, "/api/writing/requests", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          instruction: draft,
          mode,
          targetGeneratedDocumentId: mode === "revise" ? targetId : null,
          writingSkillIds: selectedWritingSkillIds,
        }),
      })
      setDraft("")
      setTargetId(null)
      await onReload()
    } catch (submitError) {
      setError(
        submitError instanceof Error
          ? submitError.message
          : t`Failed to submit writing request`
      )
    } finally {
      setIsSubmitting(false)
    }
  }, [
    draft,
    hasValidSkillSelection,
    mode,
    onReload,
    selectedWritingSkillIds,
    t,
    targetId,
    transport.apiBase,
  ])

  const submitRefinement = useCallback(async () => {
    if (
      !refinementName.trim() ||
      refinementName.trim().length > 80 ||
      selectedSourceCount === 0 ||
      unreadySourceCount > 0
    ) {
      return
    }
    setIsSubmitting(true)
    setError(null)
    try {
      await apiJson(transport.apiBase, "/api/writing/refinement-requests", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: refinementName }),
      })
      setRefinementName("")
      await onReload()
    } catch (submitError) {
      setError(
        submitError instanceof Error
          ? submitError.message
          : t`Failed to submit refinement request`
      )
    } finally {
      setIsSubmitting(false)
    }
  }, [
    onReload,
    refinementName,
    selectedSourceCount,
    t,
    transport.apiBase,
    unreadySourceCount,
  ])

  const toggleWritingSkill = useCallback(
    (skillId: string, isSelected: boolean) => {
      if (mode === "revise") {
        setSelectedRevisionWritingSkillId(isSelected ? skillId : null)
        return
      }
      setSelectedCreateWritingSkillIds((current) =>
        toggleWritingSkillSelection(current, skillId, isSelected, "create")
      )
    },
    [mode]
  )

  const taskAction = useCallback(
    async (taskId: string, action: "cancel" | "retry") => {
      await apiJson(
        transport.apiBase,
        `/api/tasks/${encodeURIComponent(taskId)}/${action}`,
        { method: "POST" }
      )
      await onReload()
    },
    [onReload, transport.apiBase]
  )

  return (
    <section className="op-wiki-column op-writing-composer">
      <div className="op-wiki-column__header">
        <div className="op-wiki-column__title">
          <Button
            aria-label={t`Open document library`}
            className="op-writing-mobile-library-button"
            isIconOnly
            onPress={onOpenLibrary}
            size="sm"
            variant="ghost"
          >
            <PanelLeft size={17} />
          </Button>
          <h2>{t`Writing`}</h2>
        </div>
      </div>
      <div className="op-writing-composer__content op-wiki-column__content">
        <Tabs
          className="op-writing-mode"
          onSelectionChange={(key) =>
            setMode(String(key) as WritingState["mode"])
          }
          selectedKey={mode}
        >
          <Tabs.ListContainer>
            <Tabs.List aria-label={t`Writing mode`}>
              <Tabs.Tab id="create">
                {t`New document`}
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="revise">
                {t`Revise`}
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="refine">
                {t`Refine`}
                <Tabs.Indicator />
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.ListContainer>
        </Tabs>

        {mode === "refine" ? (
          <div className="op-writing-refinement">
            <div className="op-writing-refinement__intro">
              <Sparkles aria-hidden size={18} />
              <div>
                <strong>{t`Turn selected articles into a Writing Skill`}</strong>
                <p>
                  {t`The Agent will extract reusable voice, structure, pacing, and techniques from all selected raw and generated documents.`}
                </p>
              </div>
            </div>
            <p className="op-writing-refinement__note">
              {t`The selected Wiki is ignored. Raw documents must be converted to Markdown. When complete, the project Skill will be available for new documents and revisions.`}
            </p>
            <div className="op-writing-refinement__sources">
              <strong>
                {t("Selected articles")}: {selectedSourceCount}
              </strong>
              {selectedSourceCount === 0 ? (
                <span>{t`Select at least one raw or generated document`}</span>
              ) : unreadySourceCount > 0 ? (
                <span>
                  {t`Some selected documents are not ready. Wait for processing or deselect them.`}
                </span>
              ) : (
                <span>{t`All selected articles will be refined together`}</span>
              )}
            </div>
            <label className="op-writing-target">
              <span>{t`Writing Skill name`}</span>
              <input
                aria-label={t`Writing Skill name`}
                maxLength={80}
                onChange={(event) => setRefinementName(event.target.value)}
                placeholder={t`Name this reusable writing method`}
                value={refinementName}
              />
            </label>
            {error ? <div className="op-writing-error">{error}</div> : null}
          </div>
        ) : (
          <>
            {mode === "revise" ? (
              <label className="op-writing-target">
                <span>{t`Document to revise`}</span>
                <select
                  onChange={(event) => setTargetId(event.target.value || null)}
                  value={targetId ?? ""}
                >
                  <option value="">{t`Select a generated document`}</option>
                  {documents.map((document) => (
                    <option key={document.id} value={document.id}>
                      {document.title}
                    </option>
                  ))}
                </select>
              </label>
            ) : null}

            <div className="op-writing-skills">
              <div className="op-writing-skills__header">
                <strong>{t`Writing Skills`}</strong>
                <span>
                  {mode === "revise" ? t`Select one` : t`Select one or more`}
                </span>
              </div>
              <div className="op-writing-skills__list">
                {writingSkills.map((item) => (
                  <label
                    className="op-writing-skill"
                    htmlFor={`writing-skill-${item.skill.id}`}
                    key={item.skill.id}
                  >
                    <Checkbox
                      aria-label={`${t`Select Writing Skill`}: ${item.skill.title}`}
                      id={`writing-skill-${item.skill.id}`}
                      isSelected={selectedWritingSkillIds.includes(
                        item.skill.id
                      )}
                      onChange={(isSelected) =>
                        toggleWritingSkill(item.skill.id, isSelected)
                      }
                    />
                    <span className="op-writing-skill__body">
                      <span className="op-writing-skill__title">
                        <strong>{item.skill.title}</strong>
                        <span>
                          {item.source === "builtin" ? t`Built-in` : t`Project`}
                        </span>
                      </span>
                      <span className="op-writing-skill__description">
                        {item.skill.description}
                      </span>
                    </span>
                  </label>
                ))}
                {writingSkills.length === 0 ? (
                  <div className="op-wiki-empty-inline">
                    {t`No Writing Skills available`}
                  </div>
                ) : null}
              </div>
              {skillSelectionError === "required" ? (
                <div className="op-writing-error">
                  {t`Select at least one Writing Skill`}
                </div>
              ) : skillSelectionError === "revision_limit" ? (
                <div className="op-writing-error">
                  {t`Revision mode supports one Writing Skill`}
                </div>
              ) : null}
            </div>

            <textarea
              aria-label={t`Writing instructions`}
              className="op-writing-instructions"
              onChange={(event) => setDraft(event.target.value)}
              placeholder={t`Describe what the agent should write`}
              value={draft}
            />
            {error ? <div className="op-writing-error">{error}</div> : null}
          </>
        )}

        <div className="op-writing-requests">
          {writingTasks.map((task) => {
            const instruction = taskInstruction(task)
            const skillTitle = taskWritingSkillTitle(task)
            const isRefinement = task.type === "refine_writing_skill"
            const canCancel = [
              "queued",
              "reserved",
              "running",
              "claimed",
            ].includes(task.status)
            return (
              <div className="op-writing-request" key={task.id}>
                <div className="op-writing-request__body">
                  <strong>
                    {skillTitle ||
                      (isRefinement
                        ? t`Writing Skill refinement`
                        : t`Writing request`)}
                  </strong>
                  {instruction ? (
                    <span className="op-writing-request__instruction">
                      {instruction}
                    </span>
                  ) : null}
                  <span>{writingTaskStatus(task, t)}</span>
                </div>
                {canCancel ? (
                  <Button
                    aria-label={
                      isRefinement
                        ? t`Cancel refinement request`
                        : t`Cancel writing request`
                    }
                    isIconOnly
                    onPress={() => taskAction(task.id, "cancel")}
                    size="sm"
                    variant="ghost"
                  >
                    <X size={14} />
                  </Button>
                ) : task.status === "failed" ? (
                  <Button
                    aria-label={
                      isRefinement
                        ? t`Retry refinement request`
                        : t`Retry writing request`
                    }
                    isIconOnly
                    onPress={() => taskAction(task.id, "retry")}
                    size="sm"
                    variant="ghost"
                  >
                    <RotateCcw size={14} />
                  </Button>
                ) : null}
              </div>
            )
          })}
          {writingTasks.length === 0 ? (
            <div className="op-wiki-empty-inline">
              {mode === "refine"
                ? t`No refinement requests yet`
                : t`No writing requests yet`}
            </div>
          ) : null}
        </div>
      </div>
      <div className="op-writing-submit-dock">
        {mode === "refine" ? (
          <Button
            className="op-writing-submit"
            isDisabled={
              isSubmitting ||
              isSelectionBusy ||
              !refinementName.trim() ||
              refinementName.trim().length > 80 ||
              selectedSourceCount === 0 ||
              unreadySourceCount > 0
            }
            onPress={() => submitRefinement()}
            variant="primary"
          >
            <Sparkles size={15} />
            <span>{isSubmitting ? t`Submitting` : t`Start refinement`}</span>
          </Button>
        ) : (
          <Button
            className="op-writing-submit"
            isDisabled={
              isSubmitting ||
              !draft.trim() ||
              !hasValidSkillSelection ||
              (mode === "revise" && !targetId)
            }
            onPress={() => submit()}
            variant="primary"
          >
            <Send size={15} />
            <span>{isSubmitting ? t`Submitting` : t`Start writing`}</span>
          </Button>
        )}
      </div>
    </section>
  )
}

function taskInstruction(task: ProjectTask): string {
  if (!task.input || typeof task.input !== "object") return ""
  const instruction = (task.input as { instruction?: unknown }).instruction
  return typeof instruction === "string" ? instruction : ""
}

function taskWritingSkillTitle(task: ProjectTask): string {
  if (!task.input || typeof task.input !== "object") return ""
  const refinementName = (task.input as { name?: unknown }).name
  if (typeof refinementName === "string") return refinementName
  const writingSkill = (task.input as { writingSkill?: unknown }).writingSkill
  if (!(writingSkill && typeof writingSkill === "object")) return ""
  const title = (writingSkill as { title?: unknown }).title
  return typeof title === "string" ? title : ""
}

function writingTaskStatus(
  task: ProjectTask,
  t: (value: string) => string
): string {
  if (task.dispatchState === "noTarget" && task.status === "queued") {
    return t("Waiting for Agent")
  }
  if (task.status === "queued" || task.status === "reserved") return t("Queued")
  if (["running", "claimed"].includes(task.status)) {
    return task.type === "refine_writing_skill" ? t("Refining") : t("Writing")
  }
  if (task.status === "succeeded") return t("Completed")
  if (task.status === "failed") return t("Failed")
  if (task.status === "cancelled") return t("Cancelled")
  return task.status
}
