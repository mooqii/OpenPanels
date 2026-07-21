import {
  Button,
  Input,
  ListBox,
  Modal,
  Popover,
  Select,
  Tooltip,
} from "@heroui/react"
import Image from "@tiptap/extension-image"
import {
  NodeViewWrapper,
  ReactNodeViewRenderer,
  type useEditor,
  useEditorState,
} from "@tiptap/react"
import {
  AlertCircle,
  Bold,
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
  MyOpenPanelsTransport,
  WikiGeneratedDocument,
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
      document: WikiGeneratedDocument
      error: string | null
      format: "markdown" | "text"
      kind: "generated"
      loading: boolean
    }

export function TypesettingToolbar({
  editor,
}: {
  editor: ReturnType<typeof useEditor>
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
    <div className="op-typesetting-toolbar">
      <Select
        aria-label={t`Text style`}
        className="w-24 shrink-0"
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
        onPress={onPress}
        size="sm"
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
  onClose,
  onInsert,
  preview,
  transport,
}: {
  activePublication: boolean
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
                  : t`Generated Documents`}
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
      return {
        ...this.parent?.(),
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
      return ReactNodeViewRenderer(({ node, selected }) => (
        <NodeViewWrapper
          className={
            selected
              ? "is-selected op-typesetting-editor-image"
              : "op-typesetting-editor-image"
          }
          contentEditable={false}
        >
          <img
            alt={node.attrs.alt ?? ""}
            src={
              typeof node.attrs.src === "string" &&
              node.attrs.src.startsWith("/")
                ? apiUrl(apiBase, node.attrs.src).toString()
                : node.attrs.src
            }
          />
        </NodeViewWrapper>
      ))
    },
  }).configure({
    allowBase64: false,
    inline: false,
    resize: {
      alwaysPreserveAspectRatio: true,
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
