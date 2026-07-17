import { type ReactNode, useCallback, useEffect, useRef, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import {
  apiFetch,
  apiJson,
  isTypesettingState,
  originalPreviewKind,
  savePanelState,
  tryOpenBrowserWindow,
  wikiRawOriginalUrl,
} from "../../lib/api"
import { randomId } from "../../lib/id"
import {
  createTypesettingPublication,
  mergeTypesettingConflict,
  TYPESETTING_AUTOSAVE_DELAY_MS,
} from "../../lib/typesetting"
import type {
  MyOpenPanelsTransport,
  TypesettingCanvasAsset,
  TypesettingPublication,
  TypesettingPublicationImage,
  TypesettingState,
  WikiGeneratedDocument,
  WikiRawDocument,
  WikiState,
} from "../../types"
import { ConfirmDialog } from "../wiki/Dialogs"

import { PublicationList, TypesettingLibrary } from "./TypesettingLibrary"
import { PublicationDetail } from "./TypesettingPublication"
import { DocumentPreviewDialog } from "./TypesettingToolbar"

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

interface ImportedTypesettingAsset {
  assetRef: string
  fileName: string
  mimeType: string
  sourceAssetRef: string
  sourceCanvasPanelId: string
  sourceProjectId: string
  src: string
}

export function TypesettingPanel({
  chromeContent,
  onStateSaved,
  panelId,
  projectId,
  revision,
  state: initialState,
  transport,
  wiki,
}: {
  chromeContent: ReactNode
  onStateSaved: (state: TypesettingState, revision: number) => void
  panelId: string
  projectId: string
  revision: number
  state: TypesettingState
  transport: MyOpenPanelsTransport
  wiki: WikiState
}) {
  const { t } = useMyOpenPanelsI18n()
  const [state, setState] = useState(initialState)
  const [view, setView] = useState<"list" | "detail">("list")
  const [activePublicationId, setActivePublicationId] = useState<string | null>(
    null
  )
  const [pendingDelete, setPendingDelete] =
    useState<TypesettingPublication | null>(null)
  const [preview, setPreview] = useState<DocumentPreview | null>(null)
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("saved")
  const [saveError, setSaveError] = useState<string | null>(null)
  const [saveGeneration, setSaveGeneration] = useState(0)
  const [isLibraryOpen, setIsLibraryOpen] = useState(false)
  const stateRef = useRef(state)
  const revisionRef = useRef(revision)
  const dirtyIdsRef = useRef(new Set<string>())
  const deletedIdsRef = useRef(new Set<string>())
  const changeGenerationRef = useRef(0)
  const saveInFlightRef = useRef<Promise<void> | null>(null)
  const flushRef = useRef<() => Promise<void>>(async () => undefined)
  const insertDocumentRef = useRef<
    | ((title: string, content: string, format: "markdown" | "text") => void)
    | null
  >(null)

  useEffect(() => {
    stateRef.current = state
  }, [state])

  useEffect(() => {
    if (
      revision <= revisionRef.current ||
      dirtyIdsRef.current.size > 0 ||
      deletedIdsRef.current.size > 0
    ) {
      return
    }
    revisionRef.current = revision
    stateRef.current = initialState
    setState(initialState)
  }, [initialState, revision])

  const replaceState = useCallback(
    (
      next: TypesettingState,
      publicationId: string,
      options?: { deleted?: boolean }
    ) => {
      stateRef.current = next
      setState(next)
      dirtyIdsRef.current.add(publicationId)
      if (options?.deleted) deletedIdsRef.current.add(publicationId)
      else deletedIdsRef.current.delete(publicationId)
      changeGenerationRef.current += 1
      setSaveGeneration(changeGenerationRef.current)
      setSaveStatus("saving")
      setSaveError(null)
    },
    []
  )

  const updatePublication = useCallback(
    (
      publicationId: string,
      updater: (publication: TypesettingPublication) => TypesettingPublication
    ) => {
      const current = stateRef.current
      const publications = current.publications.map((publication) =>
        publication.id === publicationId ? updater(publication) : publication
      )
      replaceState({ ...current, publications }, publicationId)
    },
    [replaceState]
  )

  const flushSave = useCallback(async () => {
    if (saveInFlightRef.current) {
      await saveInFlightRef.current
      if (dirtyIdsRef.current.size > 0 || deletedIdsRef.current.size > 0) {
        await flushRef.current()
      }
      return
    }
    if (dirtyIdsRef.current.size === 0 && deletedIdsRef.current.size === 0) {
      return
    }

    const save = (async () => {
      let payloadState = stateRef.current
      let generation = changeGenerationRef.current
      try {
        let saved: { revision: number }
        try {
          saved = await savePanelState(
            transport,
            projectId,
            panelId,
            payloadState,
            revisionRef.current
          )
        } catch (error) {
          if (!(error instanceof Error && error.message === "HTTP 409")) {
            throw error
          }
          const remote = await apiJson<{
            revision: number
            state: unknown
          }>(
            transport.apiBase,
            `/api/projects/${encodeURIComponent(projectId)}/panels/${encodeURIComponent(panelId)}/state`
          )
          if (!isTypesettingState(remote.state)) {
            throw new Error("Invalid remote Typesetting state")
          }
          payloadState = mergeTypesettingConflict({
            deletedIds: deletedIdsRef.current,
            dirtyIds: dirtyIdsRef.current,
            local: stateRef.current,
            remote: remote.state,
          })
          generation = changeGenerationRef.current
          stateRef.current = payloadState
          setState(payloadState)
          saved = await savePanelState(
            transport,
            projectId,
            panelId,
            payloadState,
            remote.revision
          )
        }

        revisionRef.current = saved.revision
        onStateSaved(payloadState, saved.revision)
        if (changeGenerationRef.current === generation) {
          dirtyIdsRef.current.clear()
          deletedIdsRef.current.clear()
          setSaveStatus("saved")
        }
      } catch (error) {
        setSaveStatus("failed")
        setSaveError(String(error instanceof Error ? error.message : error))
      } finally {
        saveInFlightRef.current = null
      }
    })()
    saveInFlightRef.current = save
    await save
  }, [onStateSaved, panelId, projectId, transport])

  useEffect(() => {
    flushRef.current = flushSave
  }, [flushSave])

  useEffect(() => {
    if (saveStatus !== "saving") return
    const timer = window.setTimeout(() => {
      if (saveGeneration !== changeGenerationRef.current) return
      flushSave().catch(() => undefined)
    }, TYPESETTING_AUTOSAVE_DELAY_MS)
    return () => window.clearTimeout(timer)
  }, [flushSave, saveGeneration, saveStatus])

  useEffect(
    () => () => {
      flushRef.current().catch(() => undefined)
    },
    []
  )

  const createPublication = useCallback(() => {
    const timestamp = new Date().toISOString()
    const publication = createTypesettingPublication(
      randomId("publication"),
      timestamp
    )
    const next = {
      ...stateRef.current,
      publications: [publication, ...stateRef.current.publications],
    }
    replaceState(next, publication.id)
    setActivePublicationId(publication.id)
    setView("detail")
  }, [replaceState])

  const deletePublication = useCallback(
    (publication: TypesettingPublication) => {
      const next = {
        ...stateRef.current,
        publications: stateRef.current.publications.filter(
          (candidate) => candidate.id !== publication.id
        ),
      }
      replaceState(next, publication.id, { deleted: true })
      setPendingDelete(null)
      if (activePublicationId === publication.id) {
        setActivePublicationId(null)
        setView("list")
      }
    },
    [activePublicationId, replaceState]
  )

  const importAsset = useCallback(
    async (
      asset: TypesettingCanvasAsset
    ): Promise<TypesettingPublicationImage> => {
      const imported = await apiJson<ImportedTypesettingAsset>(
        transport.apiBase,
        `/api/projects/${encodeURIComponent(projectId)}/panels/${encodeURIComponent(panelId)}/assets/import`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ sourceAssetRef: asset.assetRef }),
        }
      )
      return {
        assetRef: imported.assetRef,
        fileName: imported.fileName,
        height: asset.height,
        mimeType: imported.mimeType || asset.mimeType,
        sourceAssetRef: imported.sourceAssetRef,
        sourceCanvasPanelId: imported.sourceCanvasPanelId,
        sourceProjectId: imported.sourceProjectId,
        src: imported.src,
        width: asset.width,
      }
    },
    [panelId, projectId, transport.apiBase]
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
          className={isLibraryOpen ? "is-open" : ""}
          onClose={() => setIsLibraryOpen(false)}
          onOpenGenerated={openGeneratedDocument}
          onOpenRaw={openRawDocument}
          onOpenRawOriginal={openRawOriginal}
          projectId={projectId}
          transport={transport}
          wiki={wiki}
        />
        <main className="op-typesetting-main">
          {view === "detail" && activePublication ? (
            <PublicationDetail
              importAsset={importAsset}
              key={activePublication.id}
              onBack={() => {
                setView("list")
                setActivePublicationId(null)
              }}
              onDelete={() => setPendingDelete(activePublication)}
              onInsertHandlerChange={(handler) => {
                insertDocumentRef.current = handler
              }}
              onOpenLibrary={() => setIsLibraryOpen(true)}
              onRetrySave={() => flushSave().catch(() => undefined)}
              onUpdate={(updater) =>
                updatePublication(activePublication.id, updater)
              }
              publication={activePublication}
              saveError={saveError}
              saveStatus={saveStatus}
              transport={transport}
            />
          ) : (
            <PublicationList
              onCreate={createPublication}
              onOpen={(publication) => {
                setActivePublicationId(publication.id)
                setView("detail")
              }}
              onOpenLibrary={() => setIsLibraryOpen(true)}
              onRetrySave={() => flushSave().catch(() => undefined)}
              publications={state.publications}
              saveError={saveError}
              saveStatus={saveStatus}
              transport={transport}
            />
          )}
        </main>
      </div>

      {preview ? (
        <DocumentPreviewDialog
          activePublication={Boolean(activePublication)}
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
