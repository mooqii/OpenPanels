import {
  Button,
  Checkbox,
  Dropdown,
  Header,
  Surface,
  Tooltip,
} from "@heroui/react"
import {
  ChevronDown,
  ChevronRight,
  Edit3,
  ExternalLink,
  Eye,
  FileOutput,
  FileText,
  Folder,
  FolderOpen,
  Info,
  MoreHorizontal,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
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
  fileToDataUrl,
  originalPreviewKind,
  titleFromFileName,
  wikiRawOriginalUrl,
} from "../../lib/api"
import type {
  AgentSkillListing,
  MyOpenPanelsTransport,
  WikiGeneratedDocument,
  WikiRawDocument,
  WikiState,
} from "../../types"
import {
  ConfirmDialog,
  GeneratedDocumentDialog,
  MarkdownDialog,
  OriginalPreviewDialog,
  RenameDocumentDialog,
} from "./Dialogs"
import {
  documentIndexStatus,
  formatWikiPageType,
  WikiIndexStatus,
  WikiStatus,
} from "./helpers"
import { buildWikiPageTree, type WikiPageTreeNode } from "./page-tree"

const DEFAULT_WIKI_AGENT_SKILL_ID = "karpathy-llm-wiki"
const DEFAULT_ZH_WIKI_AGENT_SKILL_ID = "karpathy-llm-wiki-zh"
type WikiModule = "raw" | "structured" | "generated"

interface WikiAgentSelection {
  isWikiSelected: boolean
  selectedGeneratedDocumentIds: string[]
  selectedRawDocumentIds: string[]
}

export function WikiPanel({
  chromeContent,
  onReload,
  selectionVersion,
  state,
  transport,
}: {
  chromeContent: ReactNode
  onReload: () => Promise<void>
  selectionVersion: number
  state: WikiState
  transport: MyOpenPanelsTransport
}) {
  const { locale, t } = useMyOpenPanelsI18n()
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
  const [isSelectionBusy, setIsSelectionBusy] = useState(true)
  const [agentSelection, setAgentSelection] = useState<WikiAgentSelection>({
    isWikiSelected: false,
    selectedGeneratedDocumentIds: [],
    selectedRawDocumentIds: [],
  })
  const [isRawDragActive, setIsRawDragActive] = useState(false)
  const [collapsedWikiFolders, setCollapsedWikiFolders] = useState<Set<string>>(
    () => new Set()
  )
  const [collapsedModules, setCollapsedModules] = useState<Set<WikiModule>>(
    () => new Set()
  )
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

  const toggleModule = useCallback((module: WikiModule) => {
    setCollapsedModules((current) => {
      const next = new Set(current)
      if (next.has(module)) next.delete(module)
      else next.add(module)
      return next
    })
  }, [])

  const moduleHeaderToggle = (module: WikiModule, title: string) => {
    const isCollapsed = collapsedModules.has(module)
    return (
      <button
        aria-expanded={!isCollapsed}
        aria-label={`${isCollapsed ? t`Expand module` : t`Collapse module`}: ${title}`}
        className="op-wiki-column__header-toggle"
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

        const secondaryLabel =
          node.page.summary ||
          (node.page.title && node.page.title !== node.fileName
            ? t(node.page.title)
            : node.page.path)
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
              <span className="op-wiki-page-row__summary">
                {secondaryLabel}
              </span>
            </span>
            <span className="op-wiki-page-row__side">
              <span className="op-wiki-page-row__type">
                {formatWikiPageType(node.page.type, t)}
              </span>
              <Edit3 className="op-wiki-page-row__edit" size={14} />
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
    apiFetch(
      transport.apiBase,
      `/api/wiki/selection?version=${selectionVersion}`
    )
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
  }, [selectionVersion, transport.apiBase])

  const updateAgentSelection = useCallback(
    async (next: WikiAgentSelection) => {
      const previous = agentSelection
      setAgentSelection(next)
      setIsSelectionBusy(true)
      try {
        const response = await apiFetch(
          transport.apiBase,
          "/api/wiki/selection",
          {
            method: "PUT",
            headers: { "content-type": "application/json" },
            body: JSON.stringify(next),
          }
        )
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
    [agentSelection, transport.apiBase]
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

  return (
    <section className="op-wiki-panel">
      <header className="op-canvas-title">{chromeContent}</header>
      <Surface className="op-wiki-panel__surface" variant="default">
        <div className="op-wiki-workbench">
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
            <div className="op-wiki-list op-wiki-column__content">
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
                        <WikiStatus
                          document={document}
                          isDisabled={isBusy || !hasMarkdown}
                          onOpenMarkdown={() => {
                            openMarkdown(document).catch((error) => {
                              console.error(
                                "Failed to open wiki markdown",
                                error
                              )
                            })
                          }}
                        />
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
                                    revealOriginal(document).catch((error) => {
                                      console.error(
                                        "Failed to reveal wiki raw document",
                                        error
                                      )
                                    })
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
                <div className="op-wiki-empty-inline">{t`No raw documents yet`}</div>
              )}
            </div>
          </aside>

          <section
            className={
              collapsedModules.has("structured")
                ? "op-wiki-column--collapsed op-wiki-column op-wiki-column--structured"
                : "op-wiki-column op-wiki-column--structured"
            }
          >
            <div className="op-wiki-column__header">
              {moduleHeaderToggle("structured", activeSpace?.title || t`Wiki`)}
              <div className="op-wiki-column__title">
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
                      console.error("Failed to update Wiki selection", error)
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
                <h2>{activeSpace?.title ? t(activeSpace.title) : t`Wiki`}</h2>
                {moduleInfo(
                  activeSpace?.title || t`Wiki`,
                  t`Structured knowledge pages generated from your sources live here. Agents can search and update this Wiki. Selecting it lets the agent discover the Wiki and load relevant pages when needed.`
                )}
              </div>
              <div className="op-wiki-actions">
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
                  <Dropdown.Popover className="op-wiki-agent-skill-popover">
                    <Dropdown.Menu
                      aria-label={t`Wiki generation method`}
                      onAction={(key) => {
                        updateWikiAgentSkill(String(key)).catch(() => undefined)
                      }}
                      selectedKeys={[wikiAgentSkillId]}
                      selectionMode="single"
                    >
                      <Dropdown.Section>
                        <Header className="op-wiki-agent-skill-menu-header">
                          {t`Wiki generation method`}
                        </Header>
                        {agentSkills.map((item) => (
                          <Dropdown.Item id={item.skill.id} key={item.skill.id}>
                            {item.skill.title}
                          </Dropdown.Item>
                        ))}
                      </Dropdown.Section>
                    </Dropdown.Menu>
                  </Dropdown.Popover>
                </Dropdown>
              </div>
            </div>
            <div className="op-wiki-page-tree op-wiki-column__content">
              {renderWikiPageNodes(wikiPageTree)}
            </div>
          </section>

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
            <div className="op-wiki-list op-wiki-column__content">
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
                          <span className="op-wiki-list-item__meta">
                            {document.format === "markdown"
                              ? t`Markdown`
                              : t`Plain text`}
                            {" · "}
                            {new Date(document.updatedAt).toLocaleString()}
                          </span>
                        </div>
                      </button>
                      <div className="op-wiki-list-item__tools">
                        {isGenerating || generationFailed ? (
                          <span
                            className={`op-generated-document-status${
                              generationFailed
                                ? "op-generated-document-status--failed"
                                : ""
                            }`}
                            title={document.generation?.error ?? undefined}
                          >
                            <RefreshCw
                              className={
                                isGenerating ? "op-wiki-spin" : undefined
                              }
                              size={14}
                            />
                            {isGenerating
                              ? t`Generating`
                              : t`Generation failed`}
                          </span>
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
                                  setPendingRenameGeneratedDocument(document)
                                } else if (key === "delete") {
                                  setPendingDeleteGeneratedDocument(document)
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
                <div className="op-wiki-empty-inline">
                  {t`No generated documents yet`}
                </div>
              )}
            </div>
          </section>
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
