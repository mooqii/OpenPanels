import { AlertDialog, Button, Input, Modal, TextArea } from "@heroui/react"
import {
  AlertCircle,
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  FolderOpen,
  LoaderCircle,
  Minus,
  Pencil,
  Plus,
  Trash2,
  X,
} from "lucide-react"
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import {
  clampImageScale,
  formatBytes,
  originalPreviewKind,
} from "../../lib/api"
import type { WikiOriginalPreviewDocument } from "../../types"

export interface SkillTextFile {
  content: string
  path: string
}

interface SkillFileTreeNode {
  children: SkillFileTreeNode[]
  path: string
  type: "file" | "folder"
}

export function SkillFilesDialog({
  backdropClassName,
  closeLabel,
  files,
  onClose,
  onSave,
  readOnly,
  title,
}: {
  backdropClassName?: string
  closeLabel: string
  files: SkillTextFile[]
  onClose: () => void
  onSave: (path: string, content: string) => Promise<void>
  readOnly: boolean
  title: string
}) {
  const { t } = useMyOpenPanelsI18n()
  const [drafts, setDrafts] = useState(() =>
    Object.fromEntries(files.map((file) => [file.path, file.content]))
  )
  const [selectedPath, setSelectedPath] = useState(
    files.find((file) => file.path === "SKILL.md")?.path ?? files[0]?.path ?? ""
  )
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(
    () => new Set(files.flatMap((file) => parentPaths(file.path)))
  )
  const [saveStatus, setSaveStatus] = useState<
    "idle" | "saving" | "saved" | "error"
  >("idle")
  const latestDraftsRef = useRef(drafts)
  const savedDraftsRef = useRef({ ...drafts })
  const timersRef = useRef(new Map<string, ReturnType<typeof setTimeout>>())
  const savesRef = useRef(new Map<string, Promise<void>>())
  latestDraftsRef.current = drafts

  const saveFile = useCallback(
    (path: string, content: string) => {
      setSaveStatus("saving")
      const previous = savesRef.current.get(path) ?? Promise.resolve()
      const request = previous
        .catch(() => undefined)
        .then(() => onSave(path, content))
        .then(() => {
          savedDraftsRef.current[path] = content
          setSaveStatus("saved")
        })
        .catch(() => setSaveStatus("error"))
      savesRef.current.set(path, request)
      return request
    },
    [onSave]
  )

  const updateFile = useCallback(
    (path: string, content: string) => {
      setDrafts((current) => ({ ...current, [path]: content }))
      setSaveStatus("saving")
      const currentTimer = timersRef.current.get(path)
      if (currentTimer) clearTimeout(currentTimer)
      timersRef.current.set(
        path,
        setTimeout(() => {
          timersRef.current.delete(path)
          saveFile(path, content)
        }, 600)
      )
    },
    [saveFile]
  )

  const closeAfterSave = useCallback(async () => {
    for (const [path, timer] of timersRef.current) {
      clearTimeout(timer)
      timersRef.current.delete(path)
      const content = latestDraftsRef.current[path]
      if (content !== savedDraftsRef.current[path]) saveFile(path, content)
    }
    await Promise.all([...savesRef.current.values()])
    onClose()
  }, [onClose, saveFile])

  const tree = buildSkillFileTree(files)
  return (
    <Modal.Backdrop
      className={backdropClassName}
      isOpen
      onOpenChange={(isOpen) => !isOpen && closeAfterSave()}
    >
      <Modal.Container className="op-markdown-dialog__container" size="cover">
        <Modal.Dialog className="op-markdown-dialog__panel op-skill-files-dialog">
          <Modal.Header className="op-markdown-dialog__header">
            <div className="op-markdown-dialog__filename">
              <Modal.Heading>{title}</Modal.Heading>
            </div>
            <div className="op-markdown-dialog__actions">
              {!readOnly && saveStatus !== "idle" ? (
                <span
                  className="op-markdown-dialog__save-status"
                  data-status={saveStatus}
                >
                  {saveStatus === "saving"
                    ? t`Saving`
                    : saveStatus === "saved"
                      ? t`Saved`
                      : t`Save failed`}
                </span>
              ) : null}
              <Button
                aria-label={closeLabel}
                className="op-markdown-dialog__close"
                isIconOnly
                onPress={() => closeAfterSave()}
                size="md"
                variant="ghost"
              >
                <X size={21} />
              </Button>
            </div>
          </Modal.Header>
          <Modal.Body className="op-skill-files-dialog__body">
            <nav
              aria-label={t`Skill files`}
              className="op-skill-files-dialog__tree"
            >
              {tree.map((node) => (
                <SkillFileNode
                  expandedPaths={expandedPaths}
                  key={node.path}
                  node={node}
                  onSelect={setSelectedPath}
                  onToggle={(path) =>
                    setExpandedPaths((current) => {
                      const next = new Set(current)
                      if (next.has(path)) next.delete(path)
                      else next.add(path)
                      return next
                    })
                  }
                  selectedPath={selectedPath}
                />
              ))}
            </nav>
            <div className="op-skill-files-dialog__editor-pane">
              <TextArea
                aria-label={selectedPath || title}
                className="op-markdown-dialog__editor"
                fullWidth
                onChange={(event) =>
                  updateFile(selectedPath, event.currentTarget.value)
                }
                readOnly={readOnly}
                value={drafts[selectedPath] ?? ""}
                variant="secondary"
              />
            </div>
          </Modal.Body>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}

function SkillFileNode({
  expandedPaths,
  node,
  onSelect,
  onToggle,
  selectedPath,
}: {
  expandedPaths: Set<string>
  node: SkillFileTreeNode
  onSelect: (path: string) => void
  onToggle: (path: string) => void
  selectedPath: string
}) {
  const name = node.path.split("/").at(-1)
  const expanded = expandedPaths.has(node.path)
  if (node.type === "file") {
    return (
      <button
        className="op-skill-files-dialog__tree-item"
        data-selected={selectedPath === node.path}
        onClick={() => onSelect(node.path)}
        type="button"
      >
        <FileText size={14} />
        <span>{name}</span>
      </button>
    )
  }
  return (
    <div className="op-skill-files-dialog__folder">
      <button
        className="op-skill-files-dialog__tree-item"
        onClick={() => onToggle(node.path)}
        type="button"
      >
        {expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        {expanded ? <FolderOpen size={14} /> : <Folder size={14} />}
        <span>{name}</span>
      </button>
      {expanded ? (
        <div className="op-skill-files-dialog__tree-children">
          {node.children.map((child) => (
            <SkillFileNode
              expandedPaths={expandedPaths}
              key={child.path}
              node={child}
              onSelect={onSelect}
              onToggle={onToggle}
              selectedPath={selectedPath}
            />
          ))}
        </div>
      ) : null}
    </div>
  )
}

function parentPaths(path: string): string[] {
  const parts = path.split("/")
  return parts
    .slice(0, -1)
    .map((_, index) => parts.slice(0, index + 1).join("/"))
}

function buildSkillFileTree(files: SkillTextFile[]): SkillFileTreeNode[] {
  const root: SkillFileTreeNode[] = []
  for (const file of files) {
    const parts = file.path.split("/")
    let children = root
    for (let index = 0; index < parts.length; index += 1) {
      const path = parts.slice(0, index + 1).join("/")
      let node = children.find((candidate) => candidate.path === path)
      if (!node) {
        node = {
          children: [],
          path,
          type: index === parts.length - 1 ? "file" : "folder",
        }
        children.push(node)
      }
      children = node.children
    }
  }
  const sort = (nodes: SkillFileTreeNode[]) => {
    nodes.sort((left, right) =>
      left.type === right.type
        ? left.path.localeCompare(right.path)
        : left.type === "folder"
          ? -1
          : 1
    )
    for (const node of nodes) sort(node.children)
  }
  sort(root)
  return root
}

export function MyDocumentDialog({
  closeLabel,
  content,
  onClose,
  title,
  titleLabel,
}: {
  closeLabel: string
  content: string
  onClose: () => void
  title: string
  titleLabel: string
}) {
  return (
    <Modal.Backdrop isOpen onOpenChange={(isOpen) => !isOpen && onClose()}>
      <Modal.Container size="cover">
        <Modal.Dialog className="op-markdown-dialog__panel">
          <Modal.CloseTrigger aria-label={closeLabel} />
          <Modal.Header>
            <div>
              <div className="op-wiki-panel__label">{titleLabel}</div>
              <Modal.Heading>{title}</Modal.Heading>
            </div>
          </Modal.Header>
          <Modal.Body>
            <TextArea
              aria-label={title}
              className="op-markdown-dialog__editor"
              fullWidth
              readOnly
              value={content}
              variant="secondary"
            />
          </Modal.Body>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}

export function RenameDocumentDialog({
  cancelLabel,
  confirmLabel,
  isBusy,
  onCancel,
  onConfirm,
  title,
  value,
}: {
  cancelLabel: string
  confirmLabel: string
  isBusy: boolean
  onCancel: () => void
  onConfirm: (title: string) => void
  title: string
  value: string
}) {
  const [nextTitle, setNextTitle] = useState(value)
  return (
    <Modal.Backdrop isOpen onOpenChange={(isOpen) => !isOpen && onCancel()}>
      <Modal.Container>
        <Modal.Dialog>
          <Modal.Header>
            <Modal.Heading>{title}</Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <Input
              aria-label={title}
              autoFocus
              onChange={(event) => setNextTitle(event.currentTarget.value)}
              value={nextTitle}
            />
          </Modal.Body>
          <Modal.Footer>
            <Button isDisabled={isBusy} onPress={onCancel} variant="tertiary">
              {cancelLabel}
            </Button>
            <Button
              isDisabled={isBusy || !nextTitle.trim()}
              onPress={() => onConfirm(nextTitle.trim())}
              variant="primary"
            >
              {confirmLabel}
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}

export function MarkdownDialog({
  content,
  fileName,
  onChange,
  onClose,
  onRenameFileName,
  onSave,
  primaryAction,
  closeLabel,
}: {
  closeLabel: string
  content: string
  fileName: string
  onChange: (content: string) => void
  onClose: () => void
  onRenameFileName: (fileName: string) => Promise<void>
  onSave: (content: string) => Promise<void>
  primaryAction?: {
    icon?: ReactNode
    isDisabled?: boolean
    label: string
    onPress: (content: string) => void | Promise<void>
  }
}) {
  const { t } = useMyOpenPanelsI18n()
  const [isEditingFileName, setIsEditingFileName] = useState(false)
  const [fileNameDraft, setFileNameDraft] = useState("")
  const [saveStatus, setSaveStatus] = useState<
    "idle" | "saving" | "saved" | "error"
  >("idle")
  const latestContentRef = useRef(content)
  const savedContentRef = useRef(content)
  const contentRevisionRef = useRef(0)
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const saveChainRef = useRef<Promise<void>>(Promise.resolve())
  const cancelFileNameEditRef = useRef(false)

  latestContentRef.current = content

  const fileNameParts = (() => {
    const slashIndex = Math.max(
      fileName.lastIndexOf("/"),
      fileName.lastIndexOf("\\")
    )
    const directory = slashIndex >= 0 ? fileName.slice(0, slashIndex + 1) : ""
    const baseName = fileName.slice(slashIndex + 1)
    const dotIndex = baseName.lastIndexOf(".")
    return {
      directory,
      extension: dotIndex > 0 ? baseName.slice(dotIndex) : "",
      stem: dotIndex > 0 ? baseName.slice(0, dotIndex) : baseName,
    }
  })()

  const queueSave = useCallback(
    (nextContent: string, revision: number) => {
      setSaveStatus("saving")
      const request = saveChainRef.current
        .catch(() => undefined)
        .then(() => onSave(nextContent))
      saveChainRef.current = request
      return request
        .then(() => {
          if (revision !== contentRevisionRef.current) return
          savedContentRef.current = nextContent
          setSaveStatus("saved")
        })
        .catch(() => {
          if (revision === contentRevisionRef.current) {
            setSaveStatus("error")
          }
        })
    },
    [onSave]
  )

  useEffect(() => {
    if (content === savedContentRef.current) return
    contentRevisionRef.current += 1
    const revision = contentRevisionRef.current
    setSaveStatus("saving")
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current)
    saveTimerRef.current = setTimeout(() => {
      saveTimerRef.current = null
      queueSave(content, revision)
    }, 600)
    return () => {
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current)
        saveTimerRef.current = null
      }
    }
  }, [content, queueSave])

  const commitFileName = useCallback(async () => {
    if (cancelFileNameEditRef.current) {
      cancelFileNameEditRef.current = false
      return
    }
    const nextStem = fileNameDraft.trim().replaceAll(/[\\/]/g, "")
    setIsEditingFileName(false)
    if (!nextStem || nextStem === fileNameParts.stem) return
    setSaveStatus("saving")
    try {
      await onRenameFileName(
        `${fileNameParts.directory}${nextStem}${fileNameParts.extension}`
      )
      setSaveStatus("saved")
    } catch {
      setSaveStatus("error")
    }
  }, [fileNameDraft, fileNameParts, onRenameFileName])

  const closeAfterSave = useCallback(async () => {
    if (saveTimerRef.current) {
      clearTimeout(saveTimerRef.current)
      saveTimerRef.current = null
    }
    const latestContent = latestContentRef.current
    if (latestContent !== savedContentRef.current) {
      await queueSave(latestContent, contentRevisionRef.current)
    } else {
      await saveChainRef.current.catch(() => undefined)
    }
    onClose()
  }, [onClose, queueSave])

  return (
    <Modal.Backdrop
      isOpen
      onOpenChange={(isOpen) => {
        if (!isOpen) closeAfterSave()
      }}
    >
      <Modal.Container className="op-markdown-dialog__container" size="cover">
        <Modal.Dialog className="op-markdown-dialog__panel">
          <Modal.Header className="op-markdown-dialog__header">
            <div className="op-markdown-dialog__filename">
              {isEditingFileName ? (
                <div className="op-markdown-dialog__filename-editor">
                  <Input
                    aria-label={t`File name`}
                    autoFocus
                    className="min-w-20 max-w-[55vw] sm:w-105"
                    onBlur={() => commitFileName()}
                    onChange={(event) =>
                      setFileNameDraft(event.currentTarget.value)
                    }
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.currentTarget.blur()
                      } else if (event.key === "Escape") {
                        event.stopPropagation()
                        cancelFileNameEditRef.current = true
                        setIsEditingFileName(false)
                      }
                    }}
                    value={fileNameDraft}
                  />
                  <span>{fileNameParts.extension}</span>
                </div>
              ) : (
                <Modal.Heading>{fileName}</Modal.Heading>
              )}
              {isEditingFileName ? null : (
                <Button
                  aria-label={t`Edit file name`}
                  className="op-markdown-dialog__rename"
                  isIconOnly
                  onPress={() => {
                    cancelFileNameEditRef.current = false
                    setFileNameDraft(fileNameParts.stem)
                    setIsEditingFileName(true)
                  }}
                  size="sm"
                  variant="ghost"
                >
                  <Pencil size={14} />
                </Button>
              )}
            </div>
            <div className="op-markdown-dialog__actions">
              {primaryAction ? (
                <Button
                  isDisabled={primaryAction.isDisabled}
                  onPress={() => {
                    const latestContent = latestContentRef.current
                    closeAfterSave()
                      .then(() => primaryAction.onPress(latestContent))
                      .catch(() => undefined)
                  }}
                  size="sm"
                  variant="primary"
                >
                  {primaryAction.icon}
                  {primaryAction.label}
                </Button>
              ) : null}
              {saveStatus !== "idle" ? (
                <span
                  className="op-markdown-dialog__save-status"
                  data-status={saveStatus}
                >
                  {saveStatus === "saving"
                    ? t`Saving`
                    : saveStatus === "saved"
                      ? t`Saved`
                      : t`Save failed`}
                </span>
              ) : null}
              <Button
                aria-label={closeLabel}
                className="op-markdown-dialog__close"
                isIconOnly
                onPress={() => closeAfterSave()}
                size="md"
                variant="ghost"
              >
                <X size={21} />
              </Button>
            </div>
          </Modal.Header>
          <Modal.Body>
            <TextArea
              aria-label={fileName}
              className="op-markdown-dialog__editor"
              fullWidth
              onChange={(event) => onChange(event.currentTarget.value)}
              value={content}
              variant="secondary"
            />
          </Modal.Body>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}

function ImagePreviewDialog({
  closeLabel,
  document,
  onClose,
  previewUrl,
}: {
  closeLabel: string
  document: WikiOriginalPreviewDocument
  onClose: () => void
  previewUrl: string
}) {
  const { t } = useMyOpenPanelsI18n()
  const [imageScale, setImageScale] = useState(1)
  const zoomPercentage = Math.round(imageScale * 100)

  return (
    <Modal.Backdrop
      className="op-image-preview"
      isOpen
      onOpenChange={(isOpen) => {
        if (!isOpen) onClose()
      }}
      variant="opaque"
    >
      <Modal.Container size="full">
        <Modal.Dialog className="op-image-preview__dialog">
          <div
            className="op-image-preview__stage"
            onClick={(event) => {
              if (event.target === event.currentTarget) onClose()
            }}
            onWheel={(event) => {
              event.preventDefault()
              setImageScale((current) =>
                clampImageScale(current + (event.deltaY < 0 ? 0.12 : -0.12))
              )
            }}
          >
            <img
              alt={document.title}
              src={previewUrl}
              style={{ transform: `scale(${imageScale})` }}
            />
          </div>
          <Button
            aria-label={closeLabel}
            className="op-image-preview__close"
            isIconOnly
            onPress={onClose}
            size="md"
            variant="ghost"
          >
            <X size={21} />
          </Button>
          <div className="op-image-preview__controls">
            <Button
              aria-label={t`Zoom out`}
              isIconOnly
              onPress={() =>
                setImageScale((current) => clampImageScale(current - 0.2))
              }
              size="sm"
              variant="ghost"
            >
              <Minus size={15} />
            </Button>
            <span className="op-image-preview__zoom-value">
              {zoomPercentage}%
            </span>
            <Button
              aria-label={t`Zoom in`}
              isIconOnly
              onPress={() =>
                setImageScale((current) => clampImageScale(current + 0.2))
              }
              size="sm"
              variant="ghost"
            >
              <Plus size={15} />
            </Button>
          </div>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}

function OriginalTextPreview({
  previewUrl,
  title,
}: {
  previewUrl: string
  title: string
}) {
  const { t } = useMyOpenPanelsI18n()
  const [content, setContent] = useState<string | null>(null)
  const [error, setError] = useState(false)

  useEffect(() => {
    const controller = new AbortController()
    setContent(null)
    setError(false)
    fetch(previewUrl, { signal: controller.signal })
      .then((response) => {
        if (!response.ok) throw new Error(`HTTP ${response.status}`)
        return response.text()
      })
      .then(setContent)
      .catch((fetchError: unknown) => {
        if (
          fetchError instanceof DOMException &&
          fetchError.name === "AbortError"
        ) {
          return
        }
        setError(true)
      })
    return () => controller.abort()
  }, [previewUrl])

  if (error) {
    return (
      <div className="op-original-preview-dialog__status">
        <AlertCircle size={18} />
        {t`Failed to load document`}
      </div>
    )
  }
  if (content === null) {
    return (
      <div className="op-original-preview-dialog__status">
        <LoaderCircle className="op-spin" size={18} />
        {t`Loading document`}
      </div>
    )
  }
  return (
    <TextArea
      aria-label={title}
      className="op-original-preview-dialog__text"
      fullWidth
      readOnly
      value={content}
      variant="secondary"
    />
  )
}

export function OriginalPreviewDialog({
  closeLabel,
  document,
  onClose,
  previewUrl,
  titleLabel,
}: {
  closeLabel: string
  document: WikiOriginalPreviewDocument
  onClose: () => void
  previewUrl: string
  titleLabel: string
}) {
  const kind = originalPreviewKind(document)

  if (!kind) return null

  if (kind === "image") {
    return (
      <ImagePreviewDialog
        closeLabel={closeLabel}
        document={document}
        onClose={onClose}
        previewUrl={previewUrl}
      />
    )
  }

  return (
    <Modal.Backdrop
      isOpen
      onOpenChange={(isOpen) => {
        if (!isOpen) onClose()
      }}
    >
      <Modal.Container size="cover">
        <Modal.Dialog className="op-original-preview-dialog__panel">
          <Modal.CloseTrigger aria-label={closeLabel} />
          <Modal.Header>
            <div>
              <div className="op-wiki-panel__label">{titleLabel}</div>
              <Modal.Heading>{document.title}</Modal.Heading>
              <p className="op-original-preview-dialog__meta">
                {[document.originalFileName, formatBytes(document.sizeBytes)]
                  .filter(Boolean)
                  .join(" · ")}
              </p>
            </div>
          </Modal.Header>
          <Modal.Body>
            <div className="op-original-preview-dialog__body">
              {kind === "pdf" ? (
                <iframe src={previewUrl} title={document.title} />
              ) : null}
              {kind === "audio" ? (
                // biome-ignore lint/a11y/useMediaCaption: Raw file previews do not have caption tracks.
                <audio controls src={previewUrl}>
                  {document.originalFileName}
                </audio>
              ) : null}
              {kind === "video" ? (
                // biome-ignore lint/a11y/useMediaCaption: Raw file previews do not have caption tracks.
                <video controls src={previewUrl}>
                  {document.originalFileName}
                </video>
              ) : null}
              {kind === "text" ? (
                <OriginalTextPreview
                  previewUrl={previewUrl}
                  title={document.title}
                />
              ) : null}
            </div>
          </Modal.Body>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}

export function ConfirmDialog({
  backdropClassName,
  cancelLabel,
  confirmLabel,
  isBusy,
  message,
  onCancel,
  onConfirm,
  title,
}: {
  backdropClassName?: string
  cancelLabel: string
  confirmLabel: string
  isBusy: boolean
  message: string
  onCancel: () => void
  onConfirm: () => void
  title: string
}) {
  return (
    <AlertDialog.Backdrop
      className={backdropClassName}
      isOpen
      onOpenChange={(isOpen) => {
        if (!isOpen) onCancel()
      }}
    >
      <AlertDialog.Container placement="center">
        <AlertDialog.Dialog>
          <AlertDialog.Header>
            <AlertDialog.Icon status="danger" />
            <AlertDialog.Heading>{title}</AlertDialog.Heading>
          </AlertDialog.Header>
          <AlertDialog.Body>
            <p>{message}</p>
          </AlertDialog.Body>
          <AlertDialog.Footer>
            <Button isDisabled={isBusy} slot="close" variant="tertiary">
              {cancelLabel}
            </Button>
            <Button isDisabled={isBusy} onPress={onConfirm} variant="danger">
              <Trash2 size={15} />
              {confirmLabel}
            </Button>
          </AlertDialog.Footer>
        </AlertDialog.Dialog>
      </AlertDialog.Container>
    </AlertDialog.Backdrop>
  )
}
