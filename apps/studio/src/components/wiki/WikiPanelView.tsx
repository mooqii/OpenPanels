import {
  Button,
  Checkbox,
  Dropdown,
  Header,
  Label,
  Surface,
  Tooltip,
} from "@heroui/react"
import { ChevronDown, PanelLeft, RefreshCw, RotateCcw, X } from "lucide-react"
import {
  latestWritingTaskForDocument,
  writingDocumentStatus,
} from "../../lib/writing"
import { WikiPagesEmpty } from "./DocumentModuleEmpty"
import {
  conversionStatusTaskFilter,
  WikiTaskStatusIcon,
  type WikiTaskStatusKind,
} from "./helpers"
import {
  MyDocumentActions,
  MyDocumentItem,
  MyDocumentsModule,
} from "./MyDocumentsModule"
import { myDocumentConversionDisplay } from "./my-document-display"
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
    setPendingDeleteMyDocument,
    isBusy,
    retryingMyDocumentId,
    myDocumentRetryError,
    isSelectionBusy,
    agentSelection,
    isMyDocumentDragActive,
    isDocumentLibraryOpen,
    setIsDocumentLibraryOpen,
    collapsedModules,
    toggleModule,
    myDocumentFileInputRef,
    wikiPageTree,
    moduleHeaderToggle,
    moduleInfo,
    renderWikiPageNodes,
    updateAgentSelection,
    openMyDocument,
    openMyDocumentOriginal,
    createMyDocumentMarkdownDocument,
    addMyDocumentFiles,
    handleMyDocumentDragEnter,
    handleMyDocumentDragOver,
    handleMyDocumentDragLeave,
    handleMyDocumentDrop,
    publishMyDocument,
    retryMyDocument,
    displayedMyDocuments,
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
            const myDocumentsModule = (
              <MyDocumentsModule
                addFiles={addMyDocumentFiles}
                createDocument={createMyDocumentMarkdownDocument}
                fileInputRef={myDocumentFileInputRef}
                isBusy={isBusy}
                isCollapsed={writing && collapsedModules.has("myDocuments")}
                isDragActive={isMyDocumentDragActive}
                isEmpty={displayedMyDocuments.length === 0}
                onDragEnter={handleMyDocumentDragEnter}
                onDragLeave={handleMyDocumentDragLeave}
                onDragOver={handleMyDocumentDragOver}
                onDrop={handleMyDocumentDrop}
                onToggle={
                  writing ? () => toggleModule("myDocuments") : undefined
                }
              >
                {displayedMyDocuments.map((document) => {
                  const writingTask = writing
                    ? latestWritingTaskForDocument(writing.tasks, document)
                    : null
                  const writingStatus = writingDocumentStatus(writingTask)
                  const isWriting = document.writeOperation !== undefined
                  const conversion = myDocumentConversionDisplay(document)
                  const conversionFailed = conversion.isFailed
                  const isContentLocked = isWriting || conversion.isLocked
                  const isWritingLocked =
                    writingStatus === "pending_create" ||
                    writingStatus === "pending_revise" ||
                    writingStatus === "active"
                  const displayTitle = document.title.trim() || t`Untitled`
                  const taskStatus: {
                    doneIcon?: "check" | "sparkles"
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
                    : document.writeOperation
                      ? {
                          doneIcon: "sparkles",
                          filter: "active",
                          kind: "running",
                          label: t`Writing`,
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
                                    : document.conversion.status === "cancelled"
                                      ? "cancelled"
                                      : "done",
                            label:
                              document.conversion.status === "converting"
                                ? t`Converting`
                                : document.conversion.status === "queued"
                                  ? t`Pending conversion`
                                  : document.conversion.status === "failed"
                                    ? t`Conversion failed`
                                    : document.conversion.status === "cancelled"
                                      ? t`Conversion cancelled`
                                      : t`Succeeded`,
                            taskId: document.conversion.taskId,
                          }
                        : null
                  return (
                    <MyDocumentItem
                      document={document}
                      isOpenDisabled={
                        isContentLocked || isWritingLocked || conversionFailed
                      }
                      key={document.id}
                      leading={
                        <Checkbox
                          aria-label={`${t`Select for agent context`}: ${displayTitle}`}
                          className="op-wiki-selection-checkbox op-wiki-selection-checkbox--document"
                          isDisabled={
                            isSelectionBusy ||
                            isWritingLocked ||
                            isContentLocked ||
                            conversionFailed
                          }
                          isSelected={agentSelection.selectedMyDocumentIds.includes(
                            document.id
                          )}
                          onChange={(isSelected) => {
                            const selectedMyDocumentIds = isSelected
                              ? [
                                  ...agentSelection.selectedMyDocumentIds,
                                  document.id,
                                ]
                              : agentSelection.selectedMyDocumentIds.filter(
                                  (documentId) => documentId !== document.id
                                )
                            updateAgentSelection({
                              ...agentSelection,
                              selectedMyDocumentIds,
                            }).catch((error) => {
                              console.error(
                                "Failed to update My Document selection",
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
                      }
                      onOpen={() => {
                        openMyDocument(document).catch((error) => {
                          console.error("Failed to open My Document", error)
                        })
                      }}
                      onOpenOriginal={
                        document.importSource
                          ? () => openMyDocumentOriginal(document)
                          : undefined
                      }
                      status={
                        taskStatus ? (
                          <WikiTaskStatusIcon
                            doneIcon={taskStatus.doneIcon}
                            filter={taskStatus.filter}
                            kind={taskStatus.kind}
                            label={taskStatus.label}
                            onOpenTasks={onOpenAgentTasks}
                            taskId={taskStatus.taskId}
                          />
                        ) : null
                      }
                      transport={transport}
                    >
                      {writingStatus === "failed" || conversionFailed ? (
                        <Tooltip closeDelay={0} delay={0}>
                          <Button
                            aria-label={
                              conversionFailed
                                ? t`Conversion failed. Click to retry`
                                : t`My Document write failed. Click to retry`
                            }
                            className="op-my-document-retry"
                            isIconOnly
                            onPress={() =>
                              retryMyDocument(document, writingTask)
                            }
                            size="sm"
                            variant="secondary"
                          >
                            {retryingMyDocumentId === document.id ? (
                              <RefreshCw className="op-wiki-spin" size={14} />
                            ) : (
                              <RotateCcw size={14} />
                            )}
                          </Button>
                          <Tooltip.Content placement="top" shouldFlip={false}>
                            {myDocumentRetryError === document.id
                              ? t`Retry failed. Ask the Agent to try again.`
                              : conversionFailed
                                ? t`Conversion failed. Click to retry`
                                : t`My Document write failed. Click to retry`}
                          </Tooltip.Content>
                        </Tooltip>
                      ) : null}
                      <MyDocumentActions
                        document={document}
                        isBusy={isBusy}
                        isContentLocked={isContentLocked || conversionFailed}
                        isWritingLocked={isWritingLocked}
                        onDelete={() => setPendingDeleteMyDocument(document)}
                        onPublish={() => publishMyDocument(document)}
                      />
                    </MyDocumentItem>
                  )
                })}
              </MyDocumentsModule>
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
                    {myDocumentsModule}
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
                  {myDocumentsModule}
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
              documents={state.myDocuments}
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
