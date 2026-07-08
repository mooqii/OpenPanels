import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import {
  addWikiRawDocument,
  claimWikiTask,
  completeWikiTask,
  createOpenPanelsLocalContext,
  deleteSession,
  deleteWikiRawDocument,
  ensureCanvasBootstrap,
  extractWikiRawDocumentMarkdown,
  failWikiTask,
  getProjectBootstrap,
  getSelection,
  insertImage,
  insertPlaceholder,
  listWikiTasks,
  readActivePanel,
  readActiveSession,
  readWikiRawDocumentOriginal,
  reindexWikiRawDocument,
  reindexWikiSpace,
  revealWikiRawDocumentOriginal,
  saveSelectionState,
  setActivePanel,
  setWikiLanguage,
  writeWikiMarkdown,
  writeWikiPage,
} from "./index"

const TINY_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
  "base64"
)
const TINY_PNG_DATA_URL = `data:image/png;base64,${TINY_PNG.toString("base64")}`

describe("@openpanels/local-control", () => {
  let projectDir: string
  let previousThreadId: string | undefined
  let previousStorageDir: string | undefined
  let storageDir: string

  beforeEach(async () => {
    previousStorageDir = process.env.OPENPANELS_STORAGE_DIR
    previousThreadId = process.env.CODEX_THREAD_ID
    projectDir = await mkdtemp(join(tmpdir(), "openpanels-control-"))
    storageDir = join(projectDir, "global", ".myopenpanels")
    process.env.OPENPANELS_STORAGE_DIR = storageDir
    process.env.CODEX_THREAD_ID = "control-test"
  })

  afterEach(async () => {
    restoreEnv("OPENPANELS_STORAGE_DIR", previousStorageDir)
    restoreEnv("CODEX_THREAD_ID", previousThreadId)
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
    expect(bootstrap.storageDir).toBe(storageDir)
    expect(bootstrap.contextId).toBe("control-test")
  })

  it("bootstraps projects with wiki and canvas panels", async () => {
    const context = createOpenPanelsLocalContext(projectDir)
    const bootstrap = await getProjectBootstrap({ projectDir })

    expect(bootstrap.session.title).toBe("Project 1")
    expect(bootstrap.activePanelKind).toBe("wiki")
    expect(bootstrap.panel.kind).toBe("wiki")
    expect(bootstrap.panels.map(({ panel }) => panel.kind)).toEqual([
      "wiki",
      "canvas",
    ])
    expect(bootstrap.state).toMatchObject({
      schemaVersion: 2,
      rawDocuments: [],
      activeWikiSpaceId: "wiki:default",
    })
    expect(await readActiveSession(context)).toBe(bootstrap.session.id)
    expect(await readActivePanel(context)).toMatchObject({
      sessionId: bootstrap.session.id,
      panelId: bootstrap.panel.id,
      kind: "wiki",
    })
  })

  it("recovers when active pointer files are empty or malformed", async () => {
    const context = createOpenPanelsLocalContext(projectDir)
    await mkdir(context.paths.contextDir, { recursive: true })
    await writeFile(join(context.paths.contextDir, "active-session.json"), "")
    await writeFile(join(context.paths.contextDir, "active-panel.json"), "{")

    const bootstrap = await getProjectBootstrap({ projectDir })

    expect(bootstrap.activePanelKind).toBe("wiki")
    expect(await readActiveSession(context)).toBe(bootstrap.session.id)
    expect(await readActivePanel(context)).toMatchObject({
      sessionId: bootstrap.session.id,
      panelId: bootstrap.panel.id,
      kind: "wiki",
    })
  })

  it("queues wiki markdown ingest tasks and records process wiki space", async () => {
    const language = await setWikiLanguage({
      projectDir,
      language: "zh-CN",
    })
    expect(language.state.wikiLanguage).toBe("zh-CN")

    const added = await addWikiRawDocument({
      projectDir,
      fileName: "note.md",
      mimeType: "text/markdown",
      content: "# Note\n\nA source.",
      title: "Note",
    })
    const tasks = await listWikiTasks({ projectDir })
    expect(added.document.conversion.status).toBe("not_required")
    expect(tasks.tasks[0]).toMatchObject({
      type: "ingest_markdown_into_wiki",
      wikiSpaceId: "wiki:default",
    })

    const claimed = await claimWikiTask({
      projectDir,
      taskId: tasks.tasks[0].id,
      agentHost: "test",
      threadId: "thread",
    })
    expect(claimed.process.wikiSpaceId).toBe("wiki:default")
    expect(
      claimed.state.rawDocuments[0].ingestionByWikiSpace["wiki:default"].status
    ).toBe("ingesting")
  })

  it("queues wiki rebuild tasks when source documents change", async () => {
    const added = await addWikiRawDocument({
      projectDir,
      fileName: "note.md",
      mimeType: "text/markdown",
      content: "# Note\n\nA source.",
      title: "Note",
    })

    const written = await writeWikiMarkdown({
      projectDir,
      documentId: added.document.id,
      expectedVersion: added.document.markdownVersion,
      content: "# Note\n\nUpdated source.",
    })
    expect(written.rebuildTask).toMatchObject({
      type: "rebuild_wiki_index",
      wikiSpaceId: "wiki:default",
    })

    const reindexed = await reindexWikiSpace({ projectDir })
    expect(reindexed.task).toMatchObject({
      type: "rebuild_wiki_index",
      wikiSpaceId: "wiki:default",
    })

    const deleted = await deleteWikiRawDocument({
      projectDir,
      documentId: added.document.id,
    })
    expect(deleted.task).toMatchObject({
      type: "rebuild_wiki_index",
      wikiSpaceId: "wiki:default",
    })
    expect(deleted.state.rawDocuments).toHaveLength(0)
  })

  it("re-extracts raw documents and reindexes converted documents", async () => {
    const binary = await addWikiRawDocument({
      projectDir,
      fileName: "archive.bin",
      mimeType: "application/octet-stream",
      content: Buffer.from([1, 2, 3]),
      title: "Archive",
    })

    const extracted = await extractWikiRawDocumentMarkdown({
      projectDir,
      documentId: binary.document.id,
    })
    expect(extracted.task).toMatchObject({
      type: "convert_document_to_markdown",
      wikiSpaceId: "wiki:default",
    })
    expect(extracted.document.conversion).toMatchObject({
      status: "queued",
      taskId: extracted.task.id,
    })

    const text = await addWikiRawDocument({
      projectDir,
      fileName: "note.md",
      mimeType: "text/markdown",
      content: "# Note\n\nA source.",
      title: "Note",
    })
    const reindexed = await reindexWikiRawDocument({
      projectDir,
      documentId: text.document.id,
    })
    expect(reindexed.task).toMatchObject({
      documentId: text.document.id,
      type: "ingest_markdown_into_wiki",
      wikiSpaceId: "wiki:default",
    })
    expect(reindexed.document.ingestionByWikiSpace["wiki:default"].status).toBe(
      "queued"
    )
  })

  it("lets conversion and ingest agents write without redundant tasks", async () => {
    const added = await addWikiRawDocument({
      projectDir,
      fileName: "archive.bin",
      mimeType: "application/octet-stream",
      content: Buffer.from([1, 2, 3]),
      title: "Archive",
    })
    const conversionTaskId = added.document.conversion.taskId
    if (!conversionTaskId) throw new Error("Expected conversion task")

    const claimed = await claimWikiTask({
      projectDir,
      taskId: conversionTaskId,
      agentHost: "test",
      threadId: "thread",
    })
    expect(claimed.state.rawDocuments[0].conversion.status).toBe("converting")

    const written = await writeWikiMarkdown({
      projectDir,
      documentId: added.document.id,
      taskId: conversionTaskId,
      content: "# Archive\n\nConverted.",
    })
    expect(written.task).toBeNull()
    expect(written.rebuildTask).toBeNull()

    const completed = await completeWikiTask({
      projectDir,
      taskId: conversionTaskId,
    })
    const ingestTask = completed.state.tasks.find(
      (task) =>
        task.type === "ingest_markdown_into_wiki" &&
        task.documentId === added.document.id
    )
    expect(ingestTask).toBeTruthy()
    if (!ingestTask) throw new Error("Expected ingest task")

    const pageWrite = await writeWikiPage({
      projectDir,
      wikiSpaceId: "wiki:default",
      pagePath: "sources/archive.md",
      taskId: ingestTask.id,
      title: "Archive",
      content: "# Archive\n\nConverted.",
    })
    expect(pageWrite.task).toBeNull()
  })

  it("records failed wiki conversion status", async () => {
    const added = await addWikiRawDocument({
      projectDir,
      fileName: "archive.bin",
      mimeType: "application/octet-stream",
      content: Buffer.from([1, 2, 3]),
      title: "Archive",
    })
    const taskId = added.document.conversion.taskId
    if (!taskId) throw new Error("Expected conversion task")

    const failed = await failWikiTask({
      projectDir,
      taskId,
      error: "Unsupported binary format",
    })

    expect(failed.state.rawDocuments[0].conversion).toMatchObject({
      status: "failed",
      error: "Unsupported binary format",
    })
  })

  it("reads wiki raw document originals", async () => {
    const content = Buffer.from("binary-ish content")
    const added = await addWikiRawDocument({
      projectDir,
      fileName: "archive.bin",
      mimeType: "application/octet-stream",
      content,
      title: "Archive",
    })

    const original = await readWikiRawDocumentOriginal({
      projectDir,
      documentId: added.document.id,
    })

    expect(original.document.id).toBe(added.document.id)
    expect(original.mimeType).toBe("application/octet-stream")
    expect(original.sizeBytes).toBe(content.byteLength)
    expect(await readFile(original.filePath)).toEqual(content)
    await expect(
      readWikiRawDocumentOriginal({
        projectDir,
        documentId: "raw:missing",
      })
    ).rejects.toThrow("Wiki raw document not found")
  })

  it("returns a clear error for unsupported file manager reveal platforms", async () => {
    const added = await addWikiRawDocument({
      projectDir,
      fileName: "archive.bin",
      mimeType: "application/octet-stream",
      content: Buffer.from([1, 2, 3]),
      title: "Archive",
    })

    await expect(
      revealWikiRawDocumentOriginal(
        {
          projectDir,
          documentId: added.document.id,
        },
        { platform: "aix" }
      )
    ).rejects.toThrow("Reveal in file manager is not supported on aix")
  })

  it("switches the active project panel without changing the project", async () => {
    const first = await getProjectBootstrap({ projectDir })
    const switched = await setActivePanel({ projectDir, kind: "canvas" })

    expect(switched.session.id).toBe(first.session.id)
    expect(switched.session.title).toBe(first.session.title)
    expect(switched.activePanelKind).toBe("canvas")
    expect(switched.panel.kind).toBe("canvas")
    expect(
      await readActivePanel(createOpenPanelsLocalContext(projectDir))
    ).toMatchObject({
      sessionId: first.session.id,
      kind: "canvas",
    })
  })

  it("creates separate initial projects for separate contexts sharing storage", async () => {
    const firstContext = createOpenPanelsLocalContext(projectDir, {
      contextId: "thread-a",
      storageDir,
    })
    const secondContext = createOpenPanelsLocalContext(projectDir, {
      contextId: "thread-b",
      storageDir,
    })

    const first = await ensureCanvasBootstrap(firstContext)
    const second = await ensureCanvasBootstrap(secondContext)

    expect(first.session.id).not.toBe(second.session.id)
    expect(first.session.title).toBe("Project 1")
    expect(second.session.title).toBe("Project 2")
    const firstContextSessionIds = (
      await firstContext.runtime.listSessions()
    ).map(({ id }) => id)
    const secondContextSessionIds = (
      await secondContext.runtime.listSessions()
    ).map(({ id }) => id)
    expect(firstContextSessionIds).toEqual([
      second.session.id,
      first.session.id,
    ])
    expect(secondContextSessionIds).toEqual([
      second.session.id,
      first.session.id,
    ])
  })

  it("keeps each context active project independent", async () => {
    const firstContext = createOpenPanelsLocalContext(projectDir, {
      contextId: "thread-a",
      storageDir,
    })
    const secondContext = createOpenPanelsLocalContext(projectDir, {
      contextId: "thread-b",
      storageDir,
    })
    const first = await ensureCanvasBootstrap(firstContext)
    const second = await ensureCanvasBootstrap(secondContext)
    const firstContextExtraSession = await firstContext.runtime.createSession({
      title: "Thread A Extra",
    })

    await ensureCanvasBootstrap(firstContext, firstContextExtraSession.id)
    const secondReloaded = await ensureCanvasBootstrap(secondContext)

    expect(await readActiveSession(firstContext)).toBe(
      firstContextExtraSession.id
    )
    expect(await readActiveSession(secondContext)).toBe(second.session.id)
    expect(secondReloaded.session.id).toBe(second.session.id)
    const secondContextSessionIds = (
      await secondContext.runtime.listSessions()
    ).map(({ id }) => id)
    expect(secondContextSessionIds).toContain(first.session.id)
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

  it("deletes a project while keeping an active project", async () => {
    const context = createOpenPanelsLocalContext(projectDir)
    const first = await ensureCanvasBootstrap(context)
    const secondSession = await context.runtime.createSession({
      title: "Second",
    })
    await ensureCanvasBootstrap(context, secondSession.id)

    const result = await deleteSession(context, first.session.id)

    expect(result.deletedSessionId).toBe(first.session.id)
    expect(result.activeSessionId).toBe(secondSession.id)
    expect(result.sessions.map((session) => session.id)).toEqual([
      secondSession.id,
    ])
    expect(await readActiveSession(context)).toBe(secondSession.id)
    expect(await context.runtime.getSession(first.session.id)).toBeNull()
  })

  it("does not delete the last project", async () => {
    const context = createOpenPanelsLocalContext(projectDir)
    const bootstrap = await ensureCanvasBootstrap(context)

    await expect(deleteSession(context, bootstrap.session.id)).rejects.toThrow(
      "At least one project must remain"
    )
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

function restoreEnv(name: string, value: string | undefined) {
  if (value === undefined) {
    delete process.env[name]
    return
  }
  process.env[name] = value
}
