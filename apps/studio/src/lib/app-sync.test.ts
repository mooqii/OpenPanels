import { describe, expect, it } from "vitest"
import type { StoreSnapshot } from "../canvas"
import { PageId } from "../canvas/types/ids"
import { createEmptySnapshot } from "../canvas/types/records"
import type { MyOpenPanelsPanel, MyOpenPanelsPanelKind } from "../protocol"
import type {
  AgentOperation,
  AppState,
  BootstrapResponse,
  PanelStateSnapshot,
  ProjectTask,
  PublishingState,
  TypesettingState,
  WikiState,
  WritingState,
} from "../types"
import {
  canvasAssetStoreKey,
  mergeLiveProjectBootstrap,
  sameSelectedShapeIds,
} from "./app-sync"

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

  it("updates Writing state without reloading the canvas", () => {
    const localCanvas = canvasSnapshot("Local edits", 120)
    const current = appState({ canvasRevision: 7, canvasSnapshot: localCanvas })
    const remote = appState({ canvasRevision: 7, canvasSnapshot: localCanvas })
    const writing = remote.panels.find(
      ({ panel: candidate }) => candidate.kind === "writing"
    )
    if (!writing) throw new Error("Missing Writing panel")
    writing.revision += 1
    writing.state = { ...writingState(), draft: "New prompt" }

    const result = mergeLiveProjectBootstrap({
      current,
      currentCanvasRevision: 7,
      currentCanvasSnapshot: localCanvas,
      remote,
    })

    expect(result.changed).toBe(true)
    expect(result.shouldReloadCanvas).toBe(false)
    expect(result.canvasSnapshot).toBe(localCanvas)
  })

  it("updates Typesetting state without reloading the canvas", () => {
    const localCanvas = canvasSnapshot("Local edits", 120)
    const current = appState({ canvasRevision: 7, canvasSnapshot: localCanvas })
    const remote = appState({ canvasRevision: 7, canvasSnapshot: localCanvas })
    const typesetting = remote.panels.find(
      ({ panel: candidate }) => candidate.kind === "typesetting"
    )
    if (!typesetting) throw new Error("Missing Typesetting panel")
    typesetting.revision += 1
    typesetting.state = {
      ...typesettingState(),
      publications: [
        {
          content: { type: "doc", content: [{ type: "paragraph" }] },
          covers: [],
          createdAt: "2026-07-09T00:00:00.000Z",
          id: "publication:1",
          title: "New publication",
          updatedAt: "2026-07-09T00:00:00.000Z",
        },
      ],
    }

    const result = mergeLiveProjectBootstrap({
      current,
      currentCanvasRevision: 7,
      currentCanvasSnapshot: localCanvas,
      remote,
    })

    expect(result.changed).toBe(true)
    expect(result.shouldReloadCanvas).toBe(false)
    expect(result.canvasSnapshot).toBe(localCanvas)
  })

  it("updates operation status without reloading or clearing the canvas", () => {
    const localCanvas = canvasSnapshot("Local edits", 120)
    const active = agentOperation("active")
    const current = appState({
      agentOperations: [active],
      canvasRevision: 7,
      canvasSnapshot: localCanvas,
    })
    const remote = appState({
      agentOperations: [{ ...active, status: "completed" }],
      canvasRevision: 7,
      canvasSnapshot: canvasSnapshot("Server snapshot", -10),
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
    expect(result.appState.agentOperations?.[0]?.status).toBe("completed")
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

  it("follows a panel switch made in another browser", () => {
    const localCanvas = canvasSnapshot("Local edits", 120)
    const current = appState({ canvasRevision: 7, canvasSnapshot: localCanvas })
    const remote = appState({ canvasRevision: 7, canvasSnapshot: localCanvas })
    const remoteWiki = remote.panels.find(
      ({ panel: candidate }) => candidate.kind === "wiki"
    )
    if (!remoteWiki) throw new Error("Missing Wiki panel")
    remote.activePanelId = remoteWiki.panel.id
    remote.activePanelKind = "wiki"
    remote.panel = remoteWiki.panel
    remote.revision = remoteWiki.revision
    remote.state = remoteWiki.state

    const result = mergeLiveProjectBootstrap({
      current,
      currentCanvasRevision: 7,
      currentCanvasSnapshot: localCanvas,
      remote,
    })

    expect(result.changed).toBe(true)
    expect(result.shouldReloadCanvas).toBe(false)
    expect(result.appState.activePanelKind).toBe("wiki")
    expect(result.appState.activePanelId).toBe(remoteWiki.panel.id)
    expect(result.canvasSnapshot).toBe(localCanvas)
  })
})

describe("canvasAssetStoreKey", () => {
  it("depends on stable ids rather than panel object identity", () => {
    const firstPanel = panel("canvas", "panel:canvas")
    const secondPanel = panel("canvas", "panel:canvas")

    expect(
      canvasAssetStoreKey("http://127.0.0.1:3000", "project:1", firstPanel.id)
    ).toBe(
      canvasAssetStoreKey("http://127.0.0.1:3000", "project:1", secondPanel.id)
    )
  })
})

describe("sameSelectedShapeIds", () => {
  it("treats an echoed selection as unchanged regardless of order", () => {
    expect(
      sameSelectedShapeIds(["shape:a", "shape:b"], ["shape:b", "shape:a"])
    ).toBe(true)
    expect(sameSelectedShapeIds(["shape:a"], ["shape:b"])).toBe(false)
  })
})

function appState({
  agentOperations = [],
  canvasRevision,
  canvasSnapshot,
  pendingTaskCount = 0,
  tasks = [],
}: {
  agentOperations?: AgentOperation[]
  canvasRevision: number
  canvasSnapshot: StoreSnapshot
  pendingTaskCount?: number
  tasks?: ProjectTask[]
}): AppState & BootstrapResponse {
  const project = {
    createdAt: "2026-07-09T00:00:00.000Z",
    id: "project:1",
    panelIds: [
      "panel:wiki",
      "panel:writing",
      "panel:canvas",
      "panel:typesetting",
      "panel:publishing",
    ],
    title: "Project",
    updatedAt: "2026-07-09T00:00:00.000Z",
  }
  const wikiPanel = panel("wiki", "panel:wiki")
  const canvasPanel = panel("canvas", "panel:canvas")
  const writingPanel = panel("writing", "panel:writing")
  const typesettingPanel = panel("typesetting", "panel:typesetting")
  const publishingPanel = panel("publishing", "panel:publishing")
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
  const writingSnapshot: PanelStateSnapshot = {
    panel: writingPanel,
    revision: 1,
    state: writingState(),
  }
  const typesettingSnapshot: PanelStateSnapshot = {
    panel: typesettingPanel,
    revision: 1,
    state: typesettingState(),
  }
  const publishingSnapshot: PanelStateSnapshot = {
    panel: publishingPanel,
    revision: 1,
    state: publishingState(),
  }
  return {
    activePanelId: canvasPanel.id,
    activePanelKind: "canvas",
    agentOperations,
    panel: canvasPanel,
    panels: [
      wikiSnapshot,
      writingSnapshot,
      canvasPanelSnapshot,
      typesettingSnapshot,
      publishingSnapshot,
    ],
    pendingTaskCount,
    revision: canvasRevision,
    project,
    state: canvasSnapshot,
    tasks,
  }
}

function writingState(): WritingState {
  return {
    draft: "",
    mode: "create",
    refinementName: "",
    schemaVersion: 5,
    selectedCreateWritingSkillIds: ["writing-default"],
    selectedRefinementSkillId: "writing-skill-refiner",
    selectedRevisionWritingSkillId: "writing-default",
    targetGeneratedDocumentId: null,
  }
}

function typesettingState(): TypesettingState {
  return {
    publications: [],
    schemaVersion: 1,
  }
}

function publishingState(): PublishingState {
  return {
    releases: [],
    schemaVersion: 1,
    selectedPublicationId: null,
    selectedSkillIds: { xiaohongshu: "publishing-xiaohongshu" },
  }
}

function agentOperation(status: AgentOperation["status"]): AgentOperation {
  return {
    completedAt: status === "active" ? null : "2026-07-09T00:01:00.000Z",
    createdAt: "2026-07-09T00:00:00.000Z",
    error: null,
    id: "operation:1",
    intent: "canvas.image.generate",
    panelId: "panel:canvas",
    panelKind: "canvas",
    result: null,
    projectId: "project:1",
    status,
    updatedAt: "2026-07-09T00:01:00.000Z",
  }
}

function panel(kind: MyOpenPanelsPanelKind, id: string): MyOpenPanelsPanel {
  return {
    createdAt: "2026-07-09T00:00:00.000Z",
    id,
    kind,
    projectId: "project:1",
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
    generatedDocuments: [],
    ruleSets: [],
    schemaVersion: 4,
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
    projectId: "project:1",
    status: "queued",
    targetId: "target",
    type: "demo",
    updatedAt: "2026-07-09T00:00:00.000Z",
  }
}
