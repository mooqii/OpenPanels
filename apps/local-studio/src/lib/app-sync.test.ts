import { describe, expect, it } from "vitest"
import type { StoreSnapshot } from "../canvas"
import { PageId } from "../canvas/types/ids"
import { createEmptySnapshot } from "../canvas/types/records"
import type { OpenPanelsPanel, OpenPanelsPanelKind } from "../protocol"
import type {
  AppState,
  BootstrapResponse,
  PanelStateSnapshot,
  ProjectTask,
  WikiState,
} from "../types"
import { canvasAssetStoreKey, mergeLiveProjectBootstrap } from "./app-sync"

describe("mergeLiveProjectBootstrap", () => {
  it("keeps the local canvas snapshot for selection-only storage events", () => {
    const localCanvas = canvasSnapshot("Local edits", 120)
    const current = appState({ canvasRevision: 7, canvasSnapshot: localCanvas })
    const remote = appState({
      canvasRevision: 7,
      canvasSnapshot: canvasSnapshot("Server snapshot", -10),
    })

    const result = mergeLiveProjectBootstrap({
      current,
      currentCanvasRevision: 7,
      currentCanvasSnapshot: localCanvas,
      remote,
    })

    expect(result.changed).toBe(false)
    expect(result.shouldReloadCanvas).toBe(false)
    expect(result.canvasSnapshot).toBe(localCanvas)
  })

  it("updates task chrome without reloading the canvas", () => {
    const localCanvas = canvasSnapshot("Local edits", 120)
    const current = appState({ canvasRevision: 7, canvasSnapshot: localCanvas })
    const task = projectTask("task:queued")
    const remote = appState({
      canvasRevision: 7,
      canvasSnapshot: canvasSnapshot("Server snapshot", -10),
      pendingTaskCount: 1,
      tasks: [task],
    })

    const result = mergeLiveProjectBootstrap({
      current,
      currentCanvasRevision: 7,
      currentCanvasSnapshot: localCanvas,
      remote,
    })

    expect(result.changed).toBe(true)
    expect(result.shouldReloadCanvas).toBe(false)
    expect(result.canvasSnapshot).toBe(localCanvas)
    expect(result.appState.pendingTaskCount).toBe(1)
    expect(result.appState.tasks).toEqual([task])
  })

  it("reloads remote canvas state only when the canvas revision increases", () => {
    const localCanvas = canvasSnapshot("Local edits", 120)
    const remoteCanvas = canvasSnapshot("Remote insert", -10)
    const current = appState({ canvasRevision: 7, canvasSnapshot: localCanvas })
    const remote = appState({
      canvasRevision: 8,
      canvasSnapshot: remoteCanvas,
    })

    const result = mergeLiveProjectBootstrap({
      current,
      currentCanvasRevision: 7,
      currentCanvasSnapshot: localCanvas,
      remote,
    })

    expect(result.changed).toBe(true)
    expect(result.shouldReloadCanvas).toBe(true)
    expect(result.canvasRevision).toBe(8)
    expect(result.canvasSnapshot?.camera?.x).toBe(120)
    expect(result.canvasSnapshot?.store["page:main"]).toMatchObject({
      name: "Remote insert",
    })
  })
})

describe("canvasAssetStoreKey", () => {
  it("depends on stable ids rather than panel object identity", () => {
    const firstPanel = panel("canvas", "panel:canvas")
    const secondPanel = panel("canvas", "panel:canvas")

    expect(
      canvasAssetStoreKey("http://127.0.0.1:3000", "session:1", firstPanel.id)
    ).toBe(
      canvasAssetStoreKey("http://127.0.0.1:3000", "session:1", secondPanel.id)
    )
  })
})

function appState({
  canvasRevision,
  canvasSnapshot,
  pendingTaskCount = 0,
  tasks = [],
}: {
  canvasRevision: number
  canvasSnapshot: StoreSnapshot
  pendingTaskCount?: number
  tasks?: ProjectTask[]
}): AppState & BootstrapResponse {
  const session = {
    createdAt: "2026-07-09T00:00:00.000Z",
    id: "session:1",
    panelIds: ["panel:wiki", "panel:canvas"],
    title: "Project",
    updatedAt: "2026-07-09T00:00:00.000Z",
  }
  const wikiPanel = panel("wiki", "panel:wiki")
  const canvasPanel = panel("canvas", "panel:canvas")
  const wikiSnapshot: PanelStateSnapshot = {
    panel: wikiPanel,
    revision: 3,
    state: wikiState(),
  }
  const canvasPanelSnapshot: PanelStateSnapshot = {
    panel: canvasPanel,
    revision: canvasRevision,
    state: canvasSnapshot,
  }
  return {
    activePanelId: canvasPanel.id,
    activePanelKind: "canvas",
    panel: canvasPanel,
    panels: [wikiSnapshot, canvasPanelSnapshot],
    pendingTaskCount,
    revision: canvasRevision,
    session,
    state: canvasSnapshot,
    tasks,
  }
}

function panel(kind: OpenPanelsPanelKind, id: string): OpenPanelsPanel {
  return {
    createdAt: "2026-07-09T00:00:00.000Z",
    id,
    kind,
    sessionId: "session:1",
    title: kind,
    updatedAt: "2026-07-09T00:00:00.000Z",
  }
}

function canvasSnapshot(pageName: string, cameraX: number): StoreSnapshot {
  return {
    ...createEmptySnapshot(),
    camera: { x: cameraX, y: 0, zoom: 1 },
    currentPageId: PageId.from("page:main"),
    store: {
      "page:main": {
        id: PageId.from("page:main"),
        index: 1,
        name: pageName,
        typeName: "page",
      },
    },
  }
}

function wikiState(): WikiState {
  return {
    activeRawDocumentId: null,
    activeWikiPagePath: "index.md",
    activeWikiSpaceId: "wiki:default",
    rawDocuments: [],
    ruleSets: [],
    schemaVersion: 2,
    tasks: [],
    wikiSpaces: [],
  }
}

function projectTask(id: string): ProjectTask {
  return {
    createdAt: "2026-07-09T00:00:00.000Z",
    id,
    panelId: "panel:wiki",
    panelKind: "wiki",
    queue: "wiki",
    sessionId: "session:1",
    status: "queued",
    targetId: "target",
    task: { id },
    type: "demo",
    updatedAt: "2026-07-09T00:00:00.000Z",
  }
}
