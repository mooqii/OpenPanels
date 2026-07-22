import {
  Button,
  Checkbox,
  Dropdown,
  Header,
  Label,
  Separator,
  Surface,
  Tooltip,
} from "@heroui/react"
import {
  ChevronDown,
  FileOutput,
  FilePlus2,
  FileUp,
  MoreHorizontal,
  PanelLeft,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react"
import {
  latestWritingTaskForDocument,
  writingDocumentStatus,
} from "../../lib/writing"
import { GeneratedDocumentsEmpty, WikiPagesEmpty } from "./DocumentModuleEmpty"
import { GeneratedDocumentMeta } from "./GeneratedDocumentMeta"
import { generatedDocumentConversionDisplay } from "./generated-document-display"
import {
  conversionStatusTaskFilter,
  WikiTaskStatusIcon,
  type WikiTaskStatusKind,
} from "./helpers"
import { RawDocumentsModule } from "./RawDocumentsModule"
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
    setPendingDeleteGeneratedDocument,
    setPendingRenameGeneratedDocument,
    isBusy,
    retryingGeneratedDocumentId,
    generatedDocumentRetryError,
    isSelectionBusy,
    agentSelection,
    isGeneratedDragActive,
    isDocumentLibraryOpen,
    setIsDocumentLibraryOpen,
    collapsedModules,
    generatedFileInputRef,
    wikiPageTree,
    moduleHeaderToggle,
    moduleInfo,
    renderWikiPageNodes,
    updateAgentSelection,
    openGeneratedDocument,
    openGeneratedOriginal,
    createGeneratedMarkdownDocument,
    addGeneratedFiles,
    handleGeneratedDragEnter,
    handleGeneratedDragOver,
    handleGeneratedDragLeave,
    handleGeneratedDrop,
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
            const rawDocumentsModule = writing ? null : (
              <RawDocumentsModule
                controller={props.controller}
                onOpenAgentTasks={onOpenAgentTasks}
                state={state}
              />
            )
            const generatedDocumentsModule = (
              <section
                className={
                  writing && collapsedModules.has("generated")
                    ? "op-wiki-column--collapsed op-wiki-column op-wiki-column--generated"
                    : isGeneratedDragActive
                      ? "op-wiki-column op-wiki-column--generated op-wiki-column--drop-active"
                      : "op-wiki-column op-wiki-column--generated"
                }
                onDragEnter={handleGeneratedDragEnter}
                onDragLeave={handleGeneratedDragLeave}
                onDragOver={handleGeneratedDragOver}
                onDrop={handleGeneratedDrop}
              >
                <div className="op-wiki-drop-hint">{t`Drop files to upload`}</div>
                <div className="op-wiki-column__header">
                  {writing
                    ? moduleHeaderToggle("generated", t`My Documents`)
                    : null}
                  <div className="op-wiki-column__title">
                    <h2>{t`My Documents`}</h2>
                    {moduleInfo(
                      t`My Documents`,
                      t`Documents you add or create with agents live here. Imported files are converted when needed without changing the Wiki. Selecting a document lets the agent discover it and load its latest content.`
                    )}
                  </div>
                  <div className="op-wiki-actions">
                    <Dropdown>
                      <Button
                        aria-label={t`Add document`}
                        className="op-wiki-add-button"
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
                              generatedFileInputRef.current?.click()
                              return
                            }
                            if (key === "new-document") {
                              createGeneratedMarkdownDocument().catch(
                                (error) => {
                                  console.error(
                                    "Failed to create generated Markdown document",
                                    error
                                  )
                                }
                              )
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
                      addGeneratedFiles(event.currentTarget.files)
                      event.currentTarget.value = ""
                    }}
                    ref={generatedFileInputRef}
                    type="file"
                  />
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
                      const conversion =
                        generatedDocumentConversionDisplay(document)
                      const conversionFailed = conversion.isFailed
                      const isContentLocked =
                        isGenerating || conversion.isLocked
                      const isWritingLocked =
                        writingStatus === "pending_create" ||
                        writingStatus === "pending_revise" ||
                        writingStatus === "active"
                      const displayTitle = document.title.trim() || t`Untitled`
                      const taskStatus: {
                        filter: "active" | "done" | "pending"
                        kind: WikiTaskStatusKind
                        label: string
                        taskId: string | null | undefined
                      } | null = writingStatus
                        ? {
                            filter:
                              writingStatus === "active" ? "active" : "pending",
                            kind:
                              writingStatus === "active"
                                ? "running"
                                : writingStatus === "failed"
                                  ? "failed"
                                  : "pending",
                            label:
                              writingStatus === "pending_create"
                                ? t`Pending creation`
                                : writingStatus === "pending_revise"
                                  ? t`Pending revision`
                                  : writingStatus === "active"
                                    ? t`In progress`
                                    : t`Failed`,
                            taskId: writingTask?.id,
                          }
                        : document.generation
                          ? {
                              filter:
                                document.generation.status === "generating"
                                  ? "active"
                                  : document.generation.status === "failed"
                                    ? "pending"
                                    : "done",
                              kind:
                                document.generation.status === "generating"
                                  ? "running"
                                  : document.generation.status === "failed"
                                    ? "failed"
                                    : "done",
                              label:
                                document.generation.status === "generating"
                                  ? t`Generating`
                                  : document.generation.status === "failed"
                                    ? t`Failed`
                                    : t`Succeeded`,
                              taskId: document.taskId,
                            }
                          : document.conversion &&
                              !["not_required", "ready"].includes(
                                document.conversion.status
                              )
                            ? {
                                filter: conversionStatusTaskFilter(
                                  document.conversion.status
                                ),
                                kind:
                                  document.conversion.status === "converting"
                                    ? "running"
                                    : document.conversion.status === "queued"
                                      ? "pending"
                                      : document.conversion.status === "failed"
                                        ? "failed"
                                        : document.conversion.status ===
                                            "cancelled"
                                          ? "cancelled"
                                          : "done",
                                label:
                                  document.conversion.status === "converting"
                                    ? t`Converting`
                                    : document.conversion.status === "queued"
                                      ? t`Pending conversion`
                                      : document.conversion.status === "failed"
                                        ? t`Conversion failed`
                                        : document.conversion.status ===
                                            "cancelled"
                                          ? t`Conversion cancelled`
                                          : t`Succeeded`,
                                taskId: document.conversion.taskId,
                              }
                            : null
                      return (
                        <div
                          className="op-wiki-list-item op-wiki-list-item--interactive"
                          key={document.id}
                        >
                          <Checkbox
                            aria-label={`${t`Select for agent context`}: ${displayTitle}`}
                            className="op-wiki-selection-checkbox op-wiki-selection-checkbox--document"
                            isDisabled={
                              isSelectionBusy ||
                              isWritingLocked ||
                              isContentLocked ||
                              conversionFailed
                            }
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
                          <div className="op-wiki-list-item__body">
                            <button
                              aria-label={displayTitle}
                              className="op-generated-document-open"
                              disabled={
                                isContentLocked ||
                                isWritingLocked ||
                                conversionFailed
                              }
                              onClick={() => {
                                openGeneratedDocument(document).catch(
                                  (error) => {
                                    console.error(
                                      "Failed to open generated document",
                                      error
                                    )
                                  }
                                )
                              }}
                              type="button"
                            />
                            <div className="op-generated-document-copy">
                              <strong className="op-wiki-list-item__title">
                                {displayTitle}
                              </strong>
                              <GeneratedDocumentMeta
                                apiBase={transport.apiBase}
                                document={document}
                                onOpenOriginal={
                                  document.importSource
                                    ? () => openGeneratedOriginal(document)
                                    : undefined
                                }
                                status={
                                  taskStatus ? (
                                    <WikiTaskStatusIcon
                                      filter={taskStatus.filter}
                                      kind={taskStatus.kind}
                                      label={taskStatus.label}
                                      onOpenTasks={onOpenAgentTasks}
                                      taskId={taskStatus.taskId}
                                    />
                                  ) : null
                                }
                              />
                            </div>
                          </div>
                          <div className="op-wiki-list-item__tools">
                            {writingStatus === "failed" ||
                            generationFailed ||
                            conversionFailed ? (
                              <Tooltip closeDelay={0} delay={0}>
                                <Button
                                  aria-label={
                                    conversionFailed
                                      ? t`Conversion failed. Click to retry`
                                      : t`Generation failed. Click to retry`
                                  }
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
                                    ? t`Retry failed. Ask the Agent to try again.`
                                    : conversionFailed
                                      ? t`Conversion failed. Click to retry`
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
                                    ...(isContentLocked || conversionFailed
                                      ? ["publish", "rename"]
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
                                  {writing ? null : (
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
                                  )}
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
                  writing && collapsedModules.has("structured")
                    ? "op-wiki-column--collapsed op-wiki-column op-wiki-column--structured"
                    : "op-wiki-column op-wiki-column--structured"
                }
              >
                <div className="op-wiki-column__header">
                  {writing
                    ? moduleHeaderToggle(
                        "structured",
                        activeSpace?.title || t`Wiki`
                      )
                    : null}
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
                    {writing ? (
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
                    ) : null}
                    <h2>
                      {activeSpace?.title ? t(activeSpace.title) : t`Wiki`}
                    </h2>
                    {moduleInfo(
                      activeSpace?.title || t`Wiki`,
                      writing
                        ? t`Structured knowledge pages generated from your sources live here. Agents can search and update this Wiki. Selecting it lets the agent discover the Wiki and load relevant pages when needed.`
                        : t`Structured knowledge pages generated from your sources live here. Agents can search and update this Wiki.`
                    )}
                  </div>
                  <div className="op-wiki-actions">
                    {writing ? null : (
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
                          placement="bottom end"
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
                    <WikiPagesEmpty mentionRawDocuments={!writing} />
                  )}
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
                  {generatedDocumentsModule}
                </div>
                <div className="op-wiki-knowledge-stack">
                  {structuredWikiModule}
                  {rawDocumentsModule}
                </div>
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
              selection={agentSelection}
              skillsRevision={skillsRevision}
              state={writing.state}
              tasks={writing.tasks}
              transport={transport}
            />
          ) : null}
        </div>
      </Surface>

      <WikiDialogsLayer
        controller={props.controller}
        mentionRawDocuments={!writing}
      />
    </section>
  )
}
