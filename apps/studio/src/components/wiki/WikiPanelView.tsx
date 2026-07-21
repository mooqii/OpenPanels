import {
  Button,
  Checkbox,
  Chip,
  Dropdown,
  Header,
  Label,
  Separator,
  Surface,
  Tooltip,
} from "@heroui/react"
import {
  ChevronDown,
  ExternalLink,
  Eye,
  FileOutput,
  FilePlus2,
  FileUp,
  FolderOpen,
  MoreHorizontal,
  PanelLeft,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react"
import { originalPreviewKind } from "../../lib/api"
import {
  latestWritingTaskForDocument,
  writingDocumentStatus,
} from "../../lib/writing"
import {
  GeneratedDocumentsEmpty,
  RawDocumentsEmpty,
  WikiPagesEmpty,
} from "./DocumentModuleEmpty"
import { GeneratedDocumentMeta } from "./GeneratedDocumentMeta"
import { documentIndexStatus, WikiIndexStatus, WikiStatus } from "./helpers"
import { RawDocumentMeta } from "./RawDocumentMeta"
import type {
  ReturnTypeOfWikiPanelController,
  WikiPanelProps,
} from "./useWikiPanelController"
import { WikiDialogsLayer } from "./WikiDialogsLayer"
import { WritingComposer } from "./WritingComposer"

export function WikiPanelView(
  props: WikiPanelProps & { controller: ReturnTypeOfWikiPanelController }
) {
  const {
    chromeContent,
    onOpenAgentTasks,
    onManageSkills,
    onReload,
    skillsRevision,
    state,
    transport,
    writing,
  } = props
  const {
    t,
    activeSpace,
    wikiAgentSkillId,
    agentSkills,
    setPendingWikiAgentSkillId,
    setPendingDeleteDocument,
    setPendingDeleteGeneratedDocument,
    setPendingRenameRawDocument,
    setPendingRenameGeneratedDocument,
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
    extractMarkdown,
    reindexDocument,
    revealOriginal,
    openOriginalInNewWindow,
    openRawOriginal,
    addFiles,
    createRawMarkdownDocument,
    handleRawDragEnter,
    handleRawDragOver,
    handleRawDragLeave,
    handleRawDrop,
    openGeneratedDocument,
    createGeneratedMarkdownDocument,
    publishGeneratedDocument,
    retryGeneratedDocument,
    displayedGeneratedDocuments,
  } = props.controller
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
                    <Dropdown>
                      <Button
                        aria-label={t`Add document`}
                        isDisabled={isBusy}
                        isIconOnly
                        size="sm"
                        variant="ghost"
                      >
                        <Plus size={16} />
                      </Button>
                      <Dropdown.Popover placement="bottom end">
                        <Dropdown.Menu
                          aria-label={t`Add document`}
                          onAction={(key) => {
                            if (key === "add-file") {
                              fileInputRef.current?.click()
                              return
                            }
                            if (key === "new-document") {
                              createRawMarkdownDocument().catch((error) => {
                                console.error(
                                  "Failed to create raw Markdown document",
                                  error
                                )
                              })
                            }
                          }}
                        >
                          <Dropdown.Item id="add-file" textValue={t`Add file`}>
                            <FileUp size={15} />
                            <Label>{t`Add file`}</Label>
                          </Dropdown.Item>
                          <Dropdown.Item
                            id="new-document"
                            textValue={t`New document`}
                          >
                            <FilePlus2 size={15} />
                            <Label>{t`New document`}</Label>
                          </Dropdown.Item>
                        </Dropdown.Menu>
                      </Dropdown.Popover>
                    </Dropdown>
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
                              <Button
                                aria-label={t`Document actions`}
                                isIconOnly
                                size="sm"
                                variant="ghost"
                              >
                                <MoreHorizontal size={16} />
                              </Button>
                              <Dropdown.Popover>
                                <Dropdown.Menu
                                  disabledKeys={[
                                    ...(isBusy
                                      ? [
                                          "preview",
                                          "open",
                                          "reveal",
                                          "sync",
                                          "rename",
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
                                      case "rename":
                                        setPendingRenameRawDocument(document)
                                        break
                                      case "delete":
                                        setPendingDeleteDocument(document)
                                        break
                                      default:
                                        break
                                    }
                                  }}
                                >
                                  <Dropdown.Item
                                    id="preview"
                                    textValue={t`Preview original file`}
                                  >
                                    <Eye size={14} />
                                    <Label>{t`Preview original file`}</Label>
                                  </Dropdown.Item>
                                  <Dropdown.Item
                                    id="open"
                                    textValue={t`Open in new window`}
                                  >
                                    <ExternalLink size={14} />
                                    <Label>{t`Open in new window`}</Label>
                                  </Dropdown.Item>
                                  <Dropdown.Item
                                    id="reveal"
                                    textValue={t`Show in folder`}
                                  >
                                    <FolderOpen size={14} />
                                    <Label>{t`Show in folder`}</Label>
                                  </Dropdown.Item>
                                  <Dropdown.Item
                                    id="sync"
                                    textValue={
                                      hasMarkdown ? t`Reindex` : t`Re-extract`
                                    }
                                  >
                                    <RefreshCw size={14} />
                                    <Label>
                                      {hasMarkdown ? t`Reindex` : t`Re-extract`}
                                    </Label>
                                  </Dropdown.Item>
                                  <Dropdown.Item
                                    id="rename"
                                    textValue={t`Rename`}
                                  >
                                    <Pencil size={14} />
                                    <Label>{t`Rename`}</Label>
                                  </Dropdown.Item>
                                  <Separator />
                                  <Dropdown.Item
                                    id="delete"
                                    textValue={t`Delete`}
                                    variant="danger"
                                  >
                                    <Trash2 size={14} />
                                    <Label>{t`Delete`}</Label>
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
                  <div className="op-wiki-actions">
                    <Button
                      aria-label={t`New document`}
                      className="op-wiki-add-button"
                      isDisabled={isBusy}
                      isIconOnly
                      onPress={() => {
                        createGeneratedMarkdownDocument().catch((error) => {
                          console.error(
                            "Failed to create generated Markdown document",
                            error
                          )
                        })
                      }}
                      size="sm"
                      variant="ghost"
                    >
                      <FilePlus2 size={16} />
                    </Button>
                  </div>
                </div>
                <div
                  className={
                    displayedGeneratedDocuments.length === 0
                      ? "op-wiki-list op-wiki-column__content op-wiki-list--empty"
                      : "op-wiki-list op-wiki-column__content"
                  }
                >
                  {displayedGeneratedDocuments.length ? (
                    displayedGeneratedDocuments.map((document) => {
                      const writingTask = writing
                        ? latestWritingTaskForDocument(writing.tasks, document)
                        : null
                      const writingStatus = writingDocumentStatus(writingTask)
                      const isGenerating =
                        document.generation?.status === "generating"
                      const generationFailed =
                        document.generation?.status === "failed"
                      const isWritingLocked =
                        writingStatus === "pending_create" ||
                        writingStatus === "pending_revise" ||
                        writingStatus === "active"
                      const displayTitle = document.title.trim() || t`Untitled`
                      return (
                        <div
                          className="op-wiki-list-item op-wiki-list-item--interactive"
                          key={document.id}
                        >
                          <Checkbox
                            aria-label={`${t`Select for agent context`}: ${displayTitle}`}
                            className="op-wiki-selection-checkbox op-wiki-selection-checkbox--document"
                            isDisabled={isSelectionBusy || isWritingLocked}
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
                            disabled={isGenerating || isWritingLocked}
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
                                {displayTitle}
                              </strong>
                              <GeneratedDocumentMeta
                                apiBase={transport.apiBase}
                                document={document}
                              />
                            </div>
                          </button>
                          <div className="op-wiki-list-item__tools">
                            {writingStatus ? (
                              <Chip
                                className="op-generated-document-task-status"
                                color={
                                  writingStatus === "failed"
                                    ? "danger"
                                    : writingStatus === "active"
                                      ? "accent"
                                      : "warning"
                                }
                                size="sm"
                                variant="soft"
                              >
                                {writingStatus === "pending_create"
                                  ? t`Pending creation`
                                  : writingStatus === "pending_revise"
                                    ? t`Pending revision`
                                    : writingStatus === "active"
                                      ? t`In progress`
                                      : t`Failed`}
                              </Chip>
                            ) : isGenerating ? (
                              <span className="op-generated-document-status">
                                <RefreshCw className="op-wiki-spin" size={14} />
                                {t`Generating`}
                              </span>
                            ) : null}
                            {writingStatus === "failed" || generationFailed ? (
                              <Tooltip closeDelay={0} delay={0}>
                                <Button
                                  aria-label={t`Generation failed. Click to retry`}
                                  className="op-generated-document-retry"
                                  isIconOnly
                                  onPress={() =>
                                    retryGeneratedDocument(
                                      document,
                                      writingTask
                                    )
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
                              <Button
                                aria-label={t`Document actions`}
                                isDisabled={isBusy || isWritingLocked}
                                isIconOnly
                                size="sm"
                                variant="ghost"
                              >
                                <MoreHorizontal size={16} />
                              </Button>
                              <Dropdown.Popover>
                                <Dropdown.Menu
                                  disabledKeys={[
                                    ...(isBusy || isWritingLocked
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
                                  <Dropdown.Item
                                    id="publish"
                                    textValue={
                                      document.publishHistory.length
                                        ? t`Add latest version to raw documents`
                                        : t`Add to raw documents`
                                    }
                                  >
                                    <FileOutput size={14} />
                                    <Label>
                                      {document.publishHistory.length
                                        ? t`Add latest version to raw documents`
                                        : t`Add to raw documents`}
                                    </Label>
                                  </Dropdown.Item>
                                  <Dropdown.Item
                                    id="rename"
                                    textValue={t`Rename`}
                                  >
                                    <Pencil size={14} />
                                    <Label>{t`Rename`}</Label>
                                  </Dropdown.Item>
                                  <Separator />
                                  <Dropdown.Item
                                    id="delete"
                                    textValue={t`Delete`}
                                    variant="danger"
                                  >
                                    <Trash2 size={14} />
                                    <Label>{t`Delete`}</Label>
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
                <div
                  className={
                    wikiPageTree.length === 0
                      ? "op-wiki-page-tree op-wiki-page-tree--empty op-wiki-column__content"
                      : "op-wiki-page-tree op-wiki-column__content"
                  }
                >
                  {wikiPageTree.length ? (
                    renderWikiPageNodes(wikiPageTree)
                  ) : (
                    <WikiPagesEmpty />
                  )}
                </div>
                <div className="op-wiki-agent-skill-footer">
                  <Dropdown>
                    <Button
                      className="op-wiki-agent-skill-trigger"
                      isDisabled={isBusy || agentSkills.length === 0}
                      size="sm"
                      variant="ghost"
                    >
                      <span>
                        {agentSkills.find(
                          (item) => item.skill.id === wikiAgentSkillId
                        )?.skill.name ?? wikiAgentSkillId}
                      </span>
                      <ChevronDown size={14} />
                    </Button>
                    <Dropdown.Popover
                      className="op-wiki-agent-skill-popover"
                      placement="top start"
                      shouldFlip={false}
                    >
                      <Dropdown.Menu
                        aria-label={t`Wiki generation method`}
                        onAction={(key) => {
                          const nextSkillId = String(key)
                          if (nextSkillId !== wikiAgentSkillId) {
                            setPendingWikiAgentSkillId(nextSkillId)
                          }
                        }}
                        selectedKeys={[wikiAgentSkillId]}
                        selectionMode="single"
                      >
                        <Dropdown.Section>
                          <Header>{t`Wiki generation method`}</Header>
                          {agentSkills.map((item) => (
                            <Dropdown.Item
                              id={item.skill.id}
                              key={item.skill.id}
                              textValue={item.skill.name}
                            >
                              <Dropdown.ItemIndicator />
                              <Label>{item.skill.name}</Label>
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
                    {generatedDocumentsModule}
                    {structuredWikiModule}
                    {rawDocumentsModule}
                  </div>
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
              onManageSkills={onManageSkills}
              onOpenAgentTasks={onOpenAgentTasks}
              onOpenLibrary={() => setIsDocumentLibraryOpen(true)}
              onReload={onReload}
              rawDocuments={state.rawDocuments}
              selection={agentSelection}
              skillsRevision={skillsRevision}
              state={writing.state}
              tasks={writing.tasks}
              transport={transport}
            />
          ) : null}
        </div>
      </Surface>

      <WikiDialogsLayer controller={props.controller} transport={transport} />
    </section>
  )
}
