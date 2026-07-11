import { Button, Input, Modal, TextArea } from "@heroui/react"
import { Save, Trash2, X, ZoomIn, ZoomOut } from "lucide-react"
import { useState } from "react"
import {
  clampImageScale,
  formatBytes,
  originalPreviewKind,
} from "../../lib/api"
import type { WikiRawDocument } from "../../types"

export function GeneratedDocumentDialog({
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
        <Modal.Dialog className="op-confirm-dialog__panel">
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
          <Modal.Footer className="op-confirm-dialog__actions">
            <Button isDisabled={isBusy} onPress={onCancel} variant="secondary">
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
  isBusy,
  onChange,
  onClose,
  onSave,
  closeLabel,
  saveLabel,
  title,
  titleLabel,
}: {
  closeLabel: string
  content: string
  isBusy: boolean
  onChange: (content: string) => void
  onClose: () => void
  onSave: () => void
  saveLabel: string
  title: string
  titleLabel: string
}) {
  return (
    <Modal.Backdrop
      isOpen
      onOpenChange={(isOpen) => {
        if (!isOpen) onClose()
      }}
    >
      <Modal.Container size="cover">
        <Modal.Dialog className="op-markdown-dialog__panel">
          <Modal.CloseTrigger aria-label={closeLabel} />
          <Modal.Header>
            <div>
              <div className="op-wiki-panel__label">{titleLabel}</div>
              <Modal.Heading>{title}</Modal.Heading>
            </div>
            <Button
              aria-label={saveLabel}
              isDisabled={isBusy}
              isIconOnly
              onPress={onSave}
              size="sm"
              variant="ghost"
            >
              <Save size={16} />
            </Button>
          </Modal.Header>
          <Modal.Body>
            <TextArea
              aria-label={title}
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
  document: WikiRawDocument
  onClose: () => void
  previewUrl: string
}) {
  const [imageScale, setImageScale] = useState(1)

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
          <div className="op-image-preview__controls">
            <Button
              aria-label="Zoom out"
              isIconOnly
              onPress={() =>
                setImageScale((current) => clampImageScale(current - 0.2))
              }
              size="sm"
              variant="secondary"
            >
              <ZoomOut size={16} />
            </Button>
            <Button
              aria-label="Zoom in"
              isIconOnly
              onPress={() =>
                setImageScale((current) => clampImageScale(current + 0.2))
              }
              size="sm"
              variant="secondary"
            >
              <ZoomIn size={16} />
            </Button>
            <Button
              aria-label={closeLabel}
              isIconOnly
              onPress={onClose}
              size="sm"
              variant="secondary"
            >
              <X size={16} />
            </Button>
          </div>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
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
  document: WikiRawDocument
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
            </div>
          </Modal.Body>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}

export function ConfirmDialog({
  cancelLabel,
  confirmLabel,
  isBusy,
  message,
  onCancel,
  onConfirm,
  title,
}: {
  cancelLabel: string
  confirmLabel: string
  isBusy: boolean
  message: string
  onCancel: () => void
  onConfirm: () => void
  title: string
}) {
  return (
    <Modal.Backdrop
      isOpen
      onOpenChange={(isOpen) => {
        if (!isOpen) onCancel()
      }}
    >
      <Modal.Container>
        <Modal.Dialog className="op-confirm-dialog__panel">
          <Modal.Header>
            <Modal.Heading>{title}</Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <p>{message}</p>
          </Modal.Body>
          <Modal.Footer className="op-confirm-dialog__actions">
            <Button isDisabled={isBusy} onPress={onCancel} variant="secondary">
              {cancelLabel}
            </Button>
            <Button isDisabled={isBusy} onPress={onConfirm} variant="danger">
              <Trash2 size={15} />
              {confirmLabel}
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}
