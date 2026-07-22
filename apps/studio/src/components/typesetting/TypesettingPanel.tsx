import { Button } from "@heroui/react"
import { FileInput, FileText, PanelLeft } from "lucide-react"
import { type ReactNode, useCallback, useRef, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { useTypesettingStateEditor } from "../../hooks/use-typesetting-state-editor"
import {
  apiFetch,
  apiJson,
  originalPreviewKind,
  tryOpenBrowserWindow,
  wikiGeneratedOriginalUrl,
} from "../../lib/api"
import { randomId } from "../../lib/id"
import {
  createTypesettingPublication,
  isInsertableTypesettingDocument,
  isTypesettingLayoutTaskActive,
} from "../../lib/typesetting"
import type {
  MyOpenPanelsTransport,
  ProjectTask,
  TypesettingPublication,
  TypesettingState,
  WikiGeneratedDocument,
  WikiOriginalPreviewDocument,
  WikiState,
} from "../../types"
import { PublicationPreview } from "../publishing/PublicationPreview"
import {
  ConfirmDialog,
  MarkdownDialog,
  OriginalPreviewDialog,
} from "../wiki/Dialogs"

import { TypesettingLibrary } from "./TypesettingLibrary"
import {
  PublicationDetail,
  PublicationModeHeader,
  type PublicationView,
} from "./TypesettingPublication"

type GeneratedDocumentDialog = {
  content: string
  document: WikiGeneratedDocument
}

type InsertDocumentHandler = (
  title: string,
  content: string,
  format: "markdown" | "text"
) => void

export function TypesettingPanel({
  chromeContent,
  onStateSaved,
  onOpenAgentTasks,
  panelId,
  projectId,
  revision,
  state: initialState,
  tasks,
  transport,
  wiki,
}: {
  chromeContent: ReactNode
  onStateSaved: (state: TypesettingState, revision: number) => void
  onOpenAgentTasks: (taskIds: string[]) => void
  panelId: string
  projectId: string
  revision: number
  state: TypesettingState
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
  wiki: WikiState
}) {
  const { t } = useMyOpenPanelsI18n()
  const [view, setView] = useState<PublicationView>("edit")
  const [activePublicationId, setActivePublicationId] = useState<string | null>(
    null
  )
  const [pendingDelete, setPendingDelete] =
    useState<TypesettingPublication | null>(null)
  const [documentDialog, setDocumentDialog] =
    useState<GeneratedDocumentDialog | null>(null)
  const [originalPreview, setOriginalPreview] = useState<{
    document: WikiOriginalPreviewDocument
    previewUrl: string
  } | null>(null)
  const [isLibraryOpen, setIsLibraryOpen] = useState(false)
  const [insertingDocumentId, setInsertingDocumentId] = useState<string | null>(
    null
  )
  const insertDocumentRef = useRef<InsertDocumentHandler | null>(null)
  const pendingInsertionRef = useRef<{
    content: string
    document: WikiGeneratedDocument
    publicationId: string
  } | null>(null)
  const activePublicationIdRef = useRef(activePublicationId)
  activePublicationIdRef.current = activePublicationId
  const {
    flushSave,
    importAsset,
    replaceState,
    saveError,
    saveStatus,
    state,
    updatePublication,
    uploadAsset,
  } = useTypesettingStateEditor({
    initialState,
    onStateSaved,
    panelId,
    projectId,
    revision,
    transport,
  })
  const dialogDocumentId = documentDialog?.document.id
  const dialogFileName = documentDialog?.document.originalFileName
  const dialogMimeType = documentDialog?.document.mimeType

  const createPublication = useCallback(() => {
    const timestamp = new Date().toISOString()
    const publication = createTypesettingPublication(
      randomId("publication"),
      timestamp
    )
    const next = {
      ...state,
      publications: [publication, ...state.publications],
    }
    insertDocumentRef.current = null
    pendingInsertionRef.current = null
    replaceState(next, publication.id)
    setActivePublicationId(publication.id)
    setView("edit")
  }, [replaceState, state])

  const deletePublication = useCallback(
    (publication: TypesettingPublication) => {
      const next = {
        ...state,
        publications: state.publications.filter(
          (candidate) => candidate.id !== publication.id
        ),
      }
      replaceState(next, publication.id, { deleted: true })
      setPendingDelete(null)
      if (activePublicationId === publication.id) {
        insertDocumentRef.current = null
        pendingInsertionRef.current = null
        setActivePublicationId(null)
        setView("edit")
      }
    },
    [activePublicationId, replaceState, state]
  )

  const openGeneratedDocument = useCallback(
    async (document: WikiGeneratedDocument) => {
      try {
        const data = await apiJson<{ content?: string }>(
          transport.apiBase,
          `/api/wiki/generated-documents/${encodeURIComponent(document.id)}`
        )
        setDocumentDialog({ content: data.content ?? "", document })
      } catch (error) {
        console.error("Failed to open generated document", error)
      }
    },
    [transport.apiBase]
  )

  const saveGeneratedDocument = useCallback(
    async (content: string) => {
      if (!(dialogDocumentId && dialogFileName && dialogMimeType)) return
      const data = await apiJson<{ document: WikiGeneratedDocument }>(
        transport.apiBase,
        `/api/wiki/generated-documents/${encodeURIComponent(dialogDocumentId)}`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            content,
            fileName: dialogFileName,
            mimeType: dialogMimeType,
          }),
        }
      )
      setDocumentDialog((current) =>
        current ? { ...current, content, document: data.document } : current
      )
    },
    [dialogDocumentId, dialogFileName, dialogMimeType, transport.apiBase]
  )

  const renameGeneratedDocumentFile = useCallback(
    async (fileName: string) => {
      if (!dialogDocumentId) return
      const data = await apiJson<{ document: WikiGeneratedDocument }>(
        transport.apiBase,
        `/api/wiki/generated-documents/${encodeURIComponent(dialogDocumentId)}`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ fileName }),
        }
      )
      setDocumentDialog((current) =>
        current ? { ...current, document: data.document } : current
      )
    },
    [dialogDocumentId, transport.apiBase]
  )

  const revealGeneratedOriginal = useCallback(
    async (document: WikiGeneratedDocument) => {
      await apiFetch(
        transport.apiBase,
        `/api/wiki/generated-documents/${encodeURIComponent(document.id)}/reveal`,
        { method: "POST" }
      )
    },
    [transport.apiBase]
  )

  const openGeneratedOriginal = useCallback(
    (document: WikiGeneratedDocument) => {
      if (!document.importSource) return
      const previewDocument: WikiOriginalPreviewDocument = {
        id: document.id,
        mimeType: document.importSource.mimeType,
        originalFileName: document.importSource.fileName,
        sizeBytes: document.importSource.sizeBytes,
        title: document.title,
      }
      const previewUrl = wikiGeneratedOriginalUrl(transport.apiBase, document)
      if (originalPreviewKind(previewDocument)) {
        setOriginalPreview({ document: previewDocument, previewUrl })
        return
      }
      if (tryOpenBrowserWindow(previewUrl)) return
      revealGeneratedOriginal(document).catch((error) => {
        console.error("Failed to reveal imported generated document", error)
      })
    },
    [revealGeneratedOriginal, transport.apiBase]
  )

  const activePublication = state.publications.find(
    (publication) => publication.id === activePublicationId
  )
  const isContentLocked = Boolean(
    activePublication &&
      tasks.some(
        (task) =>
          task.type === "format_typesetting_content" &&
          task.targetId === activePublication.id &&
          isTypesettingLayoutTaskActive(task)
      )
  )

  const queueDocumentInsertion = useCallback(
    (document: WikiGeneratedDocument, content: string) => {
      if (!(activePublication && !isContentLocked)) return
      if (activePublicationIdRef.current !== activePublication.id) return
      if (view === "edit" && insertDocumentRef.current) {
        insertDocumentRef.current(document.title, content, document.format)
        return
      }
      pendingInsertionRef.current = {
        content,
        document,
        publicationId: activePublication.id,
      }
      setView("edit")
    },
    [activePublication, isContentLocked, view]
  )

  const insertGeneratedDocument = useCallback(
    async (document: WikiGeneratedDocument) => {
      if (
        !activePublication ||
        isContentLocked ||
        !isInsertableTypesettingDocument(document)
      ) {
        return
      }
      setInsertingDocumentId(document.id)
      try {
        const data = await apiJson<{ content?: string }>(
          transport.apiBase,
          `/api/wiki/generated-documents/${encodeURIComponent(document.id)}`
        )
        queueDocumentInsertion(document, data.content ?? "")
      } catch (error) {
        console.error("Failed to insert generated document", error)
      } finally {
        setInsertingDocumentId(null)
      }
    },
    [
      activePublication,
      isContentLocked,
      queueDocumentInsertion,
      transport.apiBase,
    ]
  )

  const handleInsertHandlerChange = useCallback(
    (handler: InsertDocumentHandler | null) => {
      insertDocumentRef.current = handler
      const pending = pendingInsertionRef.current
      if (
        !(handler && pending) ||
        pending.publicationId !== activePublicationIdRef.current
      ) {
        return
      }
      pendingInsertionRef.current = null
      handler(pending.document.title, pending.content, pending.document.format)
    },
    []
  )

  return (
    <section className="op-typesetting-panel">
      <header className="op-canvas-title">{chromeContent}</header>
      <div className="op-typesetting-workbench">
        {isLibraryOpen ? (
          <button
            aria-label={t`Close library`}
            className="op-typesetting-library-backdrop"
            onClick={() => setIsLibraryOpen(false)}
            type="button"
          />
        ) : null}
        <TypesettingLibrary
          activePublicationId={activePublicationId}
          className={isLibraryOpen ? "is-open" : ""}
          insertingDocumentId={insertingDocumentId}
          isInsertDisabled={!activePublication || isContentLocked}
          onClose={() => setIsLibraryOpen(false)}
          onCreatePublication={createPublication}
          onInsertGenerated={insertGeneratedDocument}
          onOpenGenerated={openGeneratedDocument}
          onOpenGeneratedOriginal={openGeneratedOriginal}
          onOpenPublication={(publication) => {
            insertDocumentRef.current = null
            pendingInsertionRef.current = null
            setActivePublicationId(publication.id)
            setView("edit")
            setIsLibraryOpen(false)
          }}
          projectId={projectId}
          publications={state.publications}
          transport={transport}
          wiki={wiki}
        />
        <div className="op-typesetting-main">
          {activePublication ? (
            <div className="op-typesetting-publication-workspace">
              <PublicationModeHeader
                onDelete={() => setPendingDelete(activePublication)}
                onOpenLibrary={() => setIsLibraryOpen(true)}
                onRetrySave={() => flushSave().catch(() => undefined)}
                onViewChange={(nextView) => {
                  if (nextView !== "edit") insertDocumentRef.current = null
                  setView(nextView)
                }}
                publication={activePublication}
                saveError={saveError}
                saveStatus={saveStatus}
                view={view}
              />
              {view === "edit" ? (
                <PublicationDetail
                  importAsset={importAsset}
                  key={activePublication.id}
                  onDelete={() => setPendingDelete(activePublication)}
                  onFlushSave={flushSave}
                  onInsertHandlerChange={handleInsertHandlerChange}
                  onOpenAgentTasks={onOpenAgentTasks}
                  onOpenLibrary={() => setIsLibraryOpen(true)}
                  onPreview={() => setView("preview")}
                  onRetrySave={() => flushSave().catch(() => undefined)}
                  onUpdate={(updater) =>
                    updatePublication(activePublication.id, updater)
                  }
                  projectId={projectId}
                  publication={activePublication}
                  saveError={saveError}
                  saveStatus={saveStatus}
                  showHeader={false}
                  tasks={tasks}
                  transport={transport}
                  uploadAsset={uploadAsset}
                />
              ) : (
                <PublicationPreview
                  className="op-typesetting-publication-preview"
                  key={activePublication.id}
                  onEdit={() => setView("edit")}
                  onOpenSources={() => setIsLibraryOpen(true)}
                  publication={activePublication}
                  showHeader={false}
                  transport={transport}
                />
              )}
            </div>
          ) : (
            <div className="op-typesetting-selection-empty">
              <FileText size={24} />
              <p>
                {t`Select publication content from the left to edit, or create new publication content.`}
              </p>
              <Button
                className="op-typesetting-selection-empty__open"
                onPress={() => setIsLibraryOpen(true)}
                variant="primary"
              >
                <PanelLeft size={16} />
                {t`Publication content`}
              </Button>
            </div>
          )}
        </div>
      </div>

      {documentDialog ? (
        <MarkdownDialog
          closeLabel={t`Close`}
          content={documentDialog.content}
          fileName={documentDialog.document.originalFileName}
          onChange={(content) =>
            setDocumentDialog((current) =>
              current ? { ...current, content } : current
            )
          }
          onClose={() => setDocumentDialog(null)}
          onRenameFileName={renameGeneratedDocumentFile}
          onSave={saveGeneratedDocument}
          primaryAction={
            isInsertableTypesettingDocument(documentDialog.document)
              ? {
                  icon: <FileInput size={15} />,
                  isDisabled: !activePublication || isContentLocked,
                  label: t`Insert into content details`,
                  onPress: (content) =>
                    queueDocumentInsertion(documentDialog.document, content),
                }
              : undefined
          }
        />
      ) : null}

      {originalPreview ? (
        <OriginalPreviewDialog
          closeLabel={t`Close`}
          document={originalPreview.document}
          key={originalPreview.document.id}
          onClose={() => setOriginalPreview(null)}
          previewUrl={originalPreview.previewUrl}
          titleLabel={t`Original file`}
        />
      ) : null}

      {pendingDelete ? (
        <ConfirmDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Delete`}
          isBusy={false}
          message={t`This publication project and its layout content will be removed.`}
          onCancel={() => setPendingDelete(null)}
          onConfirm={() => deletePublication(pendingDelete)}
          title={t`Delete publication project?`}
        />
      ) : null}
    </section>
  )
}
