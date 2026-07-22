import {
  Button,
  Chip,
  Input,
  InputGroup,
  type Key,
  Label,
  Spinner,
  Tabs,
  Tag,
  TagGroup,
  Tooltip,
} from "@heroui/react"
import { TextAlign } from "@tiptap/extension-text-align"
import { Markdown } from "@tiptap/markdown"
import { GapCursor } from "@tiptap/pm/gapcursor"
import { Selection } from "@tiptap/pm/state"
import { EditorContent, useEditor } from "@tiptap/react"
import StarterKit from "@tiptap/starter-kit"
import {
  AlertCircle,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  GripVertical,
  LoaderCircle,
  PanelLeft,
  Plus,
  Sparkles,
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
import { randomId } from "../../lib/id"
import {
  addTypesettingTitle,
  appendTypesettingTags,
  isSupportedTypesettingCoverImage,
  isTypesettingDocumentEmpty,
  isTypesettingLayoutTaskActive,
  latestTypesettingLayoutTask,
  moveTypesettingCover,
  parseTypesettingAssetDrag,
  plainTextToTypesettingContent,
  removeTypesettingTitle,
  selectedTypesettingTitleId,
  selectTypesettingTitle,
  TYPESETTING_ASSET_DRAG_TYPE,
  type TypesettingCoverTaskDisplayStatus,
  typesettingCoverTaskStatus,
  typesettingImageClickSide,
  typesettingImagesToContent,
  typesettingInsertPosition,
  typesettingLayoutTaskStatus,
  typesettingPublicationTitles,
  typesettingTitleAfterDocumentInsert,
  typesettingTitleTaskStatus,
  updateTypesettingTitle,
} from "../../lib/typesetting"
import type {
  MyOpenPanelsTransport,
  ProjectTask,
  TypesettingCanvasAsset,
  TypesettingPublication,
  TypesettingPublicationImage,
} from "../../types"
import { TypesettingAddCoverDialog } from "./TypesettingAddCoverDialog"
import { TypesettingCoverTaskDialog } from "./TypesettingCoverTaskDialog"
import { TypesettingLayoutDialog } from "./TypesettingLayoutDialog"
import { TypesettingTitleTaskDialog } from "./TypesettingTitleTaskDialog"
import {
  createTypesettingImageExtension,
  formatPublicationTime,
  TypesettingToolbar,
} from "./TypesettingToolbar"

type SaveStatus = "saved" | "saving" | "failed"
export type PublicationView = "edit" | "preview"
const TYPESETTING_COVER_DRAG_TYPE = "application/x-myopenpanels-cover-index"

export function PublicationModeHeader({
  onDelete,
  onOpenLibrary,
  onRetrySave,
  onViewChange,
  publication,
  saveError,
  saveStatus,
  view,
}: {
  onDelete: () => void
  onOpenLibrary?: () => void
  onRetrySave: () => void
  onViewChange: (view: PublicationView) => void
  publication: TypesettingPublication
  saveError: string | null
  saveStatus: SaveStatus
  view: PublicationView
}) {
  const { locale, t } = useMyOpenPanelsI18n()

  return (
    <div className="op-typesetting-view-header op-typesetting-detail-header op-typesetting-mode-header">
      <div className="op-typesetting-detail-header__save-meta">
        <span>
          {t`Last edited`}{" "}
          <time dateTime={publication.updatedAt}>
            {formatPublicationTime(publication.updatedAt, locale)}
          </time>
        </span>
        {saveStatus === "failed" ? (
          <button
            className="is-failed op-typesetting-detail-header__save-state"
            onClick={onRetrySave}
            title={saveError ?? t`Retry save`}
            type="button"
          >
            <AlertCircle size={12} />
            {t`Save failed`}
          </button>
        ) : (
          <span
            className="op-typesetting-detail-header__save-state"
            data-status={saveStatus}
          >
            {saveStatus === "saving" ? (
              <LoaderCircle className="op-spin" size={12} />
            ) : null}
            {saveStatus === "saving" ? t`Saving` : t`Auto-saved`}
          </span>
        )}
      </div>
      {onOpenLibrary ? (
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
      ) : null}
      <Tabs
        className="op-typesetting-mode-tabs"
        onSelectionChange={(key) =>
          onViewChange(key === "preview" ? "preview" : "edit")
        }
        selectedKey={view}
        variant="secondary"
      >
        <Tabs.ListContainer>
          <Tabs.List aria-label={t`Publication view`}>
            <Tabs.Tab id="edit">
              {t`Edit`}
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="preview">
              {t`Preview`}
              <Tabs.Indicator />
            </Tabs.Tab>
          </Tabs.List>
        </Tabs.ListContainer>
      </Tabs>
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
  )
}

export function PublicationDetail({
  importAsset,
  onDelete,
  onFlushSave,
  onInsertHandlerChange,
  onOpenAgentTasks,
  onOpenLibrary,
  onPreview,
  onRetrySave,
  onUpdate,
  publication,
  projectId,
  saveError,
  saveStatus,
  showHeader = true,
  tasks,
  transport,
  uploadAsset,
}: {
  importAsset: (
    asset: TypesettingCanvasAsset
  ) => Promise<TypesettingPublicationImage>
  onDelete: () => void
  onFlushSave: () => Promise<void>
  onInsertHandlerChange: (
    handler:
      | ((title: string, content: string, format: "markdown" | "text") => void)
      | null
  ) => void
  onOpenAgentTasks: (taskIds: string[]) => void
  onOpenLibrary?: () => void
  onPreview: () => void
  onRetrySave: () => void
  onUpdate: (
    updater: (publication: TypesettingPublication) => TypesettingPublication
  ) => void
  publication: TypesettingPublication
  projectId: string
  saveError: string | null
  saveStatus: SaveStatus
  showHeader?: boolean
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
  uploadAsset: (file: File) => Promise<TypesettingPublicationImage>
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [assetError, setAssetError] = useState<string | null>(null)
  const [coverDropActive, setCoverDropActive] = useState(false)
  const [isCoverUploading, setIsCoverUploading] = useState(false)
  const [draggedCoverIndex, setDraggedCoverIndex] = useState<number | null>(
    null
  )
  const [lastSavedAt, setLastSavedAt] = useState(publication.updatedAt)
  const [isCoverDialogOpen, setIsCoverDialogOpen] = useState(false)
  const [isAddCoverDialogOpen, setIsAddCoverDialogOpen] = useState(false)
  const [isInsertImageDialogOpen, setIsInsertImageDialogOpen] = useState(false)
  const [isTitleDialogOpen, setIsTitleDialogOpen] = useState(false)
  const [isTitleListExpanded, setIsTitleListExpanded] = useState(false)
  const [tagDraft, setTagDraft] = useState("")
  const [createdCoverTasks, setCreatedCoverTasks] = useState<ProjectTask[]>([])
  const [createdLayoutTasks, setCreatedLayoutTasks] = useState<ProjectTask[]>(
    []
  )
  const [createdTitleTasks, setCreatedTitleTasks] = useState<ProjectTask[]>([])
  const [isLayoutDialogOpen, setIsLayoutDialogOpen] = useState(false)
  const editorRef = useRef<ReturnType<typeof useEditor>>(null)
  const lastInsertPositionRef = useRef<number | null>(null)
  const titleFieldRef = useRef<HTMLDivElement>(null)
  const titleInputRef = useRef<HTMLInputElement>(null)
  const titleListPublicationIdRef = useRef(publication.id)
  const tagDraftPublicationIdRef = useRef(publication.id)
  const publicationRef = useRef(publication)
  publicationRef.current = publication

  useEffect(() => {
    if (saveStatus === "saved") setLastSavedAt(publication.updatedAt)
  }, [publication.updatedAt, saveStatus])

  useEffect(() => {
    if (titleListPublicationIdRef.current === publication.id) return
    titleListPublicationIdRef.current = publication.id
    setIsTitleListExpanded(false)
  }, [publication.id])

  useEffect(() => {
    if (!isTitleListExpanded) return
    const collapseTitleList = (event: PointerEvent) => {
      if (
        event.target instanceof Node &&
        !titleFieldRef.current?.contains(event.target)
      ) {
        setIsTitleListExpanded(false)
      }
    }
    document.addEventListener("pointerdown", collapseTitleList)
    return () => document.removeEventListener("pointerdown", collapseTitleList)
  }, [isTitleListExpanded])

  useEffect(() => {
    if (tagDraftPublicationIdRef.current === publication.id) return
    tagDraftPublicationIdRef.current = publication.id
    setTagDraft("")
  }, [publication.id])

  const commitTagDraft = useCallback(() => {
    const nextTags = appendTypesettingTags(publication.tags ?? [], tagDraft)
    setTagDraft("")
    if (nextTags.length === (publication.tags ?? []).length) return
    onUpdate((current) => ({
      ...current,
      tags: appendTypesettingTags(current.tags ?? [], tagDraft),
      updatedAt: new Date().toISOString(),
    }))
  }, [onUpdate, publication.tags, tagDraft])

  const removeTags = useCallback(
    (keys: Set<Key>) => {
      onUpdate((current) => ({
        ...current,
        tags: (current.tags ?? []).filter((tag) => !keys.has(tag)),
        updatedAt: new Date().toISOString(),
      }))
    },
    [onUpdate]
  )

  const coverTasks = useMemo(() => {
    const byId = new Map(tasks.map((task) => [task.id, task]))
    for (const task of createdCoverTasks) {
      if (!byId.has(task.id)) byId.set(task.id, task)
    }
    return [...byId.values()]
      .filter(
        (task) =>
          task.queue === "typesetting" &&
          task.type === "generate_typesetting_cover" &&
          task.targetId === publication.id
      )
      .sort((left, right) => left.createdAt.localeCompare(right.createdAt))
  }, [createdCoverTasks, publication.id, tasks])
  const generatedTaskIds = useMemo(
    () =>
      new Set(
        publication.covers.flatMap((cover) =>
          cover.source.kind === "generated" ? [cover.source.taskId] : []
        )
      ),
    [publication.covers]
  )
  const createdCoverTaskIds = useMemo(
    () => new Set(createdCoverTasks.map((task) => task.id)),
    [createdCoverTasks]
  )
  const visibleCoverTasks = coverTasks.filter(
    (task) =>
      !generatedTaskIds.has(task.id) &&
      (task.status !== "succeeded" || createdCoverTaskIds.has(task.id))
  )
  const layoutTasks = useMemo(() => {
    const byId = new Map(tasks.map((task) => [task.id, task]))
    for (const task of createdLayoutTasks) {
      if (!byId.has(task.id)) byId.set(task.id, task)
    }
    return [...byId.values()].filter(
      (task) =>
        task.queue === "typesetting" &&
        task.type === "format_typesetting_content" &&
        task.targetId === publication.id
    )
  }, [createdLayoutTasks, publication.id, tasks])
  const latestLayoutTask = latestTypesettingLayoutTask(
    layoutTasks,
    publication.id
  )
  const activeLayoutTask =
    layoutTasks.find(isTypesettingLayoutTaskActive) ?? null
  const titleOptions = typesettingPublicationTitles(publication)
  const activeTitleId = selectedTypesettingTitleId(publication)
  const activeTitle =
    titleOptions.find(({ id }) => id === activeTitleId) ?? titleOptions[0]
  const titleTasks = useMemo(() => {
    const byId = new Map(tasks.map((task) => [task.id, task]))
    for (const task of createdTitleTasks) {
      if (!byId.has(task.id)) byId.set(task.id, task)
    }
    return [...byId.values()]
      .filter(
        (task) =>
          task.queue === "typesetting" &&
          task.type === "generate_typesetting_titles" &&
          task.targetId === publication.id
      )
      .sort((left, right) => right.createdAt.localeCompare(left.createdAt))
  }, [createdTitleTasks, publication.id, tasks])
  const generatedTitleTaskIds = new Set(
    titleOptions.flatMap((title) =>
      title.source?.kind === "generated" ? [title.source.taskId] : []
    )
  )
  const visibleTitleTask =
    titleTasks.find(
      (task) =>
        task.status !== "succeeded" || !generatedTitleTaskIds.has(task.id)
    ) ?? null

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
      TextAlign.configure({
        alignments: ["left", "center", "right"],
        types: ["heading", "image", "paragraph"],
      }),
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
        if (activeLayoutTask) return true
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
              .insertContentAt(target, typesettingImagesToContent([image]))
              .run()
          })
          .catch((error) => {
            setAssetError(
              String(error instanceof Error ? error.message : error)
            )
          })
        return true
      },
      handleClickOn: (view, _position, node, nodePosition, event, direct) => {
        if (!(direct && node.type.name === "image")) return false
        const target = event.target
        if (!(target instanceof Element)) return false
        const container = target.closest(".op-typesetting-editor-image")
        const image = container?.querySelector("img")
        if (!image) return false
        const bounds = image.getBoundingClientRect()
        const side = typesettingImageClickSide(
          event.clientX,
          bounds.left,
          bounds.right
        )
        if (side === "inside") return false

        const cursorPosition =
          side === "before" ? nodePosition : nodePosition + node.nodeSize
        const resolvedPosition = view.state.doc.resolve(cursorPosition)
        const gapCursorType = GapCursor as typeof GapCursor & {
          valid: (position: typeof resolvedPosition) => boolean
        }
        const selection = gapCursorType.valid(resolvedPosition)
          ? new GapCursor(resolvedPosition)
          : Selection.near(resolvedPosition, side === "before" ? -1 : 1)
        view.dispatch(view.state.tr.setSelection(selection).scrollIntoView())
        view.focus()
        return true
      },
    },
    onSelectionUpdate: ({ editor: currentEditor }) => {
      lastInsertPositionRef.current = currentEditor.state.selection.to
    },
    onUpdate: ({ editor: currentEditor }) => {
      if (activeLayoutTask) return
      onUpdate((current) => ({
        ...current,
        content: currentEditor.getJSON(),
        updatedAt: new Date().toISOString(),
      }))
    },
  })
  editorRef.current = editor

  useEffect(() => {
    editor?.setEditable(!activeLayoutTask)
  }, [activeLayoutTask, editor])

  useEffect(() => {
    if (!editor) return
    if (
      JSON.stringify(editor.getJSON()) === JSON.stringify(publication.content)
    ) {
      return
    }
    editor.commands.setContent(publication.content, { emitUpdate: false })
  }, [editor, publication.content])

  const insertDocument = useCallback(
    (title: string, content: string, format: "markdown" | "text") => {
      if (!(editor && !activeLayoutTask)) return
      const position = typesettingInsertPosition(
        editor.state.doc.content.size,
        lastInsertPositionRef.current
      )
      if (!publicationRef.current.title.trim()) {
        onUpdate((current) => ({
          ...updateTypesettingTitle(
            current,
            selectedTypesettingTitleId(current),
            typesettingTitleAfterDocumentInsert(current.title, title)
          ),
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
    [activeLayoutTask, editor, onUpdate]
  )

  useEffect(() => {
    onInsertHandlerChange(insertDocument)
    return () => onInsertHandlerChange(null)
  }, [insertDocument, onInsertHandlerChange])

  const insertImages = useCallback(
    (images: TypesettingPublicationImage[]) => {
      if (!(editor && !activeLayoutTask) || images.length === 0) return
      const position = typesettingInsertPosition(
        editor.state.doc.content.size,
        lastInsertPositionRef.current
      )
      editor
        .chain()
        .focus()
        .insertContentAt(position, typesettingImagesToContent(images))
        .run()
      lastInsertPositionRef.current = editor.state.selection.to
    },
    [activeLayoutTask, editor]
  )

  const dropCover = useCallback(
    async (event: DragEvent<HTMLElement>) => {
      const asset = parseTypesettingAssetDrag(event.dataTransfer)
      event.preventDefault()
      setCoverDropActive(false)
      setAssetError(null)
      if (!asset) {
        const supported = Array.from(event.dataTransfer.files).filter(
          isSupportedTypesettingCoverImage
        )
        if (supported.length === 0 || isCoverUploading) return
        setIsCoverUploading(true)
        const added: TypesettingPublicationImage[] = []
        let failed = false
        for (const file of supported) {
          try {
            added.push(await uploadAsset(file))
          } catch {
            failed = true
          }
        }
        if (added.length > 0) {
          onUpdate((current) => ({
            ...current,
            covers: [...current.covers, ...added],
            updatedAt: new Date().toISOString(),
          }))
        }
        setIsCoverUploading(false)
        if (failed) setAssetError(t`Failed to upload some images`)
        return
      }
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
    [importAsset, isCoverUploading, onUpdate, t, uploadAsset]
  )

  return (
    <div className="op-typesetting-detail-view">
      {showHeader ? (
        <div className="op-typesetting-view-header op-typesetting-detail-header">
          {onOpenLibrary ? (
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
          ) : null}
          <div className="op-typesetting-detail-header__save-meta">
            <span>
              {t`Last edited`}{" "}
              <time dateTime={lastSavedAt}>
                {formatPublicationTime(lastSavedAt, locale)}
              </time>
            </span>
            {saveStatus === "failed" ? (
              <button
                className="is-failed op-typesetting-detail-header__save-state"
                onClick={onRetrySave}
                title={saveError ?? t`Retry save`}
                type="button"
              >
                <AlertCircle size={12} />
                {t`Save failed`}
              </button>
            ) : (
              <span
                className="op-typesetting-detail-header__save-state"
                data-status={saveStatus}
              >
                {saveStatus === "saving" ? (
                  <LoaderCircle className="op-spin" size={12} />
                ) : null}
                {saveStatus === "saving" ? t`Saving` : t`Auto-saved`}
              </span>
            )}
          </div>
          <Button
            aria-label={t`Preview`}
            onPress={onPreview}
            size="sm"
            variant="primary"
          >
            {t`Preview`}
          </Button>
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
      ) : null}

      <div className="op-typesetting-detail-scroll">
        <div
          className="op-typesetting-field op-typesetting-title-field"
          ref={titleFieldRef}
        >
          <div className="op-typesetting-title-field__heading">
            <Label htmlFor={`publication-title-${publication.id}`}>
              {t`Title`}
            </Label>
            <Button
              onPress={() => setIsTitleDialogOpen(true)}
              size="sm"
              variant="secondary"
            >
              <Sparkles size={14} />
              {t`Generate titles`}
            </Button>
          </div>
          <div className="op-typesetting-title-field__control">
            <Input
              aria-label={t`Title`}
              fullWidth
              id={`publication-title-${publication.id}`}
              onChange={(event) => {
                const value = event.currentTarget.value
                onUpdate((current) => ({
                  ...updateTypesettingTitle(current, activeTitleId, value),
                  updatedAt: new Date().toISOString(),
                }))
              }}
              placeholder={t`Untitled publication`}
              ref={titleInputRef}
              value={activeTitle.value}
              variant="secondary"
            />
            <Tooltip closeDelay={0} delay={300}>
              <Button
                aria-controls={`publication-title-options-${publication.id}`}
                aria-expanded={isTitleListExpanded}
                aria-label={
                  isTitleListExpanded ? t`Collapse titles` : t`Expand titles`
                }
                className="op-typesetting-title-field__expand-button"
                isIconOnly
                onPress={() => setIsTitleListExpanded((expanded) => !expanded)}
                size="sm"
                variant="ghost"
              >
                <ChevronDown
                  className="op-typesetting-title-field__chevron"
                  data-expanded={isTitleListExpanded}
                  size={16}
                />
              </Button>
              <Tooltip.Content placement="top">
                {isTitleListExpanded ? t`Collapse titles` : t`Expand titles`}
              </Tooltip.Content>
            </Tooltip>
          </div>

          {isTitleListExpanded ? (
            <div
              aria-label={t`Titles`}
              className="op-typesetting-title-field__list"
              id={`publication-title-options-${publication.id}`}
              role="list"
            >
              {titleOptions.map((title) => (
                <div
                  className="op-typesetting-title-field__row"
                  data-selected={title.id === activeTitleId}
                  key={title.id}
                  role="listitem"
                >
                  <Button
                    aria-pressed={title.id === activeTitleId}
                    className="op-typesetting-title-field__option"
                    onPress={() => {
                      onUpdate((current) => ({
                        ...selectTypesettingTitle(current, title.id),
                        updatedAt: new Date().toISOString(),
                      }))
                      setIsTitleListExpanded(false)
                      titleInputRef.current?.focus()
                    }}
                    variant="ghost"
                  >
                    <span className="op-typesetting-title-field__option-label">
                      {title.value.trim() || t`Untitled publication`}
                    </span>
                  </Button>
                  <Tooltip closeDelay={0} delay={300}>
                    <Button
                      aria-label={t`Delete title`}
                      isIconOnly
                      onPress={() => {
                        const replacementTitleId = randomId("publication-title")
                        onUpdate((current) => ({
                          ...removeTypesettingTitle(
                            current,
                            title.id,
                            replacementTitleId
                          ),
                          updatedAt: new Date().toISOString(),
                        }))
                      }}
                      size="sm"
                      variant="ghost"
                    >
                      <Trash2 size={15} />
                    </Button>
                    <Tooltip.Content placement="top">
                      {t`Delete title`}
                    </Tooltip.Content>
                  </Tooltip>
                </div>
              ))}
              <Button
                className="op-typesetting-title-field__add-option"
                onPress={() => {
                  onUpdate((current) => ({
                    ...addTypesettingTitle(current, {
                      id: randomId("publication-title"),
                      value: "",
                    }),
                    updatedAt: new Date().toISOString(),
                  }))
                  setIsTitleListExpanded(false)
                  titleInputRef.current?.focus()
                }}
                variant="ghost"
              >
                <Plus size={16} />
                {t`New title`}
              </Button>
            </div>
          ) : null}
          {visibleTitleTask ? (
            <TitleTaskStatus
              onOpen={() => onOpenAgentTasks([visibleTitleTask.id])}
              t={t}
              task={visibleTitleTask}
            />
          ) : null}
        </div>

        <div className="op-typesetting-field op-typesetting-tags-field">
          <Label htmlFor="op-typesetting-tags-input">{t`Tags`}</Label>
          <InputGroup
            aria-label={t`Tags`}
            className="op-typesetting-tags-field__control"
            fullWidth
          >
            {(publication.tags ?? []).length > 0 ? (
              <InputGroup.Prefix className="op-typesetting-tags-field__prefix">
                <TagGroup
                  aria-label={t`Tags`}
                  className="op-typesetting-tags-field__tags"
                  onRemove={removeTags}
                  size="sm"
                  variant="surface"
                >
                  <TagGroup.List
                    items={(publication.tags ?? []).map((tag) => ({
                      id: tag,
                      name: tag,
                    }))}
                  >
                    {(tag) => (
                      <Tag id={tag.id} textValue={tag.name}>
                        {tag.name}
                      </Tag>
                    )}
                  </TagGroup.List>
                </TagGroup>
              </InputGroup.Prefix>
            ) : null}
            <InputGroup.Input
              aria-label={t`Add tag`}
              id="op-typesetting-tags-input"
              onChange={(event) => setTagDraft(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.nativeEvent.isComposing || event.key !== "Enter")
                  return
                event.preventDefault()
                commitTagDraft()
              }}
              placeholder={t`Add tag`}
              value={tagDraft}
            />
          </InputGroup>
        </div>

        <section className="op-typesetting-section">
          <div className="op-typesetting-section__heading">
            <div>
              <span>{t`Covers`}</span>
              <small>{t`The first image is used in the project list.`}</small>
            </div>
            <div className="op-typesetting-section__actions">
              <Button
                onPress={() => setIsAddCoverDialogOpen(true)}
                size="sm"
                variant="secondary"
              >
                <Plus size={14} />
                {t`Add`}
              </Button>
              <Button
                onPress={() => setIsCoverDialogOpen(true)}
                size="sm"
                variant="secondary"
              >
                <Sparkles size={14} />
                {t`Create cover`}
              </Button>
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
              const isAssetDrag = event.dataTransfer.types.includes(
                TYPESETTING_ASSET_DRAG_TYPE
              )
              const isFileDrag = event.dataTransfer.types.includes("Files")
              if (!(isAssetDrag || isFileDrag)) {
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
            {publication.covers.length ||
            visibleCoverTasks.length ||
            isCoverUploading ? (
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
                {visibleCoverTasks.map((task) => (
                  <CoverTaskPlaceholder
                    key={task.id}
                    onOpen={() => onOpenAgentTasks([task.id])}
                    status={typesettingCoverTaskStatus(task)}
                    t={t}
                  />
                ))}
                {isCoverUploading ? (
                  <div className="op-typesetting-cover-task">
                    <span className="op-typesetting-cover-task__icon">
                      <Spinner size="sm" />
                    </span>
                    <strong>{t`Uploading images`}</strong>
                  </div>
                ) : null}
              </div>
            ) : (
              <div className="op-typesetting-drop-empty">
                <span>{t`Drag Canvas assets or image files here to add covers.`}</span>
              </div>
            )}
          </div>
        </section>

        <section className="op-typesetting-section op-typesetting-content-section">
          <div className="op-typesetting-section__heading">
            <div>
              <span>{t`Content details`}</span>
            </div>
            <div className="op-typesetting-section__actions">
              {latestLayoutTask ? (
                <LayoutTaskStatus
                  onOpen={() => onOpenAgentTasks([latestLayoutTask.id])}
                  t={t}
                  task={latestLayoutTask}
                />
              ) : null}
              <Button
                isDisabled={Boolean(activeLayoutTask)}
                onPress={() => setIsLayoutDialogOpen(true)}
                size="sm"
                variant="secondary"
              >
                <Sparkles size={14} />
                {t`Automatic layout`}
              </Button>
            </div>
          </div>
          <div
            aria-disabled={Boolean(activeLayoutTask)}
            className={
              activeLayoutTask
                ? "is-layout-locked op-typesetting-editor"
                : "op-typesetting-editor"
            }
          >
            <TypesettingToolbar
              disabled={Boolean(activeLayoutTask)}
              editor={editor}
              onInsertImages={() => setIsInsertImageDialogOpen(true)}
            />
            <div className="op-typesetting-editor__body">
              {editor && isTypesettingDocumentEmpty(editor.getJSON()) ? (
                <div className="op-typesetting-editor__empty">
                  <span>{t`Open a document from the library and insert it here.`}</span>
                </div>
              ) : null}
              <EditorContent editor={editor} />
            </div>
            {activeLayoutTask ? (
              <Tooltip closeDelay={0} delay={0}>
                <button
                  aria-label={t`A layout task is in progress. Cancel it or wait for it to finish before editing.`}
                  className="op-typesetting-editor__lock"
                  onClick={() => onOpenAgentTasks([activeLayoutTask.id])}
                  type="button"
                />
                <Tooltip.Content placement="top">
                  {t`A layout task is in progress. Cancel it or wait for it to finish before editing.`}
                </Tooltip.Content>
              </Tooltip>
            ) : null}
          </div>
        </section>
        {assetError ? (
          <div className="op-typesetting-inline-error" role="alert">
            <AlertCircle size={15} />
            <span className="op-typesetting-inline-error__message">
              {assetError}
            </span>
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

      <TypesettingAddCoverDialog
        importAsset={importAsset}
        isOpen={isAddCoverDialogOpen}
        onAdd={(images) => {
          onUpdate((current) => ({
            ...current,
            covers: [...current.covers, ...images],
            updatedAt: new Date().toISOString(),
          }))
        }}
        onOpenChange={setIsAddCoverDialogOpen}
        projectId={projectId}
        transport={transport}
        uploadAsset={uploadAsset}
      />

      <TypesettingAddCoverDialog
        importAsset={importAsset}
        isOpen={isInsertImageDialogOpen}
        onAdd={insertImages}
        onOpenChange={setIsInsertImageDialogOpen}
        projectId={projectId}
        purpose="content"
        transport={transport}
        uploadAsset={uploadAsset}
      />

      <TypesettingLayoutDialog
        isOpen={isLayoutDialogOpen}
        onFlushSave={onFlushSave}
        onOpenChange={setIsLayoutDialogOpen}
        onTaskCreated={(task) => {
          setCreatedLayoutTasks((current) => [
            task,
            ...current.filter((candidate) => candidate.id !== task.id),
          ])
        }}
        publication={publication}
        transport={transport}
      />

      <TypesettingTitleTaskDialog
        isOpen={isTitleDialogOpen}
        onFlushSave={onFlushSave}
        onOpenChange={setIsTitleDialogOpen}
        onTaskCreated={(task) => {
          setCreatedTitleTasks((current) => [
            task,
            ...current.filter((candidate) => candidate.id !== task.id),
          ])
        }}
        publication={publication}
        transport={transport}
      />

      <TypesettingCoverTaskDialog
        isOpen={isCoverDialogOpen}
        onFlushSave={onFlushSave}
        onOpenChange={setIsCoverDialogOpen}
        onTaskCreated={(task) => {
          setCreatedCoverTasks((current) => [
            task,
            ...current.filter((candidate) => candidate.id !== task.id),
          ])
        }}
        publication={publication}
        transport={transport}
      />
    </div>
  )
}

function TitleTaskStatus({
  onOpen,
  task,
  t,
}: {
  onOpen: () => void
  task: ProjectTask
  t: (value: TemplateStringsArray) => string
}) {
  const status = typesettingTitleTaskStatus(task)
  const label =
    status === "waiting"
      ? t`Waiting for titles`
      : status === "running"
        ? t`Generating titles`
        : status === "saving"
          ? t`Saving titles`
          : status === "failed"
            ? t`Title generation failed`
            : t`Title generation cancelled`
  return (
    <button
      className={`is-${status} op-typesetting-title-status`}
      onClick={onOpen}
      type="button"
    >
      {status === "waiting" || status === "running" || status === "saving" ? (
        <LoaderCircle className="op-spin" size={13} />
      ) : status === "failed" ? (
        <AlertCircle size={13} />
      ) : null}
      <span>{label}</span>
    </button>
  )
}

function LayoutTaskStatus({
  onOpen,
  task,
  t,
}: {
  onOpen: () => void
  task: ProjectTask
  t: (value: TemplateStringsArray) => string
}) {
  const status = typesettingLayoutTaskStatus(task)
  const label =
    status === "waiting"
      ? t`Waiting for layout`
      : status === "running"
        ? t`Formatting content`
        : status === "completed"
          ? t`Layout completed`
          : status === "failed"
            ? t`Layout failed`
            : t`Layout cancelled`
  return (
    <button
      className={`is-${status} op-typesetting-layout-status`}
      onClick={onOpen}
      type="button"
    >
      {status === "waiting" || status === "running" ? (
        <LoaderCircle className="op-spin" size={13} />
      ) : status === "failed" ? (
        <AlertCircle size={13} />
      ) : null}
      <span>{label}</span>
    </button>
  )
}

function CoverTaskPlaceholder({
  onOpen,
  status,
  t,
}: {
  onOpen: () => void
  status: TypesettingCoverTaskDisplayStatus
  t: (value: TemplateStringsArray) => string
}) {
  const active =
    status === "waiting" || status === "running" || status === "saving"
  const label =
    status === "waiting"
      ? t`Waiting to create`
      : status === "running"
        ? t`Creating cover`
        : status === "saving"
          ? t`Saving cover`
          : status === "failed"
            ? t`Cover creation failed`
            : t`Cover creation cancelled`
  return (
    <button
      className={`is-${status} op-typesetting-cover-task`}
      onClick={onOpen}
      type="button"
    >
      <span className="op-typesetting-cover-task__icon">
        {active ? (
          <LoaderCircle className="op-spin" size={18} />
        ) : (
          <AlertCircle size={18} />
        )}
      </span>
      <strong>{label}</strong>
      <Chip
        color={status === "failed" ? "danger" : "default"}
        size="sm"
        variant="soft"
      >
        {t`Open task`}
      </Chip>
    </button>
  )
}
