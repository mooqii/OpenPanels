import { Button, Input, Label } from "@heroui/react"
import { Markdown } from "@tiptap/markdown"
import { EditorContent, useEditor } from "@tiptap/react"
import StarterKit from "@tiptap/starter-kit"
import {
  AlertCircle,
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  GripVertical,
  PanelLeft,
  Trash2,
  X,
} from "lucide-react"
import {
  type DragEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiUrl } from "../../lib/api"
import {
  isTypesettingDocumentEmpty,
  moveTypesettingCover,
  parseTypesettingAssetDrag,
  plainTextToTypesettingContent,
  TYPESETTING_ASSET_DRAG_TYPE,
  typesettingInsertPosition,
  typesettingTitleAfterDocumentInsert,
} from "../../lib/typesetting"
import type {
  MyOpenPanelsTransport,
  TypesettingCanvasAsset,
  TypesettingPublication,
  TypesettingPublicationImage,
} from "../../types"
import {
  createTypesettingImageExtension,
  formatPublicationTime,
  SaveIndicator,
  TypesettingToolbar,
} from "./TypesettingToolbar"

type SaveStatus = "saved" | "saving" | "failed"
const TYPESETTING_COVER_DRAG_TYPE = "application/x-myopenpanels-cover-index"
export function PublicationDetail({
  importAsset,
  onBack,
  onDelete,
  onInsertHandlerChange,
  onOpenLibrary,
  onRetrySave,
  onUpdate,
  publication,
  saveError,
  saveStatus,
  transport,
}: {
  importAsset: (
    asset: TypesettingCanvasAsset
  ) => Promise<TypesettingPublicationImage>
  onBack: () => void
  onDelete: () => void
  onInsertHandlerChange: (
    handler: (
      title: string,
      content: string,
      format: "markdown" | "text"
    ) => void
  ) => void
  onOpenLibrary: () => void
  onRetrySave: () => void
  onUpdate: (
    updater: (publication: TypesettingPublication) => TypesettingPublication
  ) => void
  publication: TypesettingPublication
  saveError: string | null
  saveStatus: SaveStatus
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const [assetError, setAssetError] = useState<string | null>(null)
  const [coverDropActive, setCoverDropActive] = useState(false)
  const [draggedCoverIndex, setDraggedCoverIndex] = useState<number | null>(
    null
  )
  const editorRef = useRef<ReturnType<typeof useEditor>>(null)
  const lastInsertPositionRef = useRef<number | null>(null)
  const publicationRef = useRef(publication)
  publicationRef.current = publication

  const imageExtension = useMemo(
    () => createTypesettingImageExtension(transport.apiBase),
    [transport.apiBase]
  )
  const extensions = useMemo(
    () => [
      StarterKit.configure({
        codeBlock: false,
        heading: { levels: [1, 2, 3] },
        horizontalRule: false,
        link: {
          autolink: true,
          defaultProtocol: "https",
          openOnClick: "whenNotEditable",
          protocols: ["http", "https", "mailto"],
        },
        strike: false,
        underline: false,
      }),
      imageExtension,
      Markdown,
    ],
    [imageExtension]
  )

  const editor = useEditor({
    content: publication.content,
    extensions,
    editorProps: {
      attributes: {
        class: "op-typesetting-editor__content",
      },
      handleDrop: (view, event) => {
        if (!event.dataTransfer) return false
        const asset = parseTypesettingAssetDrag(event.dataTransfer)
        if (!asset) return false
        event.preventDefault()
        const position = view.posAtCoords({
          left: event.clientX,
          top: event.clientY,
        })?.pos
        importAsset(asset)
          .then((image) => {
            const currentEditor = editorRef.current
            if (!currentEditor) return
            const target = typesettingInsertPosition(
              currentEditor.state.doc.content.size,
              position ?? null
            )
            currentEditor
              .chain()
              .focus()
              .insertContentAt(target, {
                type: "image",
                attrs: {
                  alt: image.fileName,
                  assetRef: image.assetRef,
                  height: image.height,
                  src: image.src,
                  title: image.fileName,
                  width: image.width,
                },
              })
              .run()
          })
          .catch((error) => {
            setAssetError(
              String(error instanceof Error ? error.message : error)
            )
          })
        return true
      },
    },
    onSelectionUpdate: ({ editor: currentEditor }) => {
      lastInsertPositionRef.current = currentEditor.state.selection.to
    },
    onUpdate: ({ editor: currentEditor }) => {
      onUpdate((current) => ({
        ...current,
        content: currentEditor.getJSON(),
        updatedAt: new Date().toISOString(),
      }))
    },
  })
  editorRef.current = editor

  const insertDocument = useCallback(
    (title: string, content: string, format: "markdown" | "text") => {
      if (!editor) return
      const position = typesettingInsertPosition(
        editor.state.doc.content.size,
        lastInsertPositionRef.current
      )
      if (!publicationRef.current.title.trim()) {
        onUpdate((current) => ({
          ...current,
          title: typesettingTitleAfterDocumentInsert(current.title, title),
          updatedAt: new Date().toISOString(),
        }))
      }
      const value =
        format === "markdown" ? content : plainTextToTypesettingContent(content)
      editor
        .chain()
        .focus()
        .insertContentAt(
          position,
          value,
          format === "markdown" ? { contentType: "markdown" } : undefined
        )
        .run()
      lastInsertPositionRef.current = editor.state.selection.to
    },
    [editor, onUpdate]
  )

  useEffect(() => {
    onInsertHandlerChange(insertDocument)
  }, [insertDocument, onInsertHandlerChange])

  const dropCover = useCallback(
    async (event: DragEvent<HTMLElement>) => {
      const asset = parseTypesettingAssetDrag(event.dataTransfer)
      if (!asset) return
      event.preventDefault()
      setCoverDropActive(false)
      setAssetError(null)
      try {
        const image = await importAsset(asset)
        onUpdate((current) => ({
          ...current,
          covers: [...current.covers, image],
          updatedAt: new Date().toISOString(),
        }))
      } catch (error) {
        setAssetError(String(error instanceof Error ? error.message : error))
      }
    },
    [importAsset, onUpdate]
  )

  return (
    <div className="op-typesetting-detail-view">
      <div className="op-typesetting-view-header op-typesetting-detail-header">
        <Button
          aria-label={t`Back to publication projects`}
          isIconOnly
          onPress={onBack}
          size="sm"
          variant="ghost"
        >
          <ArrowLeft size={17} />
        </Button>
        <Button
          aria-label={t`Open library`}
          className="op-typesetting-mobile-library-button"
          isIconOnly
          onPress={onOpenLibrary}
          size="sm"
          variant="ghost"
        >
          <PanelLeft size={17} />
        </Button>
        <div className="op-typesetting-detail-header__title">
          <strong>{publication.title.trim() || t`Untitled publication`}</strong>
          <small>{formatPublicationTime(publication.updatedAt)}</small>
        </div>
        <SaveIndicator
          error={saveError}
          onRetry={onRetrySave}
          status={saveStatus}
        />
        <Button
          aria-label={t`Delete publication project`}
          isIconOnly
          onPress={onDelete}
          size="sm"
          variant="ghost"
        >
          <Trash2 size={15} />
        </Button>
      </div>

      <div className="op-typesetting-detail-scroll">
        <div className="op-typesetting-field">
          <Label>{t`Title`}</Label>
          <Input
            aria-label={t`Title`}
            fullWidth
            onChange={(event) => {
              const title = event.currentTarget.value
              onUpdate((current) => ({
                ...current,
                title,
                updatedAt: new Date().toISOString(),
              }))
            }}
            placeholder={t`Untitled publication`}
            value={publication.title}
          />
        </div>

        <section className="op-typesetting-section">
          <div className="op-typesetting-section__heading">
            <div>
              <span>{t`Covers`}</span>
              <small>{t`The first image is used in the project list.`}</small>
            </div>
          </div>
          <div
            className={
              coverDropActive
                ? "is-active op-typesetting-cover-zone"
                : "op-typesetting-cover-zone"
            }
            onDragLeave={() => setCoverDropActive(false)}
            onDragOver={(event) => {
              if (
                !event.dataTransfer.types.includes(TYPESETTING_ASSET_DRAG_TYPE)
              ) {
                return
              }
              event.preventDefault()
              event.dataTransfer.dropEffect = "copy"
              setCoverDropActive(true)
            }}
            onDrop={(event) => {
              dropCover(event).catch(() => undefined)
            }}
          >
            {publication.covers.length ? (
              <div className="op-typesetting-covers">
                {publication.covers.map((cover, index) => (
                  <div
                    className="op-typesetting-cover"
                    draggable
                    key={cover.assetRef}
                    onDragEnd={() => setDraggedCoverIndex(null)}
                    onDragOver={(event) => {
                      if (
                        draggedCoverIndex === null ||
                        !event.dataTransfer.types.includes(
                          TYPESETTING_COVER_DRAG_TYPE
                        )
                      ) {
                        return
                      }
                      event.preventDefault()
                      event.dataTransfer.dropEffect = "move"
                    }}
                    onDragStart={(event) => {
                      setDraggedCoverIndex(index)
                      event.dataTransfer.effectAllowed = "move"
                      event.dataTransfer.setData(
                        TYPESETTING_COVER_DRAG_TYPE,
                        String(index)
                      )
                    }}
                    onDrop={(event) => {
                      const rawIndex = event.dataTransfer.getData(
                        TYPESETTING_COVER_DRAG_TYPE
                      )
                      if (!rawIndex) return
                      const from = Number(rawIndex)
                      if (!Number.isInteger(from)) return
                      event.preventDefault()
                      event.stopPropagation()
                      onUpdate((current) => ({
                        ...current,
                        covers: moveTypesettingCover(
                          current.covers,
                          from,
                          index
                        ),
                        updatedAt: new Date().toISOString(),
                      }))
                      setDraggedCoverIndex(null)
                    }}
                  >
                    <img
                      alt={cover.fileName}
                      draggable={false}
                      src={apiUrl(transport.apiBase, cover.src).toString()}
                    />
                    <span className="op-typesetting-cover__grip">
                      <GripVertical size={14} />
                    </span>
                    <div className="op-typesetting-cover__actions">
                      <Button
                        aria-label={t`Move cover left`}
                        isDisabled={index === 0}
                        isIconOnly
                        onPress={() =>
                          onUpdate((current) => ({
                            ...current,
                            covers: moveTypesettingCover(
                              current.covers,
                              index,
                              index - 1
                            ),
                            updatedAt: new Date().toISOString(),
                          }))
                        }
                        size="sm"
                        variant="ghost"
                      >
                        <ChevronLeft size={14} />
                      </Button>
                      <Button
                        aria-label={t`Move cover right`}
                        isDisabled={index === publication.covers.length - 1}
                        isIconOnly
                        onPress={() =>
                          onUpdate((current) => ({
                            ...current,
                            covers: moveTypesettingCover(
                              current.covers,
                              index,
                              index + 1
                            ),
                            updatedAt: new Date().toISOString(),
                          }))
                        }
                        size="sm"
                        variant="ghost"
                      >
                        <ChevronRight size={14} />
                      </Button>
                      <Button
                        aria-label={t`Remove cover`}
                        isIconOnly
                        onPress={() =>
                          onUpdate((current) => ({
                            ...current,
                            covers: current.covers.filter(
                              (candidate) =>
                                candidate.assetRef !== cover.assetRef
                            ),
                            updatedAt: new Date().toISOString(),
                          }))
                        }
                        size="sm"
                        variant="ghost"
                      >
                        <Trash2 size={14} />
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="op-typesetting-drop-empty">
                <span>{t`Drag images from the asset library to add covers.`}</span>
              </div>
            )}
          </div>
        </section>

        <section className="op-typesetting-section op-typesetting-content-section">
          <div className="op-typesetting-section__heading">
            <div>
              <span>{t`Content details`}</span>
              <small>{t`Rich text content is saved automatically.`}</small>
            </div>
          </div>
          <div className="op-typesetting-editor">
            <TypesettingToolbar editor={editor} />
            <div className="op-typesetting-editor__body">
              {editor && isTypesettingDocumentEmpty(editor.getJSON()) ? (
                <div className="op-typesetting-editor__empty">
                  <span>{t`Open a document from the library and insert it here.`}</span>
                </div>
              ) : null}
              <EditorContent editor={editor} />
            </div>
          </div>
        </section>
        {assetError ? (
          <div className="op-typesetting-inline-error" role="alert">
            <AlertCircle size={15} />
            <span>{assetError}</span>
            <Button
              aria-label={t`Dismiss`}
              isIconOnly
              onPress={() => setAssetError(null)}
              size="sm"
              variant="ghost"
            >
              <X size={14} />
            </Button>
          </div>
        ) : null}
      </div>
    </div>
  )
}
