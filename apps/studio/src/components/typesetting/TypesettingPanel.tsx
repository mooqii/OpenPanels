import { Button } from "@heroui/react"
import { FileText, PanelLeft } from "lucide-react"
import { type ReactNode, useCallback, useRef, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { useTypesettingStateEditor } from "../../hooks/use-typesetting-state-editor"
import {
  apiFetch,
  apiJson,
  originalPreviewKind,
  tryOpenBrowserWindow,
  wikiRawOriginalUrl,
} from "../../lib/api"
import { randomId } from "../../lib/id"
import { createTypesettingPublication } from "../../lib/typesetting"
import type {
  MyOpenPanelsTransport,
  ProjectTask,
  TypesettingPublication,
  TypesettingState,
  WikiGeneratedDocument,
  WikiRawDocument,
  WikiState,
} from "../../types"
import { PublicationPreview } from "../publishing/PublicationPreview"
import { ConfirmDialog } from "../wiki/Dialogs"

import { TypesettingLibrary } from "./TypesettingLibrary"
import { PublicationDetail } from "./TypesettingPublication"
import { DocumentPreviewDialog } from "./TypesettingToolbar"

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
  const [view, setView] = useState<"edit" | "preview">("edit")
  const [activePublicationId, setActivePublicationId] = useState<string | null>(
    null
  )
  const [pendingDelete, setPendingDelete] =
    useState<TypesettingPublication | null>(null)
  const [preview, setPreview] = useState<DocumentPreview | null>(null)
  const [isLibraryOpen, setIsLibraryOpen] = useState(false)
  const insertDocumentRef = useRef<
    | ((title: string, content: string, format: "markdown" | "text") => void)
    | null
  >(null)
  const {
    flushSave,
    importAsset,
    replaceState,
    saveError,
    saveStatus,
    state,
    updatePublication,
  } = useTypesettingStateEditor({
    initialState,
    onStateSaved,
    panelId,
    projectId,
    revision,
    transport,
  })

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
        setActivePublicationId(null)
        setView("edit")
      }
    },
    [activePublicationId, replaceState, state]
  )

  const openRawDocument = useCallback(
    async (
      document: WikiRawDocument,
      requestedSource: "preferred" | "original" = "preferred"
    ) => {
      const source =
        requestedSource === "preferred" && document.markdownRef
          ? "markdown"
          : "original"
      const initial: DocumentPreview = {
        content: null,
        document,
        error: null,
        format: "markdown",
        kind: "raw",
        loading:
          source === "markdown" ||
          (source === "original" && originalPreviewKind(document) === "text"),
        source,
      }
      setPreview(initial)
      if (source === "original") {
        if (originalPreviewKind(document) !== "text") return
        try {
          const response = await apiFetch(
            transport.apiBase,
            `/api/wiki/raw-documents/${encodeURIComponent(document.id)}/original`
          )
          if (!response.ok) throw new Error(`HTTP ${response.status}`)
          const content = await response.text()
          setPreview((current) =>
            current?.kind === "raw" &&
            current.document.id === document.id &&
            current.source === "original"
              ? { ...current, content, loading: false }
              : current
          )
        } catch (error) {
          setPreview((current) =>
            current?.kind === "raw" &&
            current.document.id === document.id &&
            current.source === "original"
              ? {
                  ...current,
                  error: String(error instanceof Error ? error.message : error),
                  loading: false,
                }
              : current
          )
        }
        return
      }
      try {
        const data = await apiJson<{ markdown?: string }>(
          transport.apiBase,
          `/api/wiki/raw-documents/${encodeURIComponent(document.id)}/markdown`
        )
        setPreview((current) =>
          current?.kind === "raw" && current.document.id === document.id
            ? { ...current, content: data.markdown ?? "", loading: false }
            : current
        )
      } catch (error) {
        setPreview((current) =>
          current?.kind === "raw" && current.document.id === document.id
            ? {
                ...current,
                error: String(error instanceof Error ? error.message : error),
                loading: false,
              }
            : current
        )
      }
    },
    [transport.apiBase]
  )

  const openRawOriginal = useCallback(
    (document: WikiRawDocument) => {
      if (originalPreviewKind(document)) {
        openRawDocument(document, "original").catch((error) => {
          console.error("Failed to preview raw document", error)
        })
        return
      }
      if (
        tryOpenBrowserWindow(wikiRawOriginalUrl(transport.apiBase, document))
      ) {
        return
      }
      apiFetch(
        transport.apiBase,
        `/api/wiki/raw-documents/${encodeURIComponent(document.id)}/reveal`,
        { method: "POST" }
      ).catch((error) => {
        console.error("Failed to reveal raw document", error)
      })
    },
    [openRawDocument, transport.apiBase]
  )

  const openGeneratedDocument = useCallback(
    async (document: WikiGeneratedDocument) => {
      setPreview({
        content: null,
        document,
        error: null,
        format: document.format,
        kind: "generated",
        loading: true,
      })
      try {
        const data = await apiJson<{ content?: string }>(
          transport.apiBase,
          `/api/wiki/generated-documents/${encodeURIComponent(document.id)}`
        )
        setPreview((current) =>
          current?.kind === "generated" && current.document.id === document.id
            ? { ...current, content: data.content ?? "", loading: false }
            : current
        )
      } catch (error) {
        setPreview((current) =>
          current?.kind === "generated" && current.document.id === document.id
            ? {
                ...current,
                error: String(error instanceof Error ? error.message : error),
                loading: false,
              }
            : current
        )
      }
    },
    [transport.apiBase]
  )

  const activePublication = state.publications.find(
    (publication) => publication.id === activePublicationId
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
          onClose={() => setIsLibraryOpen(false)}
          onCreatePublication={createPublication}
          onOpenGenerated={openGeneratedDocument}
          onOpenPublication={(publication) => {
            setActivePublicationId(publication.id)
            setView("edit")
            setIsLibraryOpen(false)
          }}
          onOpenRaw={openRawDocument}
          onOpenRawOriginal={openRawOriginal}
          projectId={projectId}
          publications={state.publications}
          transport={transport}
          wiki={wiki}
        />
        <div className="op-typesetting-main">
          {activePublication && view === "edit" ? (
            <PublicationDetail
              importAsset={importAsset}
              key={activePublication.id}
              onDelete={() => setPendingDelete(activePublication)}
              onFlushSave={flushSave}
              onInsertHandlerChange={(handler) => {
                insertDocumentRef.current = handler
              }}
              onOpenAgentTasks={onOpenAgentTasks}
              onOpenLibrary={() => setIsLibraryOpen(true)}
              onPreview={() => setView("preview")}
              onRetrySave={() => flushSave().catch(() => undefined)}
              onUpdate={(updater) =>
                updatePublication(activePublication.id, updater)
              }
              publication={activePublication}
              saveError={saveError}
              saveStatus={saveStatus}
              tasks={tasks}
              transport={transport}
            />
          ) : activePublication ? (
            <PublicationPreview
              className="op-typesetting-publication-preview"
              key={activePublication.id}
              onEdit={() => setView("edit")}
              onOpenSources={() => setIsLibraryOpen(true)}
              publication={activePublication}
              transport={transport}
            />
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

      {preview ? (
        <DocumentPreviewDialog
          activePublication={Boolean(activePublication && view === "edit")}
          onClose={() => setPreview(null)}
          onInsert={() => {
            if (preview.content === null) return
            insertDocumentRef.current?.(
              preview.document.title,
              preview.content,
              preview.format
            )
            setPreview(null)
          }}
          preview={preview}
          transport={transport}
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
