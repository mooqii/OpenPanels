import { Button, Tooltip } from "@heroui/react"
import { ChevronRight, FileText, Folder, FolderOpen, Info } from "lucide-react"
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
import { sortGeneratedDocumentsByActivity } from "../../lib/writing"
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
  nextCollapsedModules,
  serializeWikiCollapsedModules,
  WIKI_COLLAPSED_MODULES_STORAGE_KEY,
  type WikiModule,
  wikiCollapsedModulesFromStorage,
} from "./module-collapse"
import { buildWikiPageTree, type WikiPageTreeNode } from "./page-tree"
import { WikiPageMeta } from "./WikiPageMeta"

const DEFAULT_WIKI_AGENT_SKILL_ID = "karpathy-llm-wiki"
const DEFAULT_ZH_WIKI_AGENT_SKILL_ID = "karpathy-llm-wiki-zh"
interface WikiAgentSelection {
  isWikiSelected: boolean
  selectedGeneratedDocumentIds: string[]
  selectedRawDocumentIds: string[]
}

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
  const { locale, t } = useMyOpenPanelsI18n()
  const selectionPath = writing
    ? "/api/writing/selection"
    : "/api/wiki/selection"
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
  const [pendingDeleteGeneratedDocument, setPendingDeleteGeneratedDocument] =
    useState<WikiGeneratedDocument | null>(null)
  const [pendingRenameGeneratedDocument, setPendingRenameGeneratedDocument] =
    useState<WikiGeneratedDocument | null>(null)
  const [generatedDocumentDialog, setGeneratedDocumentDialog] = useState<{
    content: string
    document: WikiGeneratedDocument
    originalContent: string
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
  const markdownDialogDocumentId = markdownDialog?.document.id
  const pageDialogPath = pageDialog?.pagePath
  const pageDialogTitle = pageDialog?.title
  const generatedDialogDocumentId = generatedDocumentDialog?.document.id
  const generatedDialogFileName =
    generatedDocumentDialog?.document.originalFileName
  const generatedDialogMimeType = generatedDocumentDialog?.document.mimeType
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
      ? module === "generated" || module === "structured" || module === "raw"
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

  useEffect(() => {
    if (state.wikiAgentSkillConfigured || agentSkills.length === 0) return
    const defaultSkillId =
      locale === "zh-CN"
        ? DEFAULT_ZH_WIKI_AGENT_SKILL_ID
        : DEFAULT_WIKI_AGENT_SKILL_ID
    if (!agentSkills.some((item) => item.skill.id === defaultSkillId)) return
    updateWikiAgentSkill(defaultSkillId, true).catch((error) => {
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
        originalContent: data.content ?? "",
      })
    },
    [transport.apiBase]
  )

  const createGeneratedMarkdownDocument = useCallback(async () => {
    setIsBusy(true)
    try {
      const data = await apiJson<{ document: WikiGeneratedDocument }>(
        transport.apiBase,
        "/api/wiki/generated-documents",
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
      setGeneratedDocumentDialog({
        content: "",
        document: data.document,
        originalContent: "",
      })
    } finally {
      setIsBusy(false)
    }
  }, [onReload, t, transport.apiBase])

  const saveGeneratedMarkdown = useCallback(
    async (content: string) => {
      if (
        !(
          generatedDialogDocumentId &&
          generatedDialogFileName &&
          generatedDialogMimeType
        )
      )
        return
      const data = await apiJson<{ document: WikiGeneratedDocument }>(
        transport.apiBase,
        `/api/wiki/generated-documents/${encodeURIComponent(generatedDialogDocumentId)}`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            content,
            fileName: generatedDialogFileName,
            mimeType: generatedDialogMimeType,
          }),
        }
      )
      setGeneratedDocumentDialog((current) =>
        current
          ? { ...current, document: data.document, originalContent: content }
          : current
      )
      await onReload()
    },
    [
      generatedDialogDocumentId,
      generatedDialogFileName,
      generatedDialogMimeType,
      onReload,
      transport.apiBase,
    ]
  )

  const renameGeneratedDocumentFile = useCallback(
    async (fileName: string) => {
      if (!generatedDialogDocumentId) return
      const data = await apiJson<{ document: WikiGeneratedDocument }>(
        transport.apiBase,
        `/api/wiki/generated-documents/${encodeURIComponent(generatedDialogDocumentId)}`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ fileName }),
        }
      )
      setGeneratedDocumentDialog((current) =>
        current ? { ...current, document: data.document } : current
      )
      await onReload()
    },
    [generatedDialogDocumentId, onReload, transport.apiBase]
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
    async (
      document: WikiGeneratedDocument,
      writingTask?: ProjectTask | null
    ) => {
      setRetryingGeneratedDocumentId(document.id)
      setGeneratedDocumentRetryError(null)
      try {
        const path =
          writingTask?.status === "failed"
            ? `/api/tasks/${encodeURIComponent(writingTask.id)}/retry`
            : `/api/wiki/generated-documents/${encodeURIComponent(document.id)}/retry`
        await apiJson(transport.apiBase, path, { method: "POST" })
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

  const displayedGeneratedDocuments = writing
    ? sortGeneratedDocumentsByActivity(state.generatedDocuments, writing.tasks)
    : state.generatedDocuments

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
    pendingDeleteGeneratedDocument,
    setPendingDeleteGeneratedDocument,
    pendingRenameGeneratedDocument,
    setPendingRenameGeneratedDocument,
    generatedDocumentDialog,
    setGeneratedDocumentDialog,
    originalPreviewDocument,
    setOriginalPreviewDocument,
    isBusy,
    retryingGeneratedDocumentId,
    generatedDocumentRetryError,
    isSelectionBusy,
    agentSelection,
    isRawDragActive,
    isDocumentLibraryOpen,
    setIsDocumentLibraryOpen,
    collapsedModules,
    fileInputRef,
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
    openGeneratedDocument,
    createGeneratedMarkdownDocument,
    saveGeneratedMarkdown,
    renameGeneratedDocumentFile,
    publishGeneratedDocument,
    renameGeneratedDocument,
    deleteGeneratedDocument,
    retryGeneratedDocument,
    displayedGeneratedDocuments,
  }
}

export type ReturnTypeOfWikiPanelController = ReturnType<
  typeof useWikiPanelController
>
