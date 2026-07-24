import { TextAlign } from "@tiptap/extension-text-align"
import { Markdown } from "@tiptap/markdown"
import { GapCursor } from "@tiptap/pm/gapcursor"
import { Selection } from "@tiptap/pm/state"
import { useEditor } from "@tiptap/react"
import StarterKit from "@tiptap/starter-kit"
import {
  type DragEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { taskIsSucceeded } from "../../lib/task-status"
import {
  isSupportedTypesettingCoverMedia,
  isTypesettingLayoutTaskActive,
  latestTypesettingLayoutTask,
  parseTypesettingAssetDrag,
  plainTextToTypesettingContent,
  publicationTitleAfterDocumentInsert,
  selectedPublicationTitleId,
  typesettingImageClickSide,
  typesettingImagesToContent,
  typesettingInsertPosition,
  typesettingPublicationTitles,
  updatePublicationTitle,
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
import { PublicationContentSection } from "./TypesettingPublicationContent"
import { PublicationCoversSection } from "./TypesettingPublicationCovers"
import {
  PublicationTagsField,
  PublicationTitleField,
} from "./TypesettingPublicationFields"
import {
  PublicationDetailHeader,
  type PublicationSaveStatus,
} from "./TypesettingPublicationHeader"
import { PublicationTitleTaskDialog } from "./TypesettingTitleTaskDialog"
import { createTypesettingImageExtension } from "./TypesettingToolbar"

export type { PublicationView } from "./TypesettingPublicationHeader"
export { PublicationModeHeader } from "./TypesettingPublicationHeader"
export function PublicationDetail({
  importAsset,
  onDelete,
  onFlushSave,
  onInsertHandlerChange,
  onManageSkillModule,
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
  onManageSkillModule: (moduleKind: string) => void
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
  saveStatus: PublicationSaveStatus
  showHeader?: boolean
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
  uploadAsset: (file: File) => Promise<TypesettingPublicationImage>
}) {
  const { t } = useMyOpenPanelsI18n()
  const [assetError, setAssetError] = useState<string | null>(null)
  const [isCoverUploading, setIsCoverUploading] = useState(false)
  const [lastSavedAt, setLastSavedAt] = useState(publication.updatedAt)
  const [isCoverDialogOpen, setIsCoverDialogOpen] = useState(false)
  const [isAddCoverDialogOpen, setIsAddCoverDialogOpen] = useState(false)
  const [isInsertImageDialogOpen, setIsInsertImageDialogOpen] = useState(false)
  const [isTitleDialogOpen, setIsTitleDialogOpen] = useState(false)
  const [createdCoverTasks, setCreatedCoverTasks] = useState<ProjectTask[]>([])
  const [createdLayoutTasks, setCreatedLayoutTasks] = useState<ProjectTask[]>(
    []
  )
  const [createdTitleTasks, setCreatedTitleTasks] = useState<ProjectTask[]>([])
  const [isLayoutDialogOpen, setIsLayoutDialogOpen] = useState(false)
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
          task.queue === "publication" &&
          task.type === "generate_publication_cover" &&
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
      (!taskIsSucceeded(task) || createdCoverTaskIds.has(task.id))
  )
  const layoutTasks = useMemo(() => {
    const byId = new Map(tasks.map((task) => [task.id, task]))
    for (const task of createdLayoutTasks) {
      if (!byId.has(task.id)) byId.set(task.id, task)
    }
    return [...byId.values()].filter(
      (task) =>
        task.queue === "publication" &&
        task.type === "format_publication_content" &&
        task.targetId === publication.id
    )
  }, [createdLayoutTasks, publication.id, tasks])

  useEffect(() => {
    if (createdLayoutTasks.length === 0) return
    const taskIds = new Set(tasks.map((task) => task.id))
    if (!createdLayoutTasks.some((task) => taskIds.has(task.id))) return
    setCreatedLayoutTasks((current) =>
      current.filter((task) => !taskIds.has(task.id))
    )
  }, [createdLayoutTasks, tasks])

  const latestLayoutTask = latestTypesettingLayoutTask(
    layoutTasks,
    publication.id
  )
  const activeLayoutTask =
    layoutTasks.find(isTypesettingLayoutTaskActive) ?? null
  const titleOptions = typesettingPublicationTitles(publication)
  const titleTasks = useMemo(() => {
    const byId = new Map(tasks.map((task) => [task.id, task]))
    for (const task of createdTitleTasks) {
      if (!byId.has(task.id)) byId.set(task.id, task)
    }
    return [...byId.values()]
      .filter(
        (task) =>
          task.queue === "publication" &&
          task.type === "generate_publication_titles" &&
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
      (task) => !(taskIsSucceeded(task) && generatedTitleTaskIds.has(task.id))
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
          ...updatePublicationTitle(
            current,
            selectedPublicationTitleId(current),
            publicationTitleAfterDocumentInsert(current.title, title)
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
      setAssetError(null)
      if (!asset) {
        const supported = Array.from(event.dataTransfer.files).filter(
          isSupportedTypesettingCoverMedia
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
        if (failed) setAssetError(t`Failed to upload some media`)
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
        <PublicationDetailHeader
          lastSavedAt={lastSavedAt}
          onDelete={onDelete}
          onOpenLibrary={onOpenLibrary}
          onPreview={onPreview}
          onRetrySave={onRetrySave}
          publication={publication}
          saveError={saveError}
          saveStatus={saveStatus}
        />
      ) : null}

      <div className="op-typesetting-detail-scroll">
        <PublicationTitleField
          onGenerate={() => setIsTitleDialogOpen(true)}
          onOpenTask={(taskId) => onOpenAgentTasks([taskId])}
          onUpdate={onUpdate}
          publication={publication}
          task={visibleTitleTask}
        />
        <PublicationTagsField onUpdate={onUpdate} publication={publication} />

        <PublicationCoversSection
          isUploading={isCoverUploading}
          onAdd={() => setIsAddCoverDialogOpen(true)}
          onCreate={() => setIsCoverDialogOpen(true)}
          onDropCover={dropCover}
          onOpenTask={(taskId) => onOpenAgentTasks([taskId])}
          onUpdate={onUpdate}
          publication={publication}
          tasks={visibleCoverTasks}
          transport={transport}
        />

        <PublicationContentSection
          activeTask={activeLayoutTask}
          assetError={assetError}
          editor={editor}
          latestTask={latestLayoutTask}
          onDismissError={() => setAssetError(null)}
          onInsertImages={() => setIsInsertImageDialogOpen(true)}
          onOpenLayout={() => setIsLayoutDialogOpen(true)}
          onOpenTask={(taskId) => onOpenAgentTasks([taskId])}
        />
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
        onManageSkills={() => onManageSkillModule("publication-layout")}
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

      <PublicationTitleTaskDialog
        isOpen={isTitleDialogOpen}
        onFlushSave={onFlushSave}
        onManageSkills={() => onManageSkillModule("publication-title")}
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
        onManageSkills={() => onManageSkillModule("publication-cover")}
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
