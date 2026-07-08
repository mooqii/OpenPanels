import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { Writable } from "node:stream"
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import packageJson from "../package.json"
import { runOpenPanelsCli } from "./index"

const TINY_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
  "base64"
)

describe("openpanels-local CLI", () => {
  let projectDir: string
  let previousStorageDir: string | undefined
  let previousThreadId: string | undefined
  let storageDir: string

  beforeEach(async () => {
    previousStorageDir = process.env.OPENPANELS_STORAGE_DIR
    previousThreadId = process.env.CODEX_THREAD_ID
    projectDir = await mkdtemp(join(tmpdir(), "openpanels-local-cli-test-"))
    storageDir = join(projectDir, "global", ".myopenpanels")
    process.env.OPENPANELS_STORAGE_DIR = storageDir
    process.env.CODEX_THREAD_ID = "cli-test"
  })

  afterEach(async () => {
    restoreEnv("OPENPANELS_STORAGE_DIR", previousStorageDir)
    restoreEnv("CODEX_THREAD_ID", previousThreadId)
    await rm(projectDir, { recursive: true, force: true })
  })

  it("reports the CLI version", async () => {
    const output = await runCli(["--version"])

    expect(output.exitCode).toBe(0)
    expect(output.stdout).toBe(`${packageJson.version}\n`)
  })

  it("reports missing studio status as JSON", async () => {
    const output = await runCli([
      "studio",
      "status",
      "--project",
      projectDir,
      "--format",
      "json",
    ])

    expect(output.exitCode).toBe(0)
    expect(JSON.parse(output.stdout)).toMatchObject({
      ok: true,
      server: "missing",
      projectDir,
      storageDir,
      contextId: "cli-test",
      contextIdSource: "CODEX_THREAD_ID",
      contextDir: join(storageDir, "contexts", "cli-test"),
    })
  })

  it("keeps canvas state active projects isolated by agent thread id", async () => {
    process.env.CODEX_THREAD_ID = "thread-a"
    const first = await runCli([
      "canvas-state",
      "--project",
      projectDir,
      "--format",
      "json",
    ])
    process.env.CODEX_THREAD_ID = "thread-b"
    const second = await runCli([
      "canvas-state",
      "--project",
      projectDir,
      "--format",
      "json",
    ])
    process.env.CODEX_THREAD_ID = "thread-a"
    const firstAgain = await runCli([
      "canvas-state",
      "--project",
      projectDir,
      "--format",
      "json",
    ])

    const firstPayload = JSON.parse(first.stdout)
    const secondPayload = JSON.parse(second.stdout)
    expect(firstPayload.session.id).not.toBe(secondPayload.session.id)
    expect(firstPayload.session.title).toBe("Project 1")
    expect(secondPayload.session.title).toBe("Project 2")
    expect(JSON.parse(firstAgain.stdout).session.id).toBe(
      firstPayload.session.id
    )
  })

  it("reports panels, active panel, panel state, and agent context", async () => {
    const panels = await runCli([
      "panels",
      "--project",
      projectDir,
      "--format",
      "json",
    ])
    expect(panels.exitCode).toBe(0)
    const panelsPayload = JSON.parse(panels.stdout)
    expect(panelsPayload.activePanelKind).toBe("wiki")
    expect(
      panelsPayload.panels.map((panel: { kind: string }) => panel.kind)
    ).toEqual(["wiki", "canvas"])

    const switched = await runCli([
      "active-panel",
      "--project",
      projectDir,
      "--kind",
      "canvas",
      "--format",
      "json",
    ])
    expect(switched.exitCode).toBe(0)
    expect(JSON.parse(switched.stdout).activePanelKind).toBe("canvas")

    const wikiState = await runCli([
      "panel-state",
      "--project",
      projectDir,
      "--kind",
      "wiki",
      "--format",
      "json",
    ])
    expect(wikiState.exitCode).toBe(0)
    expect(JSON.parse(wikiState.stdout).state).toMatchObject({
      schemaVersion: 2,
      rawDocuments: [],
      activeWikiSpaceId: "wiki:default",
    })

    const context = await runCli([
      "agent-context",
      "--project",
      projectDir,
      "--format",
      "json",
    ])
    expect(context.exitCode).toBe(0)
    const contextPayload = JSON.parse(context.stdout)
    expect(contextPayload).toMatchObject({
      protocolVersion: 1,
      cliVersion: packageJson.version,
      activePanel: { kind: "wiki" },
      state: { wiki: { language: null } },
    })
    expect(
      contextPayload.capabilities.map(
        (capability: { intent: string }) => capability.intent
      )
    ).toContain("canvas.placeholder.create")
    expect(contextPayload.availableGuides).toContainEqual(
      expect.objectContaining({
        id: "canvas.image-generation",
        source: "builtin",
      })
    )

    const markdownContext = await runCli([
      "agent",
      "context",
      "--project",
      projectDir,
    ])
    expect(markdownContext.exitCode).toBe(0)
    expect(markdownContext.stdout).toContain("# OpenPanels Agent Context")
    expect(markdownContext.stdout).toContain("## Capabilities")

    const guides = await runCli(["agent", "guides", "--project", projectDir])
    expect(guides.exitCode).toBe(0)
    expect(guides.stdout).toContain("wiki.index-document")

    const guide = await runCli([
      "agent",
      "guide",
      "canvas.image-generation",
      "--project",
      projectDir,
    ])
    expect(guide.exitCode).toBe(0)
    expect(guide.stdout).toContain("# Guide: canvas.image-generation")
    expect(guide.stdout).toContain("## Instructions")
  })

  it("creates wiki markdown docs, tasks, and pages", async () => {
    const raw = await runCli([
      "wiki",
      "raw",
      "new-markdown",
      "--project",
      projectDir,
      "--title",
      "Research note",
      "--file-name",
      "research-note.md",
      "--content",
      "# Research note\n\nA useful source.",
      "--format",
      "json",
    ])
    expect(raw.exitCode).toBe(0)
    const rawPayload = JSON.parse(raw.stdout)
    expect(rawPayload.document.conversion.status).toBe("not_required")
    expect(
      rawPayload.document.ingestionByWikiSpace["wiki:default"].status
    ).toBe("queued")

    const nextTask = await runCli([
      "wiki",
      "tasks",
      "next",
      "--project",
      projectDir,
      "--format",
      "json",
    ])
    expect(nextTask.exitCode).toBe(0)
    expect(JSON.parse(nextTask.stdout).task).toMatchObject({
      type: "ingest_markdown_into_wiki",
      wikiSpaceId: "wiki:default",
    })
    const taskId = JSON.parse(nextTask.stdout).task.id

    const taskGuide = await runCli([
      "agent",
      "guide",
      "wiki.index-document",
      "--project",
      projectDir,
      "--task-id",
      taskId,
    ])
    expect(taskGuide.exitCode).toBe(0)
    expect(taskGuide.stdout).toContain(`- task id: ${taskId}`)
    expect(taskGuide.stdout).toContain("openpanels-local wiki tasks claim")

    const pageFile = join(projectDir, "topic.md")
    await writeFile(pageFile, "# Topic\n\nStructured page.")
    const page = await runCli([
      "wiki",
      "pages",
      "create",
      "--project",
      projectDir,
      "--wiki-space-id",
      "wiki:default",
      "--path",
      "topics/topic.md",
      "--file",
      pageFile,
      "--format",
      "json",
    ])
    expect(page.exitCode).toBe(0)
    expect(JSON.parse(page.stdout).task).toMatchObject({
      type: "rebuild_wiki_index",
      wikiSpaceId: "wiki:default",
    })
  })

  it("reads studio sessions and log paths from the current context", async () => {
    process.env.CODEX_THREAD_ID = "thread-a"
    const contextDir = join(storageDir, "contexts", "thread-a")
    const session = {
      contextDir,
      contextId: "thread-a",
      contextIdSource: "CODEX_THREAD_ID",
      logPath: join(contextDir, "studio.log"),
      pid: 999_999_999,
      port: 49_999,
      projectDir,
      serverUrl: "http://127.0.0.1:49999",
      startedAt: new Date().toISOString(),
      storageDir,
    }
    await mkdir(contextDir, { recursive: true })
    await writeFile(
      join(contextDir, "studio-session.json"),
      `${JSON.stringify(session, null, 2)}\n`,
      "utf8"
    )

    const output = await runCli([
      "studio",
      "status",
      "--project",
      projectDir,
      "--format",
      "json",
    ])

    expect(output.exitCode).toBe(0)
    expect(JSON.parse(output.stdout)).toMatchObject({
      ok: true,
      server: "stale",
      contextDir,
      contextId: "thread-a",
      logPath: join(contextDir, "studio.log"),
      session: {
        contextDir,
        contextId: "thread-a",
        storageDir,
      },
    })
  })

  it("inserts an image and reads the fallback selection", async () => {
    const imagePath = join(projectDir, "image.png")
    await writeFile(imagePath, TINY_PNG)

    const inserted = await runCli([
      "insert-image",
      "--project",
      projectDir,
      "--image",
      imagePath,
      "--format",
      "json",
    ])
    expect(inserted.exitCode).toBe(0)
    const insertedPayload = JSON.parse(inserted.stdout)

    const selection = await runCli([
      "selection",
      "--project",
      projectDir,
      "--format",
      "json",
    ])
    expect(selection.exitCode).toBe(0)
    expect(JSON.parse(selection.stdout).selection).toMatchObject({
      fallback: "last-image",
      selectedShapeIds: [insertedPayload.shapeId],
    })
  })
})

async function runCli(argv: string[]) {
  const stdout = new CaptureStream()
  const stderr = new CaptureStream()
  const exitCode = await runOpenPanelsCli(argv, { stdout, stderr })
  return { exitCode, stdout: stdout.text, stderr: stderr.text }
}

class CaptureStream extends Writable {
  text = ""

  _write(
    chunk: Buffer | string,
    _encoding: BufferEncoding,
    callback: (error?: Error | null) => void
  ) {
    this.text += chunk.toString()
    callback()
  }
}

function restoreEnv(name: string, value: string | undefined) {
  if (value === undefined) {
    delete process.env[name]
    return
  }
  process.env[name] = value
}
