import { mkdtemp, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import {
  createOpenPanelsLocalContext,
  ensureCanvasBootstrap,
  getSelection,
  insertImage,
  insertPlaceholder,
  readActiveSession,
  saveSelectionState,
} from "./index"

const TINY_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
  "base64"
)
const TINY_PNG_DATA_URL = `data:image/png;base64,${TINY_PNG.toString("base64")}`

describe("@openpanels/local-control", () => {
  let projectDir: string

  beforeEach(async () => {
    projectDir = await mkdtemp(join(tmpdir(), "openpanels-control-"))
  })

  afterEach(async () => {
    await rm(projectDir, { recursive: true, force: true })
  })

  it("bootstraps an empty project with a canvas panel", async () => {
    const context = createOpenPanelsLocalContext(projectDir)
    const bootstrap = await ensureCanvasBootstrap(context)

    expect(bootstrap.session.title).toBe("Project 1")
    expect(bootstrap.panel.kind).toBe("canvas")
    expect(await readActiveSession(context)).toBe(bootstrap.session.id)
    expect(bootstrap.state).toMatchObject({
      currentPageId: "page:main",
    })
  })

  it("persists selection data and optional image base64", async () => {
    const context = createOpenPanelsLocalContext(projectDir)
    const bootstrap = await ensureCanvasBootstrap(context)

    await saveSelectionState({
      projectDir,
      sessionId: bootstrap.session.id,
      panelId: bootstrap.panel.id,
      selection: {
        selectedShapeIds: ["shape:1"],
        selectedShapes: [{ id: "shape:1", type: "geo" }],
      },
      imageDataUrl: TINY_PNG_DATA_URL,
    })

    const selection = await getSelection({
      projectDir,
      includeImageBase64: true,
    })

    expect(selection.base64).toBe(TINY_PNG.toString("base64"))
    expect(selection.mimeType).toBe("image/png")
    expect(selection.selection.selectedShapeIds).toEqual(["shape:1"])
  })

  it("inserts images and falls back to the latest image selection", async () => {
    const imagePath = join(projectDir, "image.png")
    await writeFile(imagePath, TINY_PNG)

    const inserted = await insertImage({ projectDir, imagePath })
    const selection = await getSelection({ projectDir })

    expect(inserted.assetRef).toContain("image.png")
    expect(inserted.bounds).toEqual({ x: 160, y: 160, width: 1, height: 1 })
    expect(selection.selection.fallback).toBe("last-image")
    expect(selection.selection.selectedShapeIds).toEqual([inserted.shapeId])
  })

  it("places generation placeholders in clear canvas space", async () => {
    const imagePath = join(projectDir, "image.png")
    await writeFile(imagePath, TINY_PNG)

    const inserted = await insertImage({
      projectDir,
      imagePath,
      displayHeight: 512,
      displayWidth: 512,
    })
    const placeholder = await insertPlaceholder({
      projectDir,
      anchorShapeId: inserted.shapeId,
      displayHeight: 512,
      displayWidth: 512,
    })

    expect(placeholder.bounds).toEqual({
      x: 160 + 512 + 80,
      y: 160,
      width: 512,
      height: 512,
    })
  })

  it("replaces a generation placeholder with the generated image", async () => {
    const imagePath = join(projectDir, "image.png")
    await writeFile(imagePath, TINY_PNG)

    const placeholder = await insertPlaceholder({
      projectDir,
      displayHeight: 256,
      displayWidth: 384,
    })
    const inserted = await insertImage({
      projectDir,
      imagePath,
      replaceShapeId: placeholder.shapeId,
    })
    const selection = await getSelection({ projectDir })

    expect(inserted.replacedShapeId).toBe(placeholder.shapeId)
    expect(inserted.bounds).toEqual({ x: 160, y: 160, width: 384, height: 256 })
    expect(selection.selection.selectedShapeIds).toEqual([inserted.shapeId])
  })
})
