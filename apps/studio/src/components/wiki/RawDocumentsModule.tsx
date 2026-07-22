import { Button, Dropdown, Label, Separator } from "@heroui/react"
import {
  ExternalLink,
  Eye,
  FilePlus2,
  FileUp,
  MoreHorizontal,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react"
import { originalPreviewKind } from "../../lib/api"
import { RawDocumentsEmpty } from "./DocumentModuleEmpty"
import { documentIndexStatus, WikiIndexStatus, WikiStatus } from "./helpers"
import { RawDocumentMeta } from "./RawDocumentMeta"
import type {
  ReturnTypeOfWikiPanelController,
  WikiPanelProps,
} from "./useWikiPanelController"

interface RawDocumentsModuleProps {
  controller: ReturnTypeOfWikiPanelController
  onOpenAgentTasks: WikiPanelProps["onOpenAgentTasks"]
  state: WikiPanelProps["state"]
}

export function RawDocumentsModule({
  controller,
  onOpenAgentTasks,
  state,
}: RawDocumentsModuleProps) {
  const {
    t,
    activeSpace,
    addFiles,
    createRawMarkdownDocument,
    extractMarkdown,
    fileInputRef,
    handleRawDragEnter,
    handleRawDragLeave,
    handleRawDragOver,
    handleRawDrop,
    isBusy,
    isRawDragActive,
    openMarkdown,
    openOriginalInNewWindow,
    openRawOriginal,
    reindexDocument,
    setPendingDeleteDocument,
    setPendingRenameRawDocument,
  } = controller

  return (
    <section
      className={
        isRawDragActive
          ? "op-wiki-raw-strip op-wiki-column--drop-active"
          : "op-wiki-raw-strip"
      }
      onDragEnter={handleRawDragEnter}
      onDragLeave={handleRawDragLeave}
      onDragOver={handleRawDragOver}
      onDrop={handleRawDrop}
    >
      <div className="op-wiki-drop-hint">{t`Drop files to upload`}</div>
      <div className="op-wiki-raw-strip__header">
        <div className="op-wiki-raw-strip__title">
          <h3>{t`Raw Documents`}</h3>
          {controller.moduleInfo(
            t`Raw Documents`,
            t`Source files live here. Added content is converted to Markdown and indexed into the Wiki.`
          )}
        </div>
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
              <Dropdown.Item id="new-document" textValue={t`New document`}>
                <FilePlus2 size={15} />
                <Label>{t`New document`}</Label>
              </Dropdown.Item>
            </Dropdown.Menu>
          </Dropdown.Popover>
        </Dropdown>
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
            ? "op-wiki-raw-strip__list op-wiki-raw-strip__list--empty"
            : "op-wiki-raw-strip__list"
        }
      >
        {state.rawDocuments.length ? (
          state.rawDocuments.map((document) => {
            const previewKind = originalPreviewKind(document)
            const hasMarkdown = Boolean(document.markdownRef)
            const indexStatus = documentIndexStatus(document, activeSpace?.id)
            return (
              <article className="op-wiki-raw-card" key={document.id}>
                <button
                  aria-label={document.title}
                  className="op-wiki-raw-card__open"
                  onClick={() => {
                    if (hasMarkdown) {
                      openMarkdown(document).catch((error) => {
                        console.error("Failed to open wiki markdown", error)
                      })
                      return
                    }
                    openRawOriginal(document)
                  }}
                  type="button"
                />
                <div className="op-wiki-raw-card__copy">
                  <div className="op-wiki-raw-card__statuses">
                    <WikiIndexStatus
                      onOpenTasks={onOpenAgentTasks}
                      status={indexStatus}
                    />
                  </div>
                  <strong className="op-wiki-raw-card__title">
                    {document.title}
                  </strong>
                  <div className="op-wiki-raw-card__subtitle">
                    <RawDocumentMeta
                      document={document}
                      onOpenOriginal={() => openRawOriginal(document)}
                    />
                    <WikiStatus
                      document={document}
                      onOpenTasks={onOpenAgentTasks}
                    />
                  </div>
                </div>
                <Dropdown>
                  <Button
                    aria-label={t`Document actions`}
                    className="op-wiki-raw-card__menu"
                    isIconOnly
                    size="sm"
                    variant="ghost"
                  >
                    <MoreHorizontal size={15} />
                  </Button>
                  <Dropdown.Popover>
                    <Dropdown.Menu
                      disabledKeys={[
                        ...(isBusy
                          ? ["preview", "open", "sync", "rename", "delete"]
                          : []),
                        ...(previewKind ? [] : ["preview"]),
                      ]}
                      onAction={(key) => {
                        switch (key) {
                          case "preview":
                            openRawOriginal(document)
                            break
                          case "open":
                            openOriginalInNewWindow(document)
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
                        id="sync"
                        textValue={hasMarkdown ? t`Reindex` : t`Re-extract`}
                      >
                        <RefreshCw size={14} />
                        <Label>
                          {hasMarkdown ? t`Reindex` : t`Re-extract`}
                        </Label>
                      </Dropdown.Item>
                      <Dropdown.Item id="rename" textValue={t`Rename`}>
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
              </article>
            )
          })
        ) : (
          <RawDocumentsEmpty />
        )}
      </div>
    </section>
  )
}
