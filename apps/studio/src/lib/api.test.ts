import { describe, expect, it, vi } from "vitest"
import type { MyOpenPanelsPanelKind } from "../protocol"
import type { BootstrapResponse, ProjectTask } from "../types"
import {
  appPanelState,
  myDocumentOriginalUrl,
  normalizeBootstrap,
  normalizePanelState,
  originalPreviewKind,
  tryOpenBrowserWindow,
} from "./api"

describe("Studio bootstrap contracts", () => {
  it.each([
    ["canvas", {}],
    ["wiki", { rawDocuments: [], wikiSpaces: [] }],
    ["writing", { mode: "create" }],
    ["typesetting", {}],
    ["publishing", { releases: [] }],
  ] satisfies [
    MyOpenPanelsPanelKind,
    unknown,
  ][])("surfaces malformed %s state instead of replacing it with empty data", (kind, state) => {
    expect(() => normalizePanelState(kind, state)).toThrow(
      `malformed ${kind} panel state`
    )
  })

  it("rejects non-canonical Task statuses at the bootstrap boundary", () => {
    const task = projectTask()
    expect(() =>
      normalizeBootstrap(
        bootstrap(
          "canvas",
          {
            currentPageId: null,
            openedGroupId: null,
            selectedShapeIds: [],
            store: {},
          },
          [{ ...task, status: "claimed" as ProjectTask["status"] }]
        )
      )
    ).toThrow('unsupported Task status "claimed"')
  })

  it("keeps Wiki UI state separate from hydrated resources", () => {
    const state = {
      activeRawDocumentId: null,
      activeWikiPagePath: "index.md",
      activeWikiSpaceId: "wiki:1",
      myDocuments: [],
      rawDocuments: [],
      ruleSets: [],
      wikiSpaces: [],
    }
    const normalized = normalizeBootstrap(bootstrap("wiki", state))
    const snapshot = normalized.panels[0]

    expect(snapshot.uiState).toEqual({
      activeRawDocumentId: null,
      activeWikiPagePath: "index.md",
      activeWikiSpaceId: "wiki:1",
      ruleSets: [],
    })
    expect(snapshot.moduleState).toEqual({
      myDocuments: [],
      rawDocuments: [],
      wikiSpaces: [],
    })
    expect(appPanelState(snapshot)).toEqual(state)
  })
})

describe("myDocumentOriginalUrl", () => {
  it("targets the immutable imported source", () => {
    expect(
      myDocumentOriginalUrl("http://localhost:43217", {
        id: "my-document:document/1",
      })
    ).toBe(
      "http://localhost:43217/api/my-documents/my-document%3Adocument%2F1/original"
    )
  })
})

function bootstrap(
  kind: MyOpenPanelsPanelKind,
  state: unknown,
  tasks: ProjectTask[] = []
): BootstrapResponse {
  const panel = {
    createdAt: "2026-07-24T00:00:00Z",
    id: `panel:${kind}`,
    kind,
    projectId: "project:1",
    title: kind,
    updatedAt: "2026-07-24T00:00:00Z",
  }
  return {
    activePanelId: panel.id,
    activePanelKind: kind,
    panel,
    panels: [{ panel, revision: 1, state }],
    project: {
      createdAt: "2026-07-24T00:00:00Z",
      id: "project:1",
      panelIds: [panel.id],
      title: "Project",
      updatedAt: "2026-07-24T00:00:00Z",
    },
    revision: 1,
    state,
    tasks,
  }
}

function projectTask(): ProjectTask {
  return {
    createdAt: "2026-07-24T00:00:00Z",
    id: "task:1",
    panelId: "panel:canvas",
    panelKind: "canvas",
    projectId: "project:1",
    queue: "canvas",
    status: "queued",
    targetId: "canvas:1",
    type: "canvas_image_generate",
    updatedAt: "2026-07-24T00:00:00Z",
  }
}

describe("originalPreviewKind", () => {
  it("previews plain-text documents in the current window", () => {
    expect(
      originalPreviewKind({
        mimeType: "application/octet-stream",
        originalFileName: "component.mdx",
      })
    ).toBe("text")
    expect(
      originalPreviewKind({
        mimeType: "text/plain; charset=utf-8",
        originalFileName: "notes.unknown",
      })
    ).toBe("text")
  })

  it("recognizes image extensions even when the uploaded MIME type is missing", () => {
    expect(
      originalPreviewKind({
        mimeType: "application/octet-stream",
        originalFileName: "scan.tiff",
      })
    ).toBe("image")
  })

  it("leaves unsupported documents for browser or folder fallback", () => {
    expect(
      originalPreviewKind({
        mimeType: "application/octet-stream",
        originalFileName: "archive.zip",
      })
    ).toBeNull()
  })
})

describe("tryOpenBrowserWindow", () => {
  it("isolates a successfully opened browser window", () => {
    const openedWindow = { opener: {} } as Window
    const openWindow = vi.fn(() => openedWindow)

    expect(tryOpenBrowserWindow("http://localhost/document", openWindow)).toBe(
      true
    )
    expect(openWindow).toHaveBeenCalledWith(
      "http://localhost/document",
      "_blank"
    )
    expect(openedWindow.opener).toBeNull()
  })

  it("reports a blocked or failed browser window so callers can reveal the file", () => {
    expect(tryOpenBrowserWindow("http://localhost/document", () => null)).toBe(
      false
    )
    expect(
      tryOpenBrowserWindow("http://localhost/document", () => {
        throw new Error("blocked")
      })
    ).toBe(false)
  })

  it("does not reveal a file when the browser opened it but restricts opener access", () => {
    const openedWindow = Object.defineProperty({}, "opener", {
      set() {
        throw new Error("restricted")
      },
    }) as Window

    expect(
      tryOpenBrowserWindow("http://localhost/document", () => openedWindow)
    ).toBe(true)
  })
})
