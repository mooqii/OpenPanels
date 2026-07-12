import type { StoreSnapshot } from "../canvas"
import type { AppState, BootstrapResponse } from "../types"
import {
  canvasRevisionFromState,
  canvasSnapshotFromState,
  normalizeBootstrap,
  normalizeSnapshot,
  serializeBootstrapForCompare,
} from "./api"

export interface LiveProjectMergeInput {
  current: AppState
  currentCanvasRevision: number
  currentCanvasSnapshot: StoreSnapshot | null
  remote: BootstrapResponse
}

export interface LiveProjectMergeResult {
  appState: AppState
  canvasRevision: number
  canvasSnapshot: StoreSnapshot | null
  changed: boolean
  shouldReloadCanvas: boolean
}

export function canvasAssetStoreKey(
  apiBase: string,
  projectId: string | null,
  canvasPanelId: string | null
): string | null {
  if (!(projectId && canvasPanelId)) return null
  return `${apiBase}\n${projectId}\n${canvasPanelId}`
}

export function sameSelectedShapeIds(
  left: readonly string[],
  right: readonly string[]
): boolean {
  return (
    left.length === right.length &&
    left.every((shapeId) => right.includes(shapeId))
  )
}

export function mergeLiveProjectBootstrap({
  current,
  currentCanvasRevision,
  currentCanvasSnapshot,
  remote,
}: LiveProjectMergeInput): LiveProjectMergeResult {
  let next = normalizeBootstrap(remote)
  const currentCanvasPanel = findCanvasPanel(current)
  const nextCanvasPanel = findCanvasPanel(next)
  const sameCanvasPanel =
    current.project.id === next.project.id &&
    currentCanvasPanel?.panel.id === nextCanvasPanel?.panel.id
  const nextCanvasRevision = nextCanvasPanel?.revision ?? next.revision ?? 0
  const shouldKeepLocalCanvas =
    sameCanvasPanel &&
    Boolean(currentCanvasSnapshot && nextCanvasPanel) &&
    nextCanvasRevision <= currentCanvasRevision

  let shouldReloadCanvas = false
  let keptLocalCanvasSnapshot: StoreSnapshot | null = null
  if (shouldKeepLocalCanvas && currentCanvasSnapshot) {
    keptLocalCanvasSnapshot = currentCanvasSnapshot
    next = replaceCanvasSnapshot(
      next,
      currentCanvasSnapshot,
      currentCanvasRevision
    )
  } else {
    next = preserveCanvasCamera(next, currentCanvasSnapshot?.camera)
    shouldReloadCanvas = Boolean(
      nextCanvasPanel &&
        (!sameCanvasPanel || nextCanvasRevision > currentCanvasRevision)
    )
  }

  const canvasSnapshot =
    keptLocalCanvasSnapshot ?? canvasSnapshotFromState(next)
  const canvasRevision = canvasRevisionFromState(next)
  const changed =
    shouldReloadCanvas ||
    serializeBootstrapForCompare(current) !== serializeBootstrapForCompare(next)

  return {
    appState: next,
    canvasRevision,
    canvasSnapshot,
    changed,
    shouldReloadCanvas,
  }
}

function preserveCanvasCamera(
  next: AppState,
  camera: StoreSnapshot["camera"] | null | undefined
): AppState {
  if (!camera) return next
  let replaced = false
  const panels = next.panels.map((snapshot) => {
    if (snapshot.panel.kind !== "canvas") return snapshot
    replaced = true
    return {
      ...snapshot,
      state: {
        ...normalizeSnapshot(snapshot.state as StoreSnapshot),
        camera,
      },
    }
  })
  if (!replaced) return next
  return {
    ...next,
    panels,
    state:
      next.panel.kind === "canvas"
        ? (panels.find(({ panel }) => panel.id === next.panel.id)?.state ??
          next.state)
        : next.state,
  }
}

function replaceCanvasSnapshot(
  appState: AppState,
  canvasSnapshot: StoreSnapshot,
  canvasRevision: number
): AppState {
  let replaced = false
  const panels = appState.panels.map((snapshot) => {
    if (snapshot.panel.kind !== "canvas") return snapshot
    replaced = true
    return {
      ...snapshot,
      revision: canvasRevision,
      state: canvasSnapshot,
    }
  })
  if (!replaced) return appState
  return {
    ...appState,
    panels,
    revision:
      appState.panel.kind === "canvas" ? canvasRevision : appState.revision,
    state: appState.panel.kind === "canvas" ? canvasSnapshot : appState.state,
  }
}

function findCanvasPanel(appState: AppState) {
  return appState.panels.find(({ panel }) => panel.kind === "canvas") ?? null
}
