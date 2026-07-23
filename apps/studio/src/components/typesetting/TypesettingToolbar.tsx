import {
  Button,
  Input,
  ListBox,
  Modal,
  Popover,
  Select,
  Tooltip,
} from "@heroui/react"
import type { Attribute, Attributes } from "@tiptap/core"
import Image from "@tiptap/extension-image"
import { type useEditor, useEditorState } from "@tiptap/react"
import {
  AlertCircle,
  AlignCenter,
  AlignLeft,
  AlignRight,
  Bold,
  ImagePlus,
  Italic,
  Link as LinkIcon,
  List,
  ListOrdered,
  LoaderCircle,
  Quote,
  Redo2,
  Undo2,
} from "lucide-react"
import { type ReactNode, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import {
  apiUrl,
  formatBytes,
  originalPreviewKind,
  wikiRawOriginalUrl,
} from "../../lib/api"
import type {
  MyDocument,
  MyOpenPanelsTransport,
  WikiRawDocument,
} from "../../types"

type SaveStatus = "saved" | "saving" | "failed"
type DocumentPreview =
  | {
      content: string | null
      document: WikiRawDocument
      error: string | null
      format: "markdown"
      kind: "raw"
      loading: boolean
      source: "markdown" | "original"
    }
  | {
      content: string | null
      document: MyDocument
      error: string | null
      format: "markdown" | "text"
      kind: "generated"
      loading: boolean
    }

export function TypesettingToolbar({
  disabled = false,
  editor,
  onInsertImages,
}: {
  disabled?: boolean
  editor: ReturnType<typeof useEditor>
  onInsertImages: () => void
}) {
  const { t } = useMyOpenPanelsI18n()
  const [isLinkOpen, setIsLinkOpen] = useState(false)
  const [linkValue, setLinkValue] = useState("")
  const state = useEditorState({
    editor,
    selector: ({ editor: current }) => ({
      block: current?.isActive("heading", { level: 1 })
        ? "h1"
        : current?.isActive("heading", { level: 2 })
          ? "h2"
          : current?.isActive("heading", { level: 3 })
            ? "h3"
            : "p",
      bold: current?.isActive("bold") ?? false,
      bulletList: current?.isActive("bulletList") ?? false,
      canRedo: current?.can().redo() ?? false,
      canUndo: current?.can().undo() ?? false,
      italic: current?.isActive("italic") ?? false,
      link: current?.isActive("link") ?? false,
      textAlign: current?.isActive({ textAlign: "center" })
        ? "center"
        : current?.isActive({ textAlign: "right" })
          ? "right"
          : "left",
      orderedList: current?.isActive("orderedList") ?? false,
      quote: current?.isActive("blockquote") ?? false,
    }),
  })
  if (!editor) return <div className="op-typesetting-toolbar" />

  const applyLink = () => {
    const href = safeEditorLink(linkValue)
    if (!href) return
    editor.chain().focus().extendMarkRange("link").setLink({ href }).run()
    setIsLinkOpen(false)
  }

  return (
    <div
      aria-disabled={disabled}
      className="op-typesetting-toolbar"
      inert={disabled || undefined}
    >
      <Select
        aria-label={t`Text style`}
        className="op-typesetting-toolbar__text-style w-32 shrink-0"
        onChange={(key) => {
          const value = String(key)
          if (value === "p") editor.chain().focus().setParagraph().run()
          else {
            editor
              .chain()
              .focus()
              .toggleHeading({ level: Number(value.slice(1)) as 1 | 2 | 3 })
              .run()
          }
        }}
        selectionMode="single"
        value={state?.block ?? "p"}
        variant="secondary"
      >
        <Select.Trigger>
          <Select.Value />
          <Select.Indicator />
        </Select.Trigger>
        <Select.Popover>
          <ListBox>
            <ListBox.Item id="p" textValue={t`Paragraph`}>
              {t`Paragraph`}
            </ListBox.Item>
            <ListBox.Item id="h1" textValue="H1">
              H1
            </ListBox.Item>
            <ListBox.Item id="h2" textValue="H2">
              H2
            </ListBox.Item>
            <ListBox.Item id="h3" textValue="H3">
              H3
            </ListBox.Item>
          </ListBox>
        </Select.Popover>
      </Select>
      <ToolbarButton
        active={state?.bold}
        label={t`Bold`}
        onPress={() => editor.chain().focus().toggleBold().run()}
      >
        <Bold size={16} />
      </ToolbarButton>
      <ToolbarButton
        active={state?.italic}
        label={t`Italic`}
        onPress={() => editor.chain().focus().toggleItalic().run()}
      >
        <Italic size={16} />
      </ToolbarButton>
      <ToolbarButton
        active={state?.bulletList}
        label={t`Bullet list`}
        onPress={() => editor.chain().focus().toggleBulletList().run()}
      >
        <List size={16} />
      </ToolbarButton>
      <ToolbarButton
        active={state?.orderedList}
        label={t`Ordered list`}
        onPress={() => editor.chain().focus().toggleOrderedList().run()}
      >
        <ListOrdered size={16} />
      </ToolbarButton>
      <ToolbarButton
        active={state?.quote}
        label={t`Block quote`}
        onPress={() => editor.chain().focus().toggleBlockquote().run()}
      >
        <Quote size={16} />
      </ToolbarButton>
      <span className="op-typesetting-toolbar__divider" />
      <ToolbarButton
        active={state?.textAlign === "left"}
        label={t`Align left`}
        onPress={() => editor.chain().focus().setTextAlign("left").run()}
      >
        <AlignLeft size={18} />
      </ToolbarButton>
      <ToolbarButton
        active={state?.textAlign === "center"}
        label={t`Align center`}
        onPress={() => editor.chain().focus().setTextAlign("center").run()}
      >
        <AlignCenter size={18} />
      </ToolbarButton>
      <ToolbarButton
        active={state?.textAlign === "right"}
        label={t`Align right`}
        onPress={() => editor.chain().focus().setTextAlign("right").run()}
      >
        <AlignRight size={18} />
      </ToolbarButton>
      <span className="op-typesetting-toolbar__divider" />
      <Popover
        isOpen={isLinkOpen}
        onOpenChange={(isOpen) => {
          setIsLinkOpen(isOpen)
          if (isOpen) {
            setLinkValue(editor.getAttributes("link").href ?? "")
          }
        }}
      >
        <ToolbarButton
          active={state?.link}
          label={t`Link`}
          onPress={() => undefined}
        >
          <LinkIcon size={16} />
        </ToolbarButton>
        <Popover.Content placement="bottom start">
          <Popover.Dialog className="w-[min(360px,calc(100vw-40px))]">
            <div className="flex items-center gap-2">
              <Input
                aria-label={t`Link URL`}
                autoFocus
                className="min-w-30 flex-1"
                onChange={(event) => setLinkValue(event.currentTarget.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") applyLink()
                }}
                placeholder="https://"
                value={linkValue}
              />
              <Button onPress={applyLink} size="sm" variant="primary">
                {t`Apply`}
              </Button>
              {state?.link ? (
                <Button
                  onPress={() => {
                    editor.chain().focus().unsetLink().run()
                    setIsLinkOpen(false)
                  }}
                  size="sm"
                  variant="ghost"
                >
                  {t`Remove`}
                </Button>
              ) : null}
            </div>
          </Popover.Dialog>
        </Popover.Content>
      </Popover>
      <ToolbarButton label={t`Insert images`} onPress={onInsertImages}>
        <ImagePlus size={18} />
      </ToolbarButton>
      <span className="op-typesetting-toolbar__spacer" />
      <ToolbarButton
        disabled={!state?.canUndo}
        label={t`Undo`}
        onPress={() => editor.chain().focus().undo().run()}
      >
        <Undo2 size={16} />
      </ToolbarButton>
      <ToolbarButton
        disabled={!state?.canRedo}
        label={t`Redo`}
        onPress={() => editor.chain().focus().redo().run()}
      >
        <Redo2 size={16} />
      </ToolbarButton>
    </div>
  )
}

function ToolbarButton({
  active = false,
  children,
  disabled = false,
  label,
  onPress,
}: {
  active?: boolean
  children: ReactNode
  disabled?: boolean
  label: string
  onPress: () => void
}) {
  return (
    <Tooltip closeDelay={0} delay={300}>
      <Button
        aria-label={label}
        isDisabled={disabled}
        isIconOnly
        // Keep the ProseMirror selection intact until the command runs.
        onMouseDown={(event) => event.preventDefault()}
        onPress={onPress}
        size="md"
        variant={active ? "primary" : "ghost"}
      >
        {children}
      </Button>
      <Tooltip.Content placement="bottom">{label}</Tooltip.Content>
    </Tooltip>
  )
}

export function SaveIndicator({
  error,
  onRetry,
  status,
}: {
  error: string | null
  onRetry: () => void
  status: SaveStatus
}) {
  const { t } = useMyOpenPanelsI18n()
  if (status === "saved") return null
  if (status === "failed") {
    return (
      <button
        className="op-typesetting-save op-typesetting-save--failed"
        onClick={onRetry}
        title={error ?? t`Retry save`}
        type="button"
      >
        <AlertCircle size={13} />
        {t`Save failed`}
      </button>
    )
  }
  return (
    <span className="op-typesetting-save op-typesetting-save--saving">
      <LoaderCircle className="op-spin" size={13} />
      {t`Saving`}
    </span>
  )
}

export function DocumentPreviewDialog({
  activePublication,
  isInsertDisabled = false,
  onClose,
  onInsert,
  preview,
  transport,
}: {
  activePublication: boolean
  isInsertDisabled?: boolean
  onClose: () => void
  onInsert: () => void
  preview: DocumentPreview
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const rawPreviewKind =
    preview.kind === "raw" && preview.source === "original"
      ? originalPreviewKind(preview.document)
      : null
  const canInsert =
    activePublication &&
    !isInsertDisabled &&
    !preview.loading &&
    !preview.error &&
    preview.content !== null

  return (
    <Modal.Backdrop isOpen onOpenChange={(open) => !open && onClose()}>
      <Modal.Container size="cover">
        <Modal.Dialog className="op-typesetting-preview">
          <Modal.CloseTrigger aria-label={t`Close`} />
          <Modal.Header>
            <div>
              <div className="op-wiki-panel__label">
                {preview.kind === "raw"
                  ? preview.source === "markdown"
                    ? t`Markdown`
                    : t`Original file`
                  : t`My Documents`}
              </div>
              <Modal.Heading>{preview.document.title}</Modal.Heading>
              {preview.kind === "raw" ? (
                <p>
                  {[
                    preview.document.originalFileName,
                    formatBytes(preview.document.sizeBytes),
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </p>
              ) : null}
              {preview.kind === "raw" && !preview.document.markdownRef ? (
                <p>{t`Convert this document to Markdown before inserting it.`}</p>
              ) : null}
            </div>
            <Button
              isDisabled={!canInsert}
              onPress={onInsert}
              size="sm"
              variant="primary"
            >
              {t`Insert into content details`}
            </Button>
          </Modal.Header>
          <Modal.Body>
            <div className="op-typesetting-preview__body">
              {preview.loading ? (
                <div className="op-typesetting-preview__status">
                  <LoaderCircle className="op-spin" size={18} />
                  {t`Loading document`}
                </div>
              ) : preview.error ? (
                <div className="op-typesetting-preview__status">
                  <AlertCircle size={18} />
                  {t`Failed to load document`}
                </div>
              ) : preview.kind === "raw" && rawPreviewKind === "text" ? (
                <pre>{preview.content ?? ""}</pre>
              ) : preview.kind === "raw" &&
                rawPreviewKind &&
                rawPreviewKind !== "text" ? (
                <RawDocumentMedia
                  document={preview.document}
                  kind={rawPreviewKind}
                  src={wikiRawOriginalUrl(transport.apiBase, preview.document)}
                />
              ) : preview.content !== null ? (
                <pre>{preview.content}</pre>
              ) : (
                <div className="op-typesetting-preview__status">
                  {preview.kind === "raw" && !preview.document.markdownRef
                    ? t`Convert this document to Markdown before inserting it.`
                    : t`Preview is not available for this file type`}
                </div>
              )}
            </div>
          </Modal.Body>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}

function RawDocumentMedia({
  document,
  kind,
  src,
}: {
  document: WikiRawDocument
  kind: Exclude<ReturnType<typeof originalPreviewKind>, "text" | null>
  src: string
}) {
  if (kind === "image") return <img alt={document.title} src={src} />
  if (kind === "pdf") return <iframe src={src} title={document.title} />
  if (kind === "audio") {
    return (
      // biome-ignore lint/a11y/useMediaCaption: Source documents do not include caption tracks.
      <audio controls src={src}>
        {document.originalFileName}
      </audio>
    )
  }
  return (
    // biome-ignore lint/a11y/useMediaCaption: Source documents do not include caption tracks.
    <video controls src={src}>
      {document.originalFileName}
    </video>
  )
}

export function createTypesettingImageExtension(apiBase: string) {
  return Image.extend({
    addAttributes() {
      const parent = (this.parent?.() ?? {}) as Attributes
      const sourceAttribute = parent.src as Attribute | undefined
      return {
        ...parent,
        src: {
          ...sourceAttribute,
          renderHTML: (attributes) => ({
            src:
              typeof attributes.src === "string" &&
              attributes.src.startsWith("/")
                ? apiUrl(apiBase, attributes.src).toString()
                : attributes.src,
          }),
        },
        assetRef: {
          default: null,
          parseHTML: (element) => element.getAttribute("data-asset-ref"),
          renderHTML: (attributes) =>
            attributes.assetRef
              ? { "data-asset-ref": String(attributes.assetRef) }
              : {},
        },
      }
    },
    addNodeView() {
      const parentNodeView = this.parent?.()
      if (!parentNodeView) return null

      return (props) => {
        const nodeView = parentNodeView(props)
        const dom = nodeView.dom
        if (!(dom instanceof HTMLElement)) return nodeView

        const syncAppearance = (node: typeof props.node) => {
          const alignment = String(node.attrs.textAlign ?? "left")
          dom.classList.add("op-typesetting-editor-image")
          dom.dataset.textAlign = alignment
          dom.draggable = true
          dom.style.justifyContent =
            alignment === "center"
              ? "center"
              : alignment === "right"
                ? "flex-end"
                : "flex-start"
          const image = dom.querySelector("img")
          if (image) image.draggable = false
        }

        syncAppearance(props.node)
        const update = nodeView.update?.bind(nodeView)
        if (update) {
          nodeView.update = (node, decorations, innerDecorations) => {
            const updated = update(node, decorations, innerDecorations)
            if (updated) syncAppearance(node)
            return updated
          }
        }
        return nodeView
      }
    },
  }).configure({
    allowBase64: false,
    inline: false,
    resize: {
      alwaysPreserveAspectRatio: true,
      directions: ["top-left", "top-right", "bottom-left", "bottom-right"],
      enabled: true,
      minHeight: 80,
      minWidth: 80,
    },
  })
}

function safeEditorLink(value: string): string | null {
  const trimmed = value.trim()
  if (!trimmed) return null
  if (trimmed.startsWith("#")) return trimmed
  const candidate = /^[a-zA-Z][a-zA-Z\d+.-]*:/.test(trimmed)
    ? trimmed
    : `https://${trimmed}`
  try {
    const url = new URL(candidate)
    return ["http:", "https:", "mailto:"].includes(url.protocol)
      ? candidate
      : null
  } catch {
    return null
  }
}

export function formatPublicationTime(value: string, locale?: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date)
}
