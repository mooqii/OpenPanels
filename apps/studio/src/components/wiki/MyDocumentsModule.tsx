import { Button, Dropdown, Label, Tooltip } from "@heroui/react"
import { FilePlus2, FileText, FileUp, Info, Plus } from "lucide-react"
import type { DragEventHandler, ReactNode, RefObject } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type { MyDocument, MyOpenPanelsTransport } from "../../types"
import { MyDocumentsEmpty } from "./DocumentModuleEmpty"
import { MyDocumentMeta } from "./MyDocumentMeta"

export function MyDocumentsModule({
  addFiles,
  children,
  className = "",
  createDocument,
  fileInputRef,
  isBusy,
  isCollapsed = false,
  isDragActive,
  isEmpty,
  onDragEnter,
  onDragLeave,
  onDragOver,
  onDrop,
  onToggle,
}: {
  addFiles: (files: FileList | null) => Promise<void>
  children: ReactNode
  className?: string
  createDocument: () => Promise<void>
  fileInputRef: RefObject<HTMLInputElement | null>
  isBusy: boolean
  isCollapsed?: boolean
  isDragActive: boolean
  isEmpty: boolean
  onDragEnter: DragEventHandler<HTMLElement>
  onDragLeave: DragEventHandler<HTMLElement>
  onDragOver: DragEventHandler<HTMLElement>
  onDrop: DragEventHandler<HTMLElement>
  onToggle?: () => void
}) {
  const { t } = useMyOpenPanelsI18n()
  const title = t`My Documents`
  const classes = [
    "op-my-documents-module",
    "op-wiki-column",
    "op-wiki-column--my-documents",
    className,
    onToggle && isCollapsed ? "op-wiki-column--collapsed" : "",
    isDragActive ? "op-wiki-column--drop-active" : "",
  ]
    .filter(Boolean)
    .join(" ")

  return (
    <section
      className={classes}
      onDragEnter={onDragEnter}
      onDragLeave={onDragLeave}
      onDragOver={onDragOver}
      onDrop={onDrop}
    >
      <div className="op-wiki-drop-hint">{t`Drop files to upload`}</div>
      <div className="op-wiki-column__header">
        {onToggle ? (
          <button
            aria-expanded={!isCollapsed}
            aria-label={`${isCollapsed ? t`Expand module` : t`Collapse module`}: ${title}`}
            className="op-wiki-column__header-toggle op-wiki-column__header-toggle--accordion"
            onClick={onToggle}
            type="button"
          />
        ) : null}
        <div className="op-wiki-column__title">
          <h2>{title}</h2>
          <Tooltip closeDelay={0} delay={0}>
            <Button
              aria-label={`${t`About module`}: ${title}`}
              className="op-wiki-module-info"
              isIconOnly
              size="sm"
              variant="ghost"
            >
              <Info size={15} />
            </Button>
            <Tooltip.Content
              className="op-wiki-module-tooltip"
              placement="bottom"
            >
              {t`Documents you add or create with agents live here. Imported files are converted when needed without changing the Wiki. Selecting a document lets the agent discover it and load its latest content.`}
            </Tooltip.Content>
          </Tooltip>
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
                    fileInputRef.current?.click()
                    return
                  }
                  if (key === "new-document") {
                    createDocument().catch((error) => {
                      console.error("Failed to create My Document", error)
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
        </div>
        <input
          hidden
          multiple
          onChange={(event) => {
            addFiles(event.currentTarget.files).catch((error) => {
              console.error("Failed to add My Documents", error)
            })
            event.currentTarget.value = ""
          }}
          ref={fileInputRef}
          type="file"
        />
      </div>
      <div
        className={
          isEmpty
            ? "op-wiki-list op-wiki-column__content op-wiki-list--empty"
            : "op-wiki-list op-wiki-column__content"
        }
      >
        {isEmpty ? <MyDocumentsEmpty /> : children}
      </div>
    </section>
  )
}

export function MyDocumentItem({
  children,
  className = "",
  document,
  isOpenDisabled = false,
  leading,
  onOpen,
  onOpenOriginal,
  status,
  transport,
}: {
  children: ReactNode
  className?: string
  document: MyDocument
  isOpenDisabled?: boolean
  leading?: ReactNode
  onOpen: () => void
  onOpenOriginal?: () => void
  status?: ReactNode
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const displayTitle = document.title.trim() || t`Untitled`

  return (
    <div
      className={`op-my-document-item op-wiki-list-item op-wiki-list-item--interactive ${className}`.trim()}
    >
      {leading ?? (
        <span className="op-my-document-item__icon">
          <FileText size={16} />
        </span>
      )}
      <div className="op-wiki-list-item__body">
        <button
          aria-label={displayTitle}
          className="op-my-document-open"
          disabled={isOpenDisabled}
          onClick={onOpen}
          type="button"
        />
        <div className="op-my-document-copy">
          <strong className="op-wiki-list-item__title">{displayTitle}</strong>
          <MyDocumentMeta
            apiBase={transport.apiBase}
            document={document}
            onOpenOriginal={onOpenOriginal}
            status={status}
          />
        </div>
      </div>
      <div className="op-wiki-list-item__tools">{children}</div>
    </div>
  )
}
