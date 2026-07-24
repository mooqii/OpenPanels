import { Button, Tooltip } from "@heroui/react"
import { ChevronRight, FileText, Folder, FolderOpen, Info } from "lucide-react"
import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import {
  apiFetch,
  apiJson,
  myDocumentOriginalUrl,
  originalPreviewKind,
  titleFromFileName,
  tryOpenBrowserWindow,
  wikiRawOriginalUrl,
} from "../../lib/api"
import { taskCanRetry } from "../../lib/task-status"
import { sortMyDocumentsByActivity } from "../../lib/writing"
import type {
  AgentSkillListing,
  MyDocument,
  MyOpenPanelsTransport,
  ProjectTask,
  WikiOriginalPreviewDocument,
  WikiRawDocument,
  WikiState,
  WritingState,
} from "../../types"
import { nextCollapsedModules, type WikiModule } from "./module-collapse"
import { buildWikiPageTree, type WikiPageTreeNode } from "./page-tree"
import { useMyDocumentDrop } from "./useMyDocumentDrop"
import { useRawDocumentDrop } from "./useRawDocumentDrop"
import { WikiPageMeta } from "./WikiPageMeta"
import {
  normalizeWikiAgentSelection,
  type WikiAgentSelection,
  wikiAgentSelectionRequest,
} from "./wiki-selection"

const DEFAULT_WIKI_AGENT_SKILL_ID = "wiki-default"

export interface WikiPanelProps {
  chromeContent: ReactNode
  onManageSkills: () => void
  onOpenAgentTasks: (
    filter: "active" | "pending" | "done" | "all",
    taskIds?: string[]
  ) => void
  onReload: () => Promise<void>
  selectionVersion: number
  skillsRevision: number
  state: WikiState
  transport: MyOpenPanelsTransport
  writing?: { state: WritingState; tasks: ProjectTask[] }
}

export function useWikiPanelController({
  onReload,
  selectionVersion,
  state,
  transport,
  writing,
}: WikiPanelProps) {
  const { t } = useMyOpenPanelsI18n()
  const selectionPath = writing
    ? "/api/writing/selection"
    : "/api/wiki/selection"
  const isWritingPanel = Boolean(writing)
  const activeSpace =
    state.wikiSpaces.find((space) => space.id === state.activeWikiSpaceId) ??
    state.wikiSpaces[0]
  const activeSpaceId = activeSpace?.id
  const wikiAgentSkillId = state.wikiAgentSkillId || DEFAULT_WIKI_AGENT_SKILL_ID
  const [agentSkills, setAgentSkills] = useState<AgentSkillListing[]>([])
  const [pendingWikiAgentSkillId, setPendingWikiAgentSkillId] = useState<
    string | null
  >(null)
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
  const [pendingRenameRawDocument, setPendingRenameRawDocument] =
    useState<WikiRawDocument | null>(null)
  const [pendingDeleteMyDocument, setPendingDeleteMyDocument] =
    useState<MyDocument | null>(null)
  const [pendingRenameMyDocument, setPendingRenameMyDocument] =
    useState<MyDocument | null>(null)
  const [myDocumentDialog, setMyDocumentDialog] = useState<{
    content: string
    document: MyDocument
    originalContent: string
  } | null>(null)
  const [originalPreview, setOriginalPreview] = useState<{
    document: WikiOriginalPreviewDocument
    previewUrl: string
  } | null>(null)
  const [isBusy, setIsBusy] = useState(false)
  const [retryingMyDocumentId, setRetryingMyDocumentId] = useState<
    string | null
  >(null)
  const [myDocumentRetryError, setMyDocumentRetryError] = useState<
    string | null
  >(null)
  const [isSelectionBusy, setIsSelectionBusy] = useState(true)
  const [agentSelection, setAgentSelection] = useState<WikiAgentSelection>({
    isWikiSelected: false,
    selectedMyDocumentIds: [],
  })
  const [isDocumentLibraryOpen, setIsDocumentLibraryOpen] = useState(false)
  const [collapsedWikiFolders, setCollapsedWikiFolders] = useState<Set<string>>(
    () => new Set()
  )
  const [writingCollapsedModules, setWritingCollapsedModules] = useState<
    Set<WikiModule>
  >(() => new Set())
  const collapsedModules = writing
    ? writingCollapsedModules
    : new Set<WikiModule>()
  const markdownDialogDocumentId = markdownDialog?.document.id
  const pageDialogPath = pageDialog?.pagePath
  const pageDialogTitle = pageDialog?.title
  const myDocumentDialogDocumentId = myDocumentDialog?.document.id
  const myDocumentDialogFileName = myDocumentDialog?.document.originalFileName
  const myDocumentDialogMimeType = myDocumentDialog?.document.mimeType
  const {
    addMyDocumentFiles,
    myDocumentFileInputRef,
    handleMyDocumentDragEnter,
    handleMyDocumentDragLeave,
    handleMyDocumentDragOver,
    handleMyDocumentDrop,
    isMyDocumentDragActive,
  } = useMyDocumentDrop({
    apiBase: transport.apiBase,
    onReload,
    setIsBusy,
  })
  const {
    addFiles,
    fileInputRef,
    handleRawDragEnter,
    handleRawDragLeave,
    handleRawDragOver,
    handleRawDrop,
    isRawDragActive,
  } = useRawDocumentDrop({
    activeSpaceId: activeSpace?.id,
    apiBase: transport.apiBase,
    onReload,
    setIsBusy,
  })
  const wikiPageTree = useMemo(
    () => buildWikiPageTree(activeSpace?.pageIndex ?? []),
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

  const toggleModule = useCallback(
    (module: WikiModule) => {
      if (!writing) return
      setWritingCollapsedModules((current) =>
        nextCollapsedModules(current, module)
      )
    },
    [writing]
  )

  const moduleHeaderToggle = (module: WikiModule, title: string) => {
    const isCollapsed = collapsedModules.has(module)
    return (
      <button
        aria-expanded={!isCollapsed}
        aria-label={`${isCollapsed ? t`Expand module` : t`Collapse module`}: ${title}`}
        className="op-wiki-column__header-toggle op-wiki-column__header-toggle--accordion"
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
              {activeSpaceId ? (
                <WikiPageMeta
                  apiBase={transport.apiBase}
                  page={node.page}
                  wikiSpaceId={activeSpaceId}
                />
              ) : null}
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
                item.skill.taskTypes.includes("ingest_markdown_into_wiki") &&
                item.skill.taskTypes.includes("maintain_wiki")
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
          setAgentSelection(
            normalizeWikiAgentSelection(data.selection, isWritingPanel)
          )
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
  }, [isWritingPanel, selectionPath, selectionVersion, transport.apiBase])

  const updateAgentSelection = useCallback(
    async (next: WikiAgentSelection) => {
      const previous = agentSelection
      const normalizedNext = normalizeWikiAgentSelection(next, isWritingPanel)
      setAgentSelection(normalizedNext)
      setIsSelectionBusy(true)
      try {
        const response = await apiFetch(transport.apiBase, selectionPath, {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(
            wikiAgentSelectionRequest(normalizedNext, isWritingPanel)
          ),
        })
        const data = (await response.json()) as {
          selection?: Partial<WikiAgentSelection>
        }
        setAgentSelection(
          normalizeWikiAgentSelection(data.selection, isWritingPanel)
        )
      } catch (error) {
        setAgentSelection(previous)
        throw error
      } finally {
        setIsSelectionBusy(false)
      }
    },
    [agentSelection, isWritingPanel, selectionPath, transport.apiBase]
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

  const saveMarkdown = useCallback(
    async (content: string) => {
      if (!markdownDialogDocumentId) return
      await apiJson(
        transport.apiBase,
        `/api/wiki/raw-documents/${encodeURIComponent(markdownDialogDocumentId)}/markdown`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ content }),
        }
      )
      setMarkdownDialog((current) =>
        current ? { ...current, originalContent: content } : current
      )
      await onReload()
    },
    [markdownDialogDocumentId, onReload, transport]
  )

  const renameRawDocumentFile = useCallback(
    async (fileName: string) => {
      if (!markdownDialogDocumentId) return
      const data = await apiJson<{ document: WikiRawDocument }>(
        transport.apiBase,
        `/api/wiki/raw-documents/${encodeURIComponent(markdownDialogDocumentId)}`,
        {
          method: "PATCH",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ fileName }),
        }
      )
      setMarkdownDialog((current) =>
        current ? { ...current, document: data.document } : current
      )
      await onReload()
    },
    [markdownDialogDocumentId, onReload, transport.apiBase]
  )

  const renameRawDocument = useCallback(
    async (document: WikiRawDocument, title: string) => {
      setIsBusy(true)
      try {
        await apiFetch(
          transport.apiBase,
          `/api/wiki/raw-documents/${encodeURIComponent(document.id)}`,
          {
            method: "PATCH",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ title }),
          }
        )
        setPendingRenameRawDocument(null)
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [onReload, transport.apiBase]
  )

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
        setOriginalPreview({
          document,
          previewUrl: wikiRawOriginalUrl(transport.apiBase, document),
        })
        return
      }
      openOriginalInNewWindow(document)
    },
    [openOriginalInNewWindow, transport.apiBase]
  )

  const revealMyDocumentOriginal = useCallback(
    async (document: MyDocument) => {
      await apiFetch(
        transport.apiBase,
        `/api/my-documents/${encodeURIComponent(document.id)}/reveal`,
        { method: "POST" }
      )
    },
    [transport.apiBase]
  )

  const openMyDocumentOriginal = useCallback(
    (document: MyDocument) => {
      if (!document.importSource) return
      const previewDocument: WikiOriginalPreviewDocument = {
        id: document.id,
        mimeType: document.importSource.mimeType,
        originalFileName: document.importSource.fileName,
        sizeBytes: document.importSource.sizeBytes,
        title: document.title,
      }
      const previewUrl = myDocumentOriginalUrl(transport.apiBase, document)
      if (originalPreviewKind(previewDocument)) {
        setOriginalPreview({ document: previewDocument, previewUrl })
        return
      }
      if (tryOpenBrowserWindow(previewUrl)) return
      revealMyDocumentOriginal(document).catch((error) => {
        console.error("Failed to reveal imported My Document", error)
      })
    },
    [revealMyDocumentOriginal, transport.apiBase]
  )

  const createRawMarkdownDocument = useCallback(async () => {
    setIsBusy(true)
    try {
      const data = await apiJson<{ document: WikiRawDocument }>(
        transport.apiBase,
        "/api/wiki/raw-documents",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            content: "",
            fileName: "untitled.md",
            mimeType: "text/markdown",
            source: "user",
            title: t`Untitled`,
            wikiSpaceId: activeSpace?.id,
          }),
        }
      )
      await onReload()
      setMarkdownDialog({
        content: "",
        document: data.document,
        originalContent: "",
      })
    } finally {
      setIsBusy(false)
    }
  }, [activeSpace?.id, onReload, t, transport.apiBase])

  const openWikiPage = useCallback(
    async (pagePath: string) => {
      if (!activeSpaceId) return
      const response = await apiFetch(
        transport.apiBase,
        `/api/wiki/spaces/${encodeURIComponent(activeSpaceId)}/pages/${pagePath
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
    [activeSpaceId, transport]
  )

  const saveWikiPage = useCallback(
    async (content: string) => {
      if (!(pageDialogPath && activeSpaceId)) return
      await apiJson(
        transport.apiBase,
        `/api/wiki/spaces/${encodeURIComponent(activeSpaceId)}/pages/${pageDialogPath
          .split("/")
          .map(encodeURIComponent)
          .join("/")}`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ title: pageDialogTitle, content }),
        }
      )
      setPageDialog((current) =>
        current ? { ...current, originalContent: content } : current
      )
      await onReload()
    },
    [activeSpaceId, onReload, pageDialogPath, pageDialogTitle, transport]
  )

  const renameWikiPageFile = useCallback(
    async (pagePath: string) => {
      if (!(pageDialogPath && activeSpaceId)) return
      await apiJson(
        transport.apiBase,
        `/api/wiki/spaces/${encodeURIComponent(activeSpaceId)}/pages/${pageDialogPath
          .split("/")
          .map(encodeURIComponent)
          .join("/")}`,
        {
          method: "PATCH",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ pagePath }),
        }
      )
      setPageDialog((current) =>
        current
          ? {
              ...current,
              pagePath,
              title: titleFromFileName(pagePath),
            }
          : current
      )
      await onReload()
    },
    [activeSpaceId, onReload, pageDialogPath, transport.apiBase]
  )

  const updateWikiAgentSkill = useCallback(
    async (agentSkillId: string, rebuildConfirmed = true) => {
      setIsBusy(true)
      try {
        const response = await apiFetch(
          transport.apiBase,
          "/api/wiki/agent-skill",
          {
            method: "PUT",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ agentSkillId, rebuildConfirmed }),
          }
        )
        if (!response.ok) {
          if (response.status === 404) return
          throw new Error(
            `Failed to update wiki agent skill: ${response.status}`
          )
        }
        setPendingWikiAgentSkillId(null)
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [onReload, transport.apiBase]
  )

  const openMyDocument = useCallback(
    async (document: MyDocument) => {
      const response = await apiFetch(
        transport.apiBase,
        `/api/my-documents/${encodeURIComponent(document.id)}`
      )
      const data = (await response.json()) as { content?: string }
      setMyDocumentDialog({
        content: data.content ?? "",
        document,
        originalContent: data.content ?? "",
      })
    },
    [transport.apiBase]
  )

  const createMyDocumentMarkdownDocument = useCallback(async () => {
    setIsBusy(true)
    try {
      const data = await apiJson<{ document: MyDocument }>(
        transport.apiBase,
        "/api/my-documents",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            content: "",
            fileName: "untitled.md",
            mimeType: "text/markdown",
            title: t`Untitled`,
          }),
        }
      )
      await onReload()
      setMyDocumentDialog({
        content: "",
        document: data.document,
        originalContent: "",
      })
    } finally {
      setIsBusy(false)
    }
  }, [onReload, t, transport.apiBase])

  const saveMyDocumentMarkdown = useCallback(
    async (content: string) => {
      if (
        !(
          myDocumentDialogDocumentId &&
          myDocumentDialogFileName &&
          myDocumentDialogMimeType
        )
      )
        return
      const data = await apiJson<{ document: MyDocument }>(
        transport.apiBase,
        `/api/my-documents/${encodeURIComponent(myDocumentDialogDocumentId)}`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            content,
            fileName: myDocumentDialogFileName,
            mimeType: myDocumentDialogMimeType,
          }),
        }
      )
      setMyDocumentDialog((current) =>
        current
          ? { ...current, document: data.document, originalContent: content }
          : current
      )
      await onReload()
    },
    [
      myDocumentDialogDocumentId,
      myDocumentDialogFileName,
      myDocumentDialogMimeType,
      onReload,
      transport.apiBase,
    ]
  )

  const renameMyDocumentFile = useCallback(
    async (fileName: string) => {
      if (!myDocumentDialogDocumentId) return
      const data = await apiJson<{ document: MyDocument }>(
        transport.apiBase,
        `/api/my-documents/${encodeURIComponent(myDocumentDialogDocumentId)}`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ fileName }),
        }
      )
      setMyDocumentDialog((current) =>
        current ? { ...current, document: data.document } : current
      )
      await onReload()
    },
    [myDocumentDialogDocumentId, onReload, transport.apiBase]
  )

  const publishMyDocument = useCallback(
    async (document: MyDocument) => {
      setIsBusy(true)
      try {
        await apiFetch(
          transport.apiBase,
          `/api/wiki-sources/from-my-document/${encodeURIComponent(document.id)}`,
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

  const renameMyDocument = useCallback(
    async (document: MyDocument, title: string) => {
      setIsBusy(true)
      try {
        await apiFetch(
          transport.apiBase,
          `/api/my-documents/${encodeURIComponent(document.id)}`,
          {
            method: "PUT",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ title }),
          }
        )
        setPendingRenameMyDocument(null)
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [onReload, transport.apiBase]
  )

  const deleteMyDocument = useCallback(
    async (document: MyDocument) => {
      setIsBusy(true)
      try {
        await apiFetch(
          transport.apiBase,
          `/api/my-documents/${encodeURIComponent(document.id)}`,
          { method: "DELETE" }
        )
        setPendingDeleteMyDocument(null)
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [onReload, transport.apiBase]
  )

  const retryMyDocument = useCallback(
    async (document: MyDocument, writingTask?: ProjectTask | null) => {
      setRetryingMyDocumentId(document.id)
      setMyDocumentRetryError(null)
      try {
        const taskId =
          writingTask && taskCanRetry(writingTask)
            ? writingTask.id
            : document.conversion?.status === "failed" &&
                document.conversion.taskId
              ? document.conversion.taskId
              : null
        if (!taskId) return
        await apiJson(
          transport.apiBase,
          `/api/tasks/${encodeURIComponent(taskId)}/retry`,
          { method: "POST" }
        )
        await onReload()
      } catch (error) {
        console.error("Failed to retry My Document", error)
        setMyDocumentRetryError(document.id)
      } finally {
        setRetryingMyDocumentId(null)
      }
    },
    [onReload, transport.apiBase]
  )

  const displayedMyDocuments = writing
    ? sortMyDocumentsByActivity(state.myDocuments, writing.tasks)
    : state.myDocuments

  return {
    t,
    activeSpace,
    wikiAgentSkillId,
    agentSkills,
    pendingWikiAgentSkillId,
    setPendingWikiAgentSkillId,
    markdownDialog,
    setMarkdownDialog,
    pageDialog,
    setPageDialog,
    pendingDeleteDocument,
    setPendingDeleteDocument,
    pendingRenameRawDocument,
    setPendingRenameRawDocument,
    pendingDeleteMyDocument,
    setPendingDeleteMyDocument,
    pendingRenameMyDocument,
    setPendingRenameMyDocument,
    myDocumentDialog,
    setMyDocumentDialog,
    originalPreview,
    setOriginalPreview,
    isBusy,
    retryingMyDocumentId,
    myDocumentRetryError,
    isSelectionBusy,
    agentSelection,
    isRawDragActive,
    isMyDocumentDragActive,
    isDocumentLibraryOpen,
    setIsDocumentLibraryOpen,
    collapsedModules,
    toggleModule,
    fileInputRef,
    myDocumentFileInputRef,
    wikiPageTree,
    moduleHeaderToggle,
    moduleInfo,
    renderWikiPageNodes,
    updateAgentSelection,
    openMarkdown,
    saveMarkdown,
    renameRawDocumentFile,
    renameRawDocument,
    extractMarkdown,
    reindexDocument,
    deleteRawDocument,
    revealOriginal,
    openOriginalInNewWindow,
    openRawOriginal,
    addFiles,
    createRawMarkdownDocument,
    handleRawDragEnter,
    handleRawDragOver,
    handleRawDragLeave,
    handleRawDrop,
    saveWikiPage,
    renameWikiPageFile,
    updateWikiAgentSkill,
    openMyDocument,
    openMyDocumentOriginal,
    createMyDocumentMarkdownDocument,
    addMyDocumentFiles,
    handleMyDocumentDragEnter,
    handleMyDocumentDragOver,
    handleMyDocumentDragLeave,
    handleMyDocumentDrop,
    saveMyDocumentMarkdown,
    renameMyDocumentFile,
    publishMyDocument,
    renameMyDocument,
    deleteMyDocument,
    retryMyDocument,
    displayedMyDocuments,
  }
}

export type ReturnTypeOfWikiPanelController = ReturnType<
  typeof useWikiPanelController
>
