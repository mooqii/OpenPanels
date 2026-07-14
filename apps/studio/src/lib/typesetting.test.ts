import { describe, expect, it } from "vitest"
import type {
  TypesettingCanvasAsset,
  TypesettingPublicationImage,
  TypesettingState,
} from "../types"
import { isTypesettingState, normalizePanelState } from "./api"
import {
  countTypesettingCharacters,
  createTypesettingPublication,
  groupTypesettingAssets,
  isTypesettingDocumentEmpty,
  mergeTypesettingConflict,
  moveTypesettingCover,
  parseTypesettingAssetDrag,
  plainTextToTypesettingContent,
  TYPESETTING_ASSET_DRAG_TYPE,
  TYPESETTING_AUTOSAVE_DELAY_MS,
  typesettingInsertPosition,
  typesettingTitleAfterDocumentInsert,
} from "./typesetting"

describe("Typesetting state", () => {
  it("normalizes malformed state and accepts complete schema v1 data", () => {
    expect(normalizePanelState("typesetting", { schemaVersion: 9 })).toEqual({
      publications: [],
      schemaVersion: 1,
    })
    const publication = createTypesettingPublication(
      "publication:1",
      "2026-07-14T00:00:00Z"
    )
    expect(
      isTypesettingState({ schemaVersion: 1, publications: [publication] })
    ).toBe(true)
    expect(
      isTypesettingState({
        schemaVersion: 1,
        publications: [{ ...publication, content: null }],
      })
    ).toBe(false)
    expect(
      isTypesettingState({
        schemaVersion: 1,
        publications: [
          {
            ...publication,
            content: { type: "doc", content: [{ type: 42 }] },
          },
        ],
      })
    ).toBe(false)
  })

  it("detects text and image content while treating an empty paragraph as empty", () => {
    expect(
      isTypesettingDocumentEmpty({
        type: "doc",
        content: [{ type: "paragraph" }],
      })
    ).toBe(true)
    expect(
      isTypesettingDocumentEmpty({
        type: "doc",
        content: [
          { type: "paragraph", content: [{ type: "text", text: "Hello" }] },
        ],
      })
    ).toBe(false)
    expect(
      isTypesettingDocumentEmpty({
        type: "doc",
        content: [{ type: "image", attrs: { src: "/asset.png" } }],
      })
    ).toBe(false)
  })

  it("counts non-whitespace characters across nested document nodes", () => {
    expect(
      countTypesettingCharacters({
        type: "doc",
        content: [
          {
            type: "paragraph",
            content: [
              { type: "text", text: "Hello " },
              { type: "text", text: "世界" },
            ],
          },
          { type: "paragraph", content: [{ type: "text", text: "Again" }] },
        ],
      })
    ).toBe(12)
  })
})

describe("Typesetting document insertion", () => {
  it("converts plain text paragraphs and hard breaks", () => {
    expect(plainTextToTypesettingContent("First\nline\n\nSecond")).toEqual([
      {
        type: "paragraph",
        content: [
          { type: "text", text: "First" },
          { type: "hardBreak" },
          { type: "text", text: "line" },
        ],
      },
      { type: "paragraph", content: [{ type: "text", text: "Second" }] },
    ])
  })

  it("uses the last selection when valid and otherwise appends", () => {
    expect(typesettingInsertPosition(20, 7)).toBe(7)
    expect(typesettingInsertPosition(20, null)).toBe(20)
    expect(typesettingInsertPosition(20, 80)).toBe(20)
  })

  it("fills only an empty publication title", () => {
    expect(typesettingTitleAfterDocumentInsert("", "Document title")).toBe(
      "Document title"
    )
    expect(
      typesettingTitleAfterDocumentInsert("Publication title", "Document title")
    ).toBe("Publication title")
  })
})

describe("Typesetting assets and persistence", () => {
  const first = image("asset:first")
  const second = image("asset:second")

  it("parses only the dedicated Canvas asset drag payload", () => {
    const asset = canvasAsset("project:1", "asset:1")
    const transfer = {
      getData: (type: string) =>
        type === TYPESETTING_ASSET_DRAG_TYPE ? JSON.stringify(asset) : "",
    }
    expect(parseTypesettingAssetDrag(transfer)).toEqual(asset)
    expect(parseTypesettingAssetDrag({ getData: () => "{}" })).toBeNull()
  })

  it("groups assets in input order and reorders covers without mutation", () => {
    const assets = [
      canvasAsset("project:current", "asset:1"),
      canvasAsset("project:current", "asset:2"),
      canvasAsset("project:other", "asset:3"),
    ]
    expect(
      groupTypesettingAssets(assets).map((group) => group.projectId)
    ).toEqual(["project:current", "project:other"])
    const covers = [first, second]
    expect(moveTypesettingCover(covers, 1, 0)).toEqual([second, first])
    expect(covers).toEqual([first, second])
  })

  it("merges only dirty publication ids and preserves remote work", () => {
    const localA = {
      ...createTypesettingPublication("publication:a", "2026-07-14T00:00:00Z"),
      title: "Local A",
    }
    const remoteA = { ...localA, title: "Remote A" }
    const remoteB = {
      ...createTypesettingPublication("publication:b", "2026-07-14T00:00:00Z"),
      title: "Remote B",
    }
    const local: TypesettingState = {
      publications: [localA],
      schemaVersion: 1,
    }
    const remote: TypesettingState = {
      publications: [remoteA, remoteB],
      schemaVersion: 1,
    }
    expect(
      mergeTypesettingConflict({
        deletedIds: new Set(),
        dirtyIds: new Set(["publication:a"]),
        local,
        remote,
      }).publications.map(({ title }) => title)
    ).toEqual(["Local A", "Remote B"])
    expect(TYPESETTING_AUTOSAVE_DELAY_MS).toBe(500)
  })
})

function image(assetRef: string): TypesettingPublicationImage {
  return {
    assetRef,
    fileName: `${assetRef}.png`,
    mimeType: "image/png",
    sourceAssetRef: `source:${assetRef}`,
    sourceCanvasPanelId: "panel:canvas",
    sourceProjectId: "project:1",
    src: `/api/${assetRef}.png`,
  }
}

function canvasAsset(
  projectId: string,
  assetId: string
): TypesettingCanvasAsset {
  return {
    assetId,
    assetRef: `projects/${projectId}/panels/panel:canvas/assets/${assetId}.png`,
    canvasPanelId: "panel:canvas",
    id: `${projectId}:${assetId}`,
    mimeType: "image/png",
    name: `${assetId}.png`,
    projectId,
    projectTitle: projectId,
    src: `/api/${assetId}.png`,
  }
}
