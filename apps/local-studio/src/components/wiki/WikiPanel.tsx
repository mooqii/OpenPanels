import { Button, Dropdown, ListBox, Select, Surface } from "@heroui/react"
import {
  BookOpen,
  Edit3,
  ExternalLink,
  Eye,
  FilePlus,
  FolderOpen,
  MoreHorizontal,
  RefreshCw,
  Trash2,
  Upload,
} from "lucide-react"
import {
  type DragEvent,
  type ReactNode,
  useCallback,
  useRef,
  useState,
} from "react"
import {
  OPENPANELS_LOCALE_LABELS,
  type OpenPanelsLocale,
  useOpenPanelsI18n,
} from "../../canvas"
import {
  apiFetch,
  fileToDataUrl,
  originalPreviewKind,
  titleFromFileName,
  wikiRawOriginalUrl,
} from "../../lib/api"
import type {
  OpenPanelsTransport,
  WikiRawDocument,
  WikiState,
} from "../../types"
import { ConfirmDialog, MarkdownDialog, OriginalPreviewDialog } from "./Dialogs"
import {
  documentIndexStatus,
  formatWikiPageType,
  formatWikiTaskStatus,
  formatWikiTaskType,
  isWikiLanguage,
  WikiIndexStatus,
  WikiStatus,
} from "./helpers"

const WIKI_LANGUAGE_OPTIONS: OpenPanelsLocale[] = ["en", "zh-CN"]

export function WikiPanel({
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
      <Surface className="op-wiki-panel__surface" variant="default">
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
                <Button
                  aria-label={t`Upload document`}
                  isDisabled={isBusy}
                  isIconOnly
                  onPress={() => fileInputRef.current?.click()}
                  size="sm"
                  variant="ghost"
                >
                  <Upload size={16} />
                </Button>
                <Button
                  aria-label={t`New Markdown`}
                  isDisabled={isBusy}
                  isIconOnly
                  onPress={createRawMarkdown}
                  size="sm"
                  variant="ghost"
                >
                  <FilePlus size={16} />
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
              <Button
                isDisabled={isBusy}
                onPress={createWikiPage}
                size="sm"
                variant="secondary"
              >
                <FilePlus size={15} />
                <span>{t`New Markdown`}</span>
              </Button>
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
              <Select
                aria-label={t`Wiki language`}
                className="op-wiki-language-select"
                isDisabled={isBusy}
                onSelectionChange={(key) => {
                  if (key) {
                    updateWikiLanguage(String(key) as OpenPanelsLocale).catch(
                      () => undefined
                    )
                  }
                }}
                selectedKey={wikiLanguage}
              >
                <Select.Trigger>
                  <Select.Value>
                    {OPENPANELS_LOCALE_LABELS[wikiLanguage]}
                  </Select.Value>
                  <Select.Indicator />
                </Select.Trigger>
                <Select.Popover>
                  <ListBox>
                    {WIKI_LANGUAGE_OPTIONS.map((language) => (
                      <ListBox.Item id={language} key={language}>
                        {OPENPANELS_LOCALE_LABELS[language]}
                      </ListBox.Item>
                    ))}
                  </ListBox>
                </Select.Popover>
              </Select>
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
