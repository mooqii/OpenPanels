import {
  Button,
  Chip,
  Input,
  Label,
  ListBox,
  Modal,
  Select,
  Spinner,
  TextArea,
} from "@heroui/react"
import { Markdown } from "@tiptap/markdown"
import { EditorContent, useEditor } from "@tiptap/react"
import StarterKit from "@tiptap/starter-kit"
import {
  AlertCircle,
  ChevronLeft,
  ChevronRight,
  GripVertical,
  ImagePlus,
  LoaderCircle,
  PanelLeft,
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
import { apiJson, apiUrl } from "../../lib/api"
import { randomId } from "../../lib/id"
import {
  isTypesettingDocumentEmpty,
  moveTypesettingCover,
  parseTypesettingAssetDrag,
  plainTextToTypesettingContent,
  TYPESETTING_ASSET_DRAG_TYPE,
  type TypesettingCoverTaskDisplayStatus,
  typesettingCoverRequestPayload,
  typesettingCoverTaskStatus,
  typesettingInsertPosition,
  typesettingTitleAfterDocumentInsert,
} from "../../lib/typesetting"
import type {
  AgentSkillListing,
  MyOpenPanelsTransport,
  ProjectTask,
  TypesettingCanvasAsset,
  TypesettingPublication,
  TypesettingPublicationImage,
} from "../../types"
import {
  createTypesettingImageExtension,
  formatPublicationTime,
  TypesettingToolbar,
} from "./TypesettingToolbar"

type SaveStatus = "saved" | "saving" | "failed"
const TYPESETTING_COVER_DRAG_TYPE = "application/x-myopenpanels-cover-index"
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
  saveError,
  saveStatus,
  tasks,
  transport,
}: {
  importAsset: (
    asset: TypesettingCanvasAsset
  ) => Promise<TypesettingPublicationImage>
  onDelete: () => void
  onFlushSave: () => Promise<void>
  onInsertHandlerChange: (
    handler: (
      title: string,
      content: string,
      format: "markdown" | "text"
    ) => void
  ) => void
  onOpenAgentTasks: (taskIds: string[]) => void
  onOpenLibrary?: () => void
  onPreview: () => void
  onRetrySave: () => void
  onUpdate: (
    updater: (publication: TypesettingPublication) => TypesettingPublication
  ) => void
  publication: TypesettingPublication
  saveError: string | null
  saveStatus: SaveStatus
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [assetError, setAssetError] = useState<string | null>(null)
  const [coverDropActive, setCoverDropActive] = useState(false)
  const [draggedCoverIndex, setDraggedCoverIndex] = useState<number | null>(
    null
  )
  const [lastSavedAt, setLastSavedAt] = useState(publication.updatedAt)
  const [isCoverDialogOpen, setIsCoverDialogOpen] = useState(false)
  const [coverSkills, setCoverSkills] = useState<AgentSkillListing[]>([])
  const [selectedCoverSkillId, setSelectedCoverSkillId] = useState("")
  const [coverInstruction, setCoverInstruction] = useState("")
  const [coverDialogError, setCoverDialogError] = useState<string | null>(null)
  const [isCoverSkillsLoading, setIsCoverSkillsLoading] = useState(false)
  const [isCoverSubmitting, setIsCoverSubmitting] = useState(false)
  const [createdCoverTasks, setCreatedCoverTasks] = useState<ProjectTask[]>([])
  const editorRef = useRef<ReturnType<typeof useEditor>>(null)
  const lastInsertPositionRef = useRef<number | null>(null)
  const publicationRef = useRef(publication)
  publicationRef.current = publication

  useEffect(() => {
    if (saveStatus === "saved") setLastSavedAt(publication.updatedAt)
  }, [publication.updatedAt, saveStatus])

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

  const openCoverDialog = useCallback(async () => {
    setIsCoverDialogOpen(true)
    setCoverDialogError(null)
    setIsCoverSkillsLoading(true)
    try {
      const response = await apiJson<{ skills?: AgentSkillListing[] }>(
        transport.apiBase,
        "/api/typesetting/cover-skills"
      )
      const skills = response.skills ?? []
      setCoverSkills(skills)
      setSelectedCoverSkillId((current) =>
        skills.some((item) => item.skill.id === current)
          ? current
          : (skills.find(
              (item) => item.skill.id === "typesetting-cover-default"
            )?.skill.id ??
            skills[0]?.skill.id ??
            "")
      )
    } catch (error) {
      setCoverDialogError(
        String(error instanceof Error ? error.message : error)
      )
    } finally {
      setIsCoverSkillsLoading(false)
    }
  }, [transport.apiBase])

  const submitCoverTask = useCallback(async () => {
    if (!selectedCoverSkillId) return
    setIsCoverSubmitting(true)
    setCoverDialogError(null)
    try {
      await onFlushSave()
      const response = await apiJson<{ task: ProjectTask }>(
        transport.apiBase,
        "/api/typesetting/cover-requests",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(
            typesettingCoverRequestPayload({
              instruction: coverInstruction,
              publicationId: publication.id,
              requestId: randomId("cover-request"),
              skillId: selectedCoverSkillId,
            })
          ),
        }
      )
      setCreatedCoverTasks((current) => [
        response.task,
        ...current.filter((task) => task.id !== response.task.id),
      ])
      setCoverInstruction("")
      setIsCoverDialogOpen(false)
    } catch (error) {
      setCoverDialogError(
        String(error instanceof Error ? error.message : error)
      )
    } finally {
      setIsCoverSubmitting(false)
    }
  }, [
    coverInstruction,
    onFlushSave,
    publication.id,
    selectedCoverSkillId,
    transport.apiBase,
  ])

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
            <Button onPress={openCoverDialog} size="sm" variant="secondary">
              <Sparkles size={14} />
              {t`Create cover`}
            </Button>
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
            {publication.covers.length || visibleCoverTasks.length ? (
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

      {isCoverDialogOpen ? (
        <Modal.Backdrop
          isOpen
          onOpenChange={(open) => {
            if (!(open || isCoverSubmitting)) setIsCoverDialogOpen(false)
          }}
        >
          <Modal.Container placement="center" size="sm">
            <Modal.Dialog className="op-typesetting-cover-dialog">
              <Modal.CloseTrigger aria-label={t`Close`} />
              <Modal.Header>
                <Modal.Icon>
                  <ImagePlus size={19} />
                </Modal.Icon>
                <Modal.Heading>{t`Create cover`}</Modal.Heading>
              </Modal.Header>
              <Modal.Body>
                <div className="op-typesetting-cover-dialog__field">
                  <Label>{t`Cover Skill`}</Label>
                  {isCoverSkillsLoading ? (
                    <div className="op-typesetting-cover-dialog__loading">
                      <Spinner size="sm" />
                      <span>{t`Loading Cover Skills`}</span>
                    </div>
                  ) : coverSkills.length ? (
                    <Select
                      aria-label={t`Cover Skill`}
                      onSelectionChange={(key) =>
                        setSelectedCoverSkillId(String(key))
                      }
                      selectedKey={selectedCoverSkillId}
                    >
                      <Select.Trigger>
                        <Select.Value />
                        <Select.Indicator />
                      </Select.Trigger>
                      <Select.Popover>
                        <ListBox>
                          {coverSkills.map((item) => (
                            <ListBox.Item
                              id={item.skill.id}
                              key={item.skill.id}
                              textValue={item.skill.name}
                            >
                              <div className="op-typesetting-cover-dialog__skill">
                                <strong>{item.skill.name}</strong>
                                <span>{item.skill.description}</span>
                              </div>
                            </ListBox.Item>
                          ))}
                        </ListBox>
                      </Select.Popover>
                    </Select>
                  ) : (
                    <span className="op-typesetting-cover-dialog__empty">
                      {t`No Cover Skills available`}
                    </span>
                  )}
                </div>
                <div className="op-typesetting-cover-dialog__field">
                  <Label>{t`Additional requirements`}</Label>
                  <TextArea
                    aria-label={t`Additional requirements`}
                    fullWidth
                    maxLength={4000}
                    onChange={(event) =>
                      setCoverInstruction(event.currentTarget.value)
                    }
                    placeholder={t`Describe the style, subject, or composition you want`}
                    value={coverInstruction}
                  />
                </div>
                {coverDialogError ? (
                  <div className="op-typesetting-inline-error" role="alert">
                    <AlertCircle size={15} />
                    <span>{coverDialogError}</span>
                  </div>
                ) : null}
              </Modal.Body>
              <Modal.Footer>
                <Button
                  isDisabled={isCoverSubmitting}
                  onPress={() => setIsCoverDialogOpen(false)}
                  variant="secondary"
                >
                  {t`Cancel`}
                </Button>
                <Button
                  isDisabled={
                    isCoverSkillsLoading ||
                    isCoverSubmitting ||
                    !selectedCoverSkillId ||
                    (!publication.title.trim() &&
                      isTypesettingDocumentEmpty(publication.content))
                  }
                  onPress={submitCoverTask}
                  variant="primary"
                >
                  {isCoverSubmitting ? (
                    <Spinner size="sm" />
                  ) : (
                    <Sparkles size={15} />
                  )}
                  {isCoverSubmitting ? t`Submitting` : t`Start creating`}
                </Button>
              </Modal.Footer>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      ) : null}
    </div>
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
