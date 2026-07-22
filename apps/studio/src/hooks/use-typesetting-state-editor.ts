import { useCallback, useEffect, useRef, useState } from "react"
import {
  apiJson,
  fileToDataUrl,
  isTypesettingState,
  savePanelState,
} from "../lib/api"
import {
  mergeTypesettingConflict,
  TYPESETTING_AUTOSAVE_DELAY_MS,
} from "../lib/typesetting"
import type {
  MyOpenPanelsTransport,
  TypesettingCanvasAsset,
  TypesettingPublication,
  TypesettingPublicationImage,
  TypesettingState,
} from "../types"

export type TypesettingSaveStatus = "saved" | "saving" | "failed"

interface ImportedTypesettingAsset {
  assetRef: string
  fileName: string
  mimeType: string
  sourceAssetRef: string
  sourceCanvasPanelId: string
  sourceProjectId: string
  src: string
}

interface UploadedTypesettingAsset {
  assetRef: string
  fileName: string
  mimeType: string
  src: string
}

export function useTypesettingStateEditor({
  initialState,
  onStateSaved,
  panelId,
  projectId,
  revision,
  transport,
}: {
  initialState: TypesettingState
  onStateSaved: (state: TypesettingState, revision: number) => void
  panelId: string
  projectId: string
  revision: number
  transport: MyOpenPanelsTransport
}) {
  const [state, setState] = useState(initialState)
  const [saveStatus, setSaveStatus] = useState<TypesettingSaveStatus>("saved")
  const [saveError, setSaveError] = useState<string | null>(null)
  const [saveGeneration, setSaveGeneration] = useState(0)
  const stateRef = useRef(state)
  const revisionRef = useRef(revision)
  const dirtyIdsRef = useRef(new Set<string>())
  const contentDirtyIdsRef = useRef(new Set<string>())
  const deletedIdsRef = useRef(new Set<string>())
  const deletedCoverAssetRefsRef = useRef(new Map<string, Set<string>>())
  const changeGenerationRef = useRef(0)
  const saveInFlightRef = useRef<Promise<void> | null>(null)
  const flushRef = useRef<() => Promise<void>>(async () => undefined)

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
      const publications = current.publications.map((publication) => {
        if (publication.id !== publicationId) return publication
        const updated = updater(publication)
        if (
          JSON.stringify(updated.content) !==
          JSON.stringify(publication.content)
        ) {
          contentDirtyIdsRef.current.add(publicationId)
        }
        const nextRefs = new Set(updated.covers.map((cover) => cover.assetRef))
        const removed = publication.covers
          .filter((cover) => !nextRefs.has(cover.assetRef))
          .map((cover) => cover.assetRef)
        if (removed.length) {
          const deleted =
            deletedCoverAssetRefsRef.current.get(publicationId) ?? new Set()
          for (const assetRef of removed) deleted.add(assetRef)
          deletedCoverAssetRefsRef.current.set(publicationId, deleted)
        }
        const restored = new Set(updated.covers.map((cover) => cover.assetRef))
        const deleted = deletedCoverAssetRefsRef.current.get(publicationId)
        if (deleted) {
          for (const assetRef of restored) deleted.delete(assetRef)
          if (deleted.size === 0) {
            deletedCoverAssetRefsRef.current.delete(publicationId)
          }
        }
        return updated
      })
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
            contentDirtyIds: contentDirtyIdsRef.current,
            deletedCoverAssetRefs: deletedCoverAssetRefsRef.current,
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
          contentDirtyIdsRef.current.clear()
          deletedIdsRef.current.clear()
          deletedCoverAssetRefsRef.current.clear()
          setSaveStatus("saved")
        }
      } catch (error) {
        setSaveStatus("failed")
        setSaveError(String(error instanceof Error ? error.message : error))
        throw error
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
        source: {
          assetRef: imported.sourceAssetRef,
          kind: "canvas",
          panelId: imported.sourceCanvasPanelId,
          projectId: imported.sourceProjectId,
        },
        src: imported.src,
        width: asset.width,
      }
    },
    [panelId, projectId, transport.apiBase]
  )

  const uploadAsset = useCallback(
    async (file: File): Promise<TypesettingPublicationImage> => {
      const uploaded = await apiJson<UploadedTypesettingAsset>(
        transport.apiBase,
        `/api/projects/${encodeURIComponent(projectId)}/panels/${encodeURIComponent(panelId)}/assets`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            dataUrl: await fileToDataUrl(file),
            fileName: file.name || "image.png",
            mimeType: file.type || "image/png",
          }),
        }
      )
      return {
        assetRef: uploaded.assetRef,
        fileName: uploaded.fileName,
        mimeType: uploaded.mimeType,
        source: { kind: "upload" },
        src: uploaded.src,
      }
    },
    [panelId, projectId, transport.apiBase]
  )

  return {
    flushSave,
    importAsset,
    replaceState,
    saveError,
    saveStatus,
    state,
    updatePublication,
    uploadAsset,
  }
}
