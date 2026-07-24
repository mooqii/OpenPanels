import { describe, expect, it } from "vitest"
import type {
  ProjectTask,
  TaskStatus,
  TypesettingCanvasAsset,
  TypesettingPublicationImage,
  TypesettingState,
} from "../types"
import { isTypesettingState, normalizePanelState } from "./api"
import {
  addPublicationTitle,
  appendTypesettingTags,
  countTypesettingCharacters,
  createTypesettingPublication,
  groupTypesettingAssets,
  isInsertableTypesettingDocument,
  isSupportedTypesettingCoverImage,
  isSupportedTypesettingCoverMedia,
  isTypesettingCoverVideo,
  isTypesettingDocumentEmpty,
  isTypesettingLayoutTaskActive,
  latestTypesettingLayoutTask,
  mergeTypesettingConflict,
  moveTypesettingCover,
  parseTypesettingAssetDrag,
  plainTextToTypesettingContent,
  publicationCoverRequestPayload,
  publicationCoverTaskStatus,
  publicationLayoutRequestPayload,
  publicationLayoutTaskStatus,
  publicationTitleAfterDocumentInsert,
  publicationTitleRequestPayload,
  removePublicationTitle,
  selectPublicationTitle,
  TYPESETTING_ASSET_DRAG_TYPE,
  TYPESETTING_AUTOSAVE_DELAY_MS,
  typesettingImageClickSide,
  typesettingImagesToContent,
  typesettingInsertPosition,
  typesettingTagsFromInput,
  updatePublicationTitle,
} from "./typesetting"

describe("Typesetting state", () => {
  it("rejects malformed state and accepts complete schema v2 data", () => {
    expect(() => normalizePanelState("typesetting", {})).toThrow(
      "malformed typesetting panel state"
    )
    const publication = createTypesettingPublication(
      "publication:1",
      "2026-07-14T00:00:00Z"
    )
    expect(isTypesettingState({ publications: [publication] })).toBe(true)
    expect(
      isTypesettingState({
        publications: [{ ...publication, content: null }],
      })
    ).toBe(false)
    expect(
      isTypesettingState({
        publications: [
          {
            ...publication,
            content: { type: "doc", content: [{ type: 42 }] },
          },
        ],
      })
    ).toBe(false)
    expect(
      isTypesettingState({
        publications: [{ ...publication, tags: ["valid", 42] }],
      })
    ).toBe(false)
    expect(
      isTypesettingState({
        publications: [
          {
            ...publication,
            selectedTitleId: "title:missing",
          },
        ],
      })
    ).toBe(false)
    const {
      selectedTitleId: _selectedTitleId,
      tags: _tags,
      titles: _titles,
      ...minimalPublication
    } = publication
    expect(
      isTypesettingState({
        publications: [minimalPublication],
      })
    ).toBe(true)
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
  it("offers insertion only for text content that is ready", () => {
    expect(
      isInsertableTypesettingDocument({
        conversion: undefined,
        mimeType: "text/plain",
      })
    ).toBe(true)
    expect(
      isInsertableTypesettingDocument({
        conversion: {
          status: "ready",
        },
        mimeType: "text/markdown",
      })
    ).toBe(true)
    expect(
      isInsertableTypesettingDocument({
        conversion: {
          status: "converting",
        },
        mimeType: "text/markdown",
      })
    ).toBe(false)
    expect(
      isInsertableTypesettingDocument({
        conversion: undefined,
        mimeType: "image/png",
      })
    ).toBe(false)
  })

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
    expect(publicationTitleAfterDocumentInsert("", "Document title")).toBe(
      "Document title"
    )
    expect(
      publicationTitleAfterDocumentInsert("Publication title", "Document title")
    ).toBe("Publication title")
  })

  it("builds image nodes that retain their managed asset metadata", () => {
    expect(
      typesettingImagesToContent([
        {
          assetRef: "projects/project:1/content/asset/asset:photo/1/photo.png",
          fileName: "photo.png",
          height: 900,
          mimeType: "image/png",
          source: { kind: "upload" },
          src: "/api/assets/photo.png",
          width: 1200,
        },
      ])
    ).toEqual([
      {
        type: "image",
        attrs: {
          alt: "photo.png",
          assetRef: "projects/project:1/content/asset/asset:photo/1/photo.png",
          height: 900,
          src: "/api/assets/photo.png",
          title: "photo.png",
          width: 1200,
        },
      },
    ])
  })

  it("maps clicks beside an image to insertion positions", () => {
    expect(typesettingImageClickSide(99, 100, 300)).toBe("before")
    expect(typesettingImageClickSide(100, 100, 300)).toBe("inside")
    expect(typesettingImageClickSide(200, 100, 300)).toBe("inside")
    expect(typesettingImageClickSide(300, 100, 300)).toBe("inside")
    expect(typesettingImageClickSide(301, 100, 300)).toBe("after")
  })

  it("normalizes, splits, and deduplicates publication tags", () => {
    expect(typesettingTagsFromInput(" #AI， 写作,AI\n效率 ")).toEqual([
      "AI",
      "写作",
      "效率",
    ])
    expect(appendTypesettingTags(["设计"], "设计, Design, DESIGN")).toEqual([
      "设计",
      "Design",
    ])
  })

  it("adds, selects, edits, and removes title alternatives", () => {
    const publication = createTypesettingPublication(
      "publication:titles",
      "2026-07-22T00:00:00Z"
    )
    const primaryId = publication.selectedTitleId as string
    const withPrimary = updatePublicationTitle(
      publication,
      primaryId,
      "Primary title"
    )
    const withAlternative = addPublicationTitle(withPrimary, {
      id: "title:alternative",
      value: "Channel title",
    })
    expect(withAlternative.title).toBe("Channel title")
    const editedPrimary = updatePublicationTitle(
      withAlternative,
      primaryId,
      "Edited primary"
    )
    expect(editedPrimary.title).toBe("Channel title")
    const selectedPrimary = selectPublicationTitle(editedPrimary, primaryId)
    expect(selectedPrimary.title).toBe("Edited primary")
    const removedPrimary = removePublicationTitle(selectedPrimary, primaryId)
    expect(removedPrimary.title).toBe("Channel title")
    expect(removedPrimary.selectedTitleId).toBe("title:alternative")

    const replacement = removePublicationTitle(
      removedPrimary,
      "title:alternative",
      "title:replacement"
    )
    expect(replacement.title).toBe("")
    expect(replacement.selectedTitleId).toBe("title:replacement")
    expect(replacement.titles).toEqual([{ id: "title:replacement", value: "" }])
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

  it("accepts supported cover image and video formats", () => {
    expect(
      isSupportedTypesettingCoverImage({ name: "cover.PNG", type: "" })
    ).toBe(true)
    expect(
      isSupportedTypesettingCoverMedia({ name: "cover.PNG", type: "" })
    ).toBe(true)
    expect(
      isSupportedTypesettingCoverImage({
        name: "cover.bin",
        type: "image/webp",
      })
    ).toBe(true)
    expect(
      isSupportedTypesettingCoverMedia({
        name: "cover.mp4",
        type: "",
      })
    ).toBe(true)
    expect(
      isSupportedTypesettingCoverMedia({
        name: "cover.bin",
        type: "video/webm",
      })
    ).toBe(true)
    expect(
      isSupportedTypesettingCoverImage({
        name: "cover.mp4",
        type: "video/mp4",
      })
    ).toBe(false)
    expect(
      isSupportedTypesettingCoverImage({
        name: "cover.svg",
        type: "image/svg+xml",
      })
    ).toBe(false)
    expect(
      isSupportedTypesettingCoverMedia({
        name: "cover.svg",
        type: "image/svg+xml",
      })
    ).toBe(false)
    expect(
      isSupportedTypesettingCoverMedia({
        name: "notes.txt",
        type: "text/plain",
      })
    ).toBe(false)
    expect(
      isTypesettingCoverVideo({
        mimeType: "video/mp4",
      })
    ).toBe(true)
    expect(
      isTypesettingCoverVideo({
        mimeType: "image/png",
      })
    ).toBe(false)
  })

  it("accepts uploaded cover sources in schema v2 state", () => {
    const publication = {
      ...createTypesettingPublication(
        "publication:upload",
        "2026-07-22T00:00:00Z"
      ),
      covers: [{ ...first, source: { kind: "upload" as const } }],
    }
    expect(isTypesettingState({ publications: [publication] })).toBe(true)
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
    }
    const remote: TypesettingState = {
      publications: [remoteA, remoteB],
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

  it("keeps local article edits while appending remotely generated covers", () => {
    const localPublication = {
      ...createTypesettingPublication(
        "publication:cover",
        "2026-07-14T00:00:00Z"
      ),
      covers: [first],
      title: "Local title",
    }
    const generated = generatedImage("asset:generated", "task:cover")
    const remotePublication = {
      ...localPublication,
      covers: [first, generated],
      title: "Saved title",
    }
    const merged = mergeTypesettingConflict({
      deletedIds: new Set(),
      dirtyIds: new Set([localPublication.id]),
      local: { publications: [localPublication] },
      remote: { publications: [remotePublication] },
    })

    expect(merged.publications[0]?.title).toBe("Local title")
    expect(merged.publications[0]?.covers).toEqual([first, generated])
  })

  it("does not restore a generated cover explicitly deleted during a conflict", () => {
    const publication = createTypesettingPublication(
      "publication:cover",
      "2026-07-14T00:00:00Z"
    )
    const generated = generatedImage("asset:deleted", "task:cover")
    const merged = mergeTypesettingConflict({
      deletedCoverAssetRefs: new Map([
        [publication.id, new Set([generated.assetRef])],
      ]),
      deletedIds: new Set(),
      dirtyIds: new Set([publication.id]),
      local: { publications: [publication] },
      remote: {
        publications: [{ ...publication, covers: [generated] }],
      },
    })

    expect(merged.publications[0]?.covers).toEqual([])
  })
})

describe("Typesetting cover tasks", () => {
  it("builds the cover request payload and omits blank optional instructions", () => {
    expect(
      publicationCoverRequestPayload({
        instruction: "  bold editorial collage  ",
        publicationId: "publication:1",
        requestId: "cover-request:1",
        skillId: "publication-cover-default",
      })
    ).toEqual({
      instruction: "bold editorial collage",
      publicationId: "publication:1",
      requestId: "cover-request:1",
      skillId: "publication-cover-default",
    })
    expect(
      publicationCoverRequestPayload({
        instruction: "  ",
        publicationId: "publication:1",
        requestId: "cover-request:2",
        skillId: "publication-cover-default",
      })
    ).toEqual({
      publicationId: "publication:1",
      requestId: "cover-request:2",
      skillId: "publication-cover-default",
    })
  })

  it("maps every cover task lifecycle state to its placeholder status", () => {
    expect(publicationCoverTaskStatus(task("queued"))).toBe("waiting")
    expect(publicationCoverTaskStatus(task("running"))).toBe("running")
    expect(publicationCoverTaskStatus(task("succeeded"))).toBe("saving")
    expect(publicationCoverTaskStatus(task("failed"))).toBe("failed")
    expect(publicationCoverTaskStatus(task("cancelled"))).toBe("cancelled")
  })
})

describe("Typesetting title tasks", () => {
  it("builds a normalized title generation request", () => {
    expect(
      publicationTitleRequestPayload({
        instruction: "  concise and curious  ",
        publicationId: "publication:1",
        requestId: "title-request:1",
        skillId: "publication-title-default",
      })
    ).toEqual({
      instruction: "concise and curious",
      publicationId: "publication:1",
      requestId: "title-request:1",
      skillId: "publication-title-default",
    })
  })
})

describe("Typesetting layout tasks", () => {
  it("builds layout payloads and maps terminal status to an unlocked state", () => {
    expect(
      publicationLayoutRequestPayload({
        instruction: "  emphasize headings  ",
        publicationId: "publication:1",
        requestId: "layout-request:1",
        skillId: "publication-layout-default",
      })
    ).toEqual({
      instruction: "emphasize headings",
      publicationId: "publication:1",
      requestId: "layout-request:1",
      skillId: "publication-layout-default",
    })
    expect(publicationLayoutTaskStatus(layoutTask("queued"))).toBe("waiting")
    expect(publicationLayoutTaskStatus(layoutTask("running"))).toBe("running")
    expect(publicationLayoutTaskStatus(layoutTask("succeeded"))).toBe(
      "completed"
    )
    expect(isTypesettingLayoutTaskActive(layoutTask("failed"))).toBe(false)
    expect(isTypesettingLayoutTaskActive(layoutTask("cancelled"))).toBe(false)
    expect(isTypesettingLayoutTaskActive(layoutTask("queued"))).toBe(true)
    expect(isTypesettingLayoutTaskActive(layoutTask("running"))).toBe(true)
    expect(isTypesettingLayoutTaskActive(layoutTask("superseded"))).toBe(false)
  })

  it("selects the latest layout task for the publication", () => {
    const older = { ...layoutTask("failed"), createdAt: "2026-07-20T00:00:00Z" }
    const latest = {
      ...layoutTask("queued"),
      createdAt: "2026-07-22T00:00:00Z",
    }
    expect(
      latestTypesettingLayoutTask([older, latest], "publication:1")?.createdAt
    ).toBe(latest.createdAt)
  })

  it("preserves remote layout content while merging a local title edit", () => {
    const localPublication = {
      ...createTypesettingPublication("publication:1", "2026-07-21T00:00:00Z"),
      title: "Local title",
    }
    const remoteContent = {
      type: "doc",
      content: [
        {
          type: "heading",
          attrs: { level: 2 },
          content: [{ type: "text", text: "Section" }],
        },
      ],
    }
    const merged = mergeTypesettingConflict({
      contentDirtyIds: new Set(),
      deletedIds: new Set(),
      dirtyIds: new Set([localPublication.id]),
      local: { publications: [localPublication] },
      remote: {
        publications: [{ ...localPublication, content: remoteContent }],
      },
    })
    expect(merged.publications[0]?.title).toBe("Local title")
    expect(merged.publications[0]?.content).toEqual(remoteContent)
  })
})

function image(assetRef: string): TypesettingPublicationImage {
  return {
    assetRef,
    fileName: `${assetRef}.png`,
    mimeType: "image/png",
    source: {
      assetRef: `source:${assetRef}`,
      kind: "canvas",
      panelId: "panel:canvas",
      projectId: "project:1",
    },
    src: `/api/${assetRef}.png`,
  }
}

function generatedImage(
  assetRef: string,
  taskId: string
): TypesettingPublicationImage {
  return {
    assetRef,
    fileName: `${assetRef}.png`,
    mimeType: "image/png",
    source: {
      kind: "generated",
      skillId: "publication-cover-default",
      taskId,
    },
    src: `/api/${assetRef}.png`,
  }
}

function canvasAsset(
  projectId: string,
  assetId: string
): TypesettingCanvasAsset {
  return {
    assetId,
    assetRef: `projects/${projectId}/content/asset/${assetId}/1/${assetId}.png`,
    canvasPanelId: "panel:canvas",
    id: `${projectId}:${assetId}`,
    mimeType: "image/png",
    name: `${assetId}.png`,
    projectId,
    projectTitle: projectId,
    src: `/api/${assetId}.png`,
  }
}

function task(status: TaskStatus): ProjectTask {
  return {
    createdAt: "2026-07-21T00:00:00Z",
    id: `task:${status}`,
    panelId: "panel:typesetting",
    panelKind: "typesetting",
    projectId: "project:1",
    queue: "publication",
    status,
    targetId: "publication:1",
    type: "generate_publication_cover",
    updatedAt: "2026-07-21T00:00:00Z",
  }
}

function layoutTask(status: TaskStatus): ProjectTask {
  return { ...task(status), type: "format_publication_content" }
}
