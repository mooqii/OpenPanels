import { useCallback, useEffect, useRef, useState } from "react"
import { apiJson, fileToDataUrl, isTypesettingState } from "../lib/api"
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
  revision,
  transport,
}: {
  initialState: TypesettingState
  onStateSaved: (state: TypesettingState, revision: number) => void
  panelId: string
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
          saved = await savePublications(
            transport,
            payloadState,
            revisionRef.current
          )
        } catch (error) {
          if (!(error instanceof Error && error.message === "HTTP 409")) {
            throw error
          }
          const remote = await apiJson<{
            publications: TypesettingPublication[]
            revision: number
          }>(transport.apiBase, "/api/publications")
          const remoteState = { publications: remote.publications }
          if (!isTypesettingState(remoteState)) {
            throw new Error("Invalid remote Typesetting state")
          }
          payloadState = mergeTypesettingConflict({
            contentDirtyIds: contentDirtyIdsRef.current,
            deletedCoverAssetRefs: deletedCoverAssetRefsRef.current,
            deletedIds: deletedIdsRef.current,
            dirtyIds: dirtyIdsRef.current,
            local: stateRef.current,
            remote: remoteState,
          })
          generation = changeGenerationRef.current
          stateRef.current = payloadState
          setState(payloadState)
          saved = await savePublications(
            transport,
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
  }, [onStateSaved, transport])

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
        "/api/assets/import",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            sourceAssetRef: asset.assetRef,
            originPanelId: panelId,
          }),
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
    [panelId, transport.apiBase]
  )

  const uploadAsset = useCallback(
    async (file: File): Promise<TypesettingPublicationImage> => {
      const uploaded = await apiJson<UploadedTypesettingAsset>(
        transport.apiBase,
        "/api/assets",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            dataUrl: await fileToDataUrl(file),
            fileName: file.name || "image.png",
            mimeType: file.type || "image/png",
            originPanelId: panelId,
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
    [panelId, transport.apiBase]
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

async function savePublications(
  transport: MyOpenPanelsTransport,
  state: TypesettingState,
  baseRevision: number
): Promise<{ revision: number }> {
  return apiJson(transport.apiBase, "/api/publications", {
    body: JSON.stringify({
      baseRevision,
      publications: state.publications,
    }),
    headers: { "content-type": "application/json" },
    method: "PUT",
  })
}
