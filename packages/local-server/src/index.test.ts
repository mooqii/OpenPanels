import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { createLocalOpenPanelsServer } from "./index"

describe("@openpanels/local-server", () => {
  let projectDir: string
  let previousStorageDir: string | undefined
  let previousThreadId: string | undefined
  let server: ReturnType<typeof createLocalOpenPanelsServer>
  let storageDir: string
  let baseUrl: string

  beforeEach(async () => {
    previousStorageDir = process.env.OPENPANELS_STORAGE_DIR
    previousThreadId = process.env.CODEX_THREAD_ID
    projectDir = await mkdtemp(join(tmpdir(), "openpanels-server-"))
    storageDir = join(projectDir, "global", ".myopenpanels")
    process.env.OPENPANELS_STORAGE_DIR = storageDir
    process.env.CODEX_THREAD_ID = "server-test"
    server = createLocalOpenPanelsServer({ projectDir, storageDir })
    await new Promise<void>((resolve) => {
      server.listen(0, "127.0.0.1", resolve)
    })
    const address = server.address()
    if (!address || typeof address === "string") {
      throw new Error("Expected local server address")
    }
    baseUrl = `http://127.0.0.1:${address.port}`
  })

  afterEach(async () => {
    await closeServer(server)
    restoreEnv("OPENPANELS_STORAGE_DIR", previousStorageDir)
    restoreEnv("CODEX_THREAD_ID", previousThreadId)
    await rm(projectDir, { recursive: true, force: true })
  })

  it("bootstraps a canvas and persists selection assets", async () => {
    const bootstrap = await fetchJson(`${baseUrl}/api/bootstrap`)
    const canvasPanel = bootstrap.panels.find(
      ({ panel }: { panel: { kind: string } }) => panel.kind === "canvas"
    ).panel
    const tinyPng =
      "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="

    const result = await fetchJson(
      `${baseUrl}/api/panels/${bootstrap.session.id}/${canvasPanel.id}/selection`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          selection: {
            selectedShapeIds: ["shape:1"],
            selectedShapes: [{ id: "shape:1", type: "geo" }],
          },
          imageDataUrl: tinyPng,
        }),
      }
    )

    expect(result.selection.assetRef).toContain("__selection/current.png")
    const assetResponse = await fetch(
      `${baseUrl}/api/panels/${bootstrap.session.id}/${canvasPanel.id}/assets/__selection/current.png`
    )
    expect(assetResponse.headers.get("content-type")).toBe("image/png")
    expect(await assetResponse.arrayBuffer()).toHaveProperty("byteLength")
  })

  it("preserves selected image asset refs when no selection raster is available", async () => {
    const bootstrap = await fetchJson(`${baseUrl}/api/bootstrap`)
    const canvasPanel = bootstrap.panels.find(
      ({ panel }: { panel: { kind: string } }) => panel.kind === "canvas"
    ).panel
    const tinyPng =
      "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
    const uploaded = await fetchJson(
      `${baseUrl}/api/panels/${bootstrap.session.id}/${canvasPanel.id}/assets`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          dataUrl: tinyPng,
          fileName: "selected.png",
          mimeType: "image/png",
        }),
      }
    )

    const result = await fetchJson(
      `${baseUrl}/api/panels/${bootstrap.session.id}/${canvasPanel.id}/selection`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          selection: {
            assetRef: uploaded.assetRef,
            selectedShapeIds: ["shape:image"],
            selectedShapes: [
              {
                id: "shape:image",
                type: "image",
                asset: { assetRef: uploaded.assetRef },
              },
            ],
          },
        }),
      }
    )

    expect(result.selection.assetRef).toBe(uploaded.assetRef)
  })

  it("bootstraps wiki and canvas panels and switches the active panel", async () => {
    const bootstrap = await fetchJson(`${baseUrl}/api/bootstrap`)

    expect(bootstrap.activePanelKind).toBe("wiki")
    expect(bootstrap.panel.kind).toBe("wiki")
    expect(
      bootstrap.panels.map(
        ({ panel }: { panel: { kind: string } }) => panel.kind
      )
    ).toEqual(["wiki", "canvas"])

    const switched = await fetchJson(`${baseUrl}/api/active-panel`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        sessionId: bootstrap.session.id,
        kind: "canvas",
      }),
    })

    expect(switched.activePanelKind).toBe("canvas")
    expect(switched.panel.kind).toBe("canvas")
    expect(await fetchJson(`${baseUrl}/api/active-panel`)).toMatchObject({
      activePanel: {
        sessionId: bootstrap.session.id,
        kind: "canvas",
      },
    })

    const current = await fetchJson(`${baseUrl}/api/bootstrap`)
    expect(current.session.id).toBe(bootstrap.session.id)
    expect(current.panel.kind).toBe("canvas")
  })

  it("recovers bootstrap when active pointer files are empty or malformed", async () => {
    const contextDir = join(storageDir, "contexts", "server-test")
    await mkdir(contextDir, { recursive: true })
    await writeFile(join(contextDir, "active-session.json"), "")
    await writeFile(join(contextDir, "active-panel.json"), "{")

    const bootstrap = await fetchJson(`${baseUrl}/api/bootstrap`)

    expect(bootstrap.activePanelKind).toBe("wiki")
    expect(bootstrap.panel.kind).toBe("wiki")
  })

  it("accepts wiki language writes with PUT and POST", async () => {
    const putLanguage = await fetchJson(`${baseUrl}/api/wiki/language`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ language: "zh-CN" }),
    })
    const postLanguage = await fetchJson(`${baseUrl}/api/wiki/language`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ language: "en" }),
    })

    expect(putLanguage.language).toBe("zh-CN")
    expect(postLanguage.language).toBe("en")
  })

  it("tracks the active project for browser and agent coordination", async () => {
    const first = await fetchJson(`${baseUrl}/api/bootstrap`)
    const second = await fetchJson(`${baseUrl}/api/projects`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ title: "Second" }),
    })

    expect(await fetchJson(`${baseUrl}/api/active-session`)).toMatchObject({
      sessionId: second.session.id,
    })

    const switched = await fetchJson(
      `${baseUrl}/api/bootstrap?sessionId=${encodeURIComponent(first.session.id)}`
    )
    expect(switched.session.id).toBe(first.session.id)
    expect(await fetchJson(`${baseUrl}/api/active-session`)).toMatchObject({
      sessionId: first.session.id,
    })

    await fetchJson(`${baseUrl}/api/active-session`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ sessionId: second.session.id }),
    })

    const current = await fetchJson(`${baseUrl}/api/bootstrap`)
    expect(current.session.id).toBe(second.session.id)
  })

  it("assigns sequential default project names", async () => {
    const first = await fetchJson(`${baseUrl}/api/bootstrap`)
    const second = await fetchJson(`${baseUrl}/api/projects`, {
      method: "POST",
    })
    const third = await fetchJson(`${baseUrl}/api/projects`, {
      method: "POST",
    })

    expect(first.session.title).toBe("Project 1")
    expect(second.session.title).toBe("Project 2")
    expect(third.session.title).toBe("Project 3")
  })

  it("creates wiki raw markdown docs, wakes targets, and writes wiki pages", async () => {
    const language = await fetchJson(`${baseUrl}/api/wiki/language`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ language: "zh-CN" }),
    })
    expect(language.language).toBe("zh-CN")

    const target = await fetchJson(`${baseUrl}/api/wiki/agent-targets`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        host: "test-host",
        threadId: "thread-1",
      }),
    })
    expect(target.target).toMatchObject({
      host: "test-host",
      threadId: "thread-1",
    })

    const raw = await fetchJson(`${baseUrl}/api/wiki/raw-documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        fileName: "note.md",
        mimeType: "text/markdown",
        title: "Note",
        content: "# Note\n\nUseful.",
      }),
    })
    expect(raw.document.conversion.status).toBe("not_required")
    expect(raw.document.ingestionByWikiSpace["wiki:default"].status).toBe(
      "queued"
    )

    const originalResponse = await fetch(
      `${baseUrl}/api/wiki/raw-documents/${encodeURIComponent(raw.document.id)}/original`
    )
    expect(originalResponse.status).toBe(200)
    expect(originalResponse.headers.get("content-type")).toBe("text/markdown")
    expect(originalResponse.headers.get("content-disposition")).toContain(
      "inline"
    )
    expect(await originalResponse.text()).toBe("# Note\n\nUseful.")

    const tasks = await fetchJson(`${baseUrl}/api/wiki/tasks?status=queued`)
    expect(tasks.tasks[0]).toMatchObject({
      type: "ingest_markdown_into_wiki",
      wikiSpaceId: "wiki:default",
    })

    const claim = await fetchJson(
      `${baseUrl}/api/wiki/tasks/${encodeURIComponent(tasks.tasks[0].id)}/claim`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ agentHost: "test-host", threadId: "thread-1" }),
      }
    )
    expect(claim.process.wikiSpaceId).toBe("wiki:default")

    const page = await fetchJson(
      `${baseUrl}/api/wiki/spaces/wiki%3Adefault/pages`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          pagePath: "topics/new-page.md",
          title: "New page",
          content: "# New page\n\nStructured.",
        }),
      }
    )
    expect(page.task).toMatchObject({
      type: "rebuild_wiki_index",
      wikiSpaceId: "wiki:default",
    })

    const readPage = await fetchJson(
      `${baseUrl}/api/wiki/spaces/wiki%3Adefault/pages/topics/new-page.md`
    )
    expect(readPage.markdown).toContain("# New page")

    const reindex = await fetchJson(
      `${baseUrl}/api/wiki/spaces/wiki%3Adefault/reindex`,
      { method: "POST" }
    )
    expect(reindex.task).toMatchObject({
      type: "rebuild_wiki_index",
      wikiSpaceId: "wiki:default",
    })

    const deletedRaw = await fetchJson(
      `${baseUrl}/api/wiki/raw-documents/${encodeURIComponent(raw.document.id)}?wikiSpaceId=wiki%3Adefault`,
      { method: "DELETE" }
    )
    expect(deletedRaw.task).toMatchObject({
      type: "rebuild_wiki_index",
      wikiSpaceId: "wiki:default",
    })
    expect(deletedRaw.state.rawDocuments).toHaveLength(0)
  })

  it("supports raw document extraction and reindex actions", async () => {
    const binary = await fetchJson(`${baseUrl}/api/wiki/raw-documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        fileName: "archive.bin",
        mimeType: "application/octet-stream",
        title: "Archive",
        dataUrl: "data:application/octet-stream;base64,AQID",
      }),
    })
    expect(binary.document.conversion.status).toBe("queued")

    const extracted = await fetchJson(
      `${baseUrl}/api/wiki/raw-documents/${encodeURIComponent(binary.document.id)}/extract?wikiSpaceId=wiki%3Adefault`,
      { method: "POST" }
    )
    expect(extracted.task).toMatchObject({
      type: "convert_document_to_markdown",
      wikiSpaceId: "wiki:default",
    })
    expect(extracted.document.conversion).toMatchObject({
      status: "queued",
      taskId: extracted.task.id,
    })

    const text = await fetchJson(`${baseUrl}/api/wiki/raw-documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        fileName: "note.md",
        mimeType: "text/markdown",
        title: "Note",
        content: "# Note\n\nUseful.",
      }),
    })
    const reindexed = await fetchJson(
      `${baseUrl}/api/wiki/raw-documents/${encodeURIComponent(text.document.id)}/reindex?wikiSpaceId=wiki%3Adefault`,
      { method: "POST" }
    )
    expect(reindexed.task).toMatchObject({
      documentId: text.document.id,
      type: "ingest_markdown_into_wiki",
      wikiSpaceId: "wiki:default",
    })
    expect(reindexed.document.ingestionByWikiSpace["wiki:default"].status).toBe(
      "queued"
    )
  })

  it("deletes projects but keeps the final project", async () => {
    const first = await fetchJson(`${baseUrl}/api/bootstrap`)
    const second = await fetchJson(`${baseUrl}/api/projects`, {
      method: "POST",
    })

    const deleted = await fetchJson(
      `${baseUrl}/api/sessions/${encodeURIComponent(first.session.id)}`,
      { method: "DELETE" }
    )
    expect(deleted.deletedSessionId).toBe(first.session.id)
    expect(deleted.activeSessionId).toBe(second.session.id)
    expect(deleted.sessions).toHaveLength(1)

    const remaining = await fetchJson(`${baseUrl}/api/sessions`)
    expect(remaining.map((session: { id: string }) => session.id)).toEqual([
      second.session.id,
    ])

    const lastDeleteResponse = await fetch(
      `${baseUrl}/api/sessions/${encodeURIComponent(second.session.id)}`,
      { method: "DELETE" }
    )
    expect(lastDeleteResponse.ok).toBe(false)
  })

  it("allows cross-origin studio API requests", async () => {
    const optionsResponse = await fetch(`${baseUrl}/api/bootstrap`, {
      method: "OPTIONS",
      headers: {
        "access-control-request-headers": "content-type",
        "access-control-request-method": "GET",
        origin: "https://example.invalid",
      },
    })

    expect(optionsResponse.status).toBe(204)
    expect(optionsResponse.headers.get("access-control-allow-origin")).toBe("*")

    const bootstrapResponse = await fetch(`${baseUrl}/api/bootstrap`, {
      headers: { origin: "https://example.invalid" },
    })
    expect(bootstrapResponse.headers.get("access-control-allow-origin")).toBe(
      "*"
    )
  })

  it("does not serve static files outside the configured static directory", async () => {
    await closeServer(server)
    const staticDir = join(projectDir, "static")
    await mkdir(staticDir)
    await writeFile(join(staticDir, "index.html"), "<h1>OpenPanels</h1>")
    await writeFile(join(projectDir, "secret.txt"), "outside-value")
    server = createLocalOpenPanelsServer({ projectDir, staticDir, storageDir })
    await new Promise<void>((resolve) => {
      server.listen(0, "127.0.0.1", resolve)
    })
    const address = server.address()
    if (!address || typeof address === "string") {
      throw new Error("Expected local server address")
    }
    baseUrl = `http://127.0.0.1:${address.port}`

    const indexResponse = await fetch(`${baseUrl}/`)
    expect(indexResponse.status).toBe(200)

    const traversalResponse = await fetch(`${baseUrl}/%2e%2e%2fsecret.txt`)
    expect(traversalResponse.status).not.toBe(200)
    expect(await traversalResponse.text()).not.toContain("outside-value")
  })
})

async function fetchJson(url: string, init?: RequestInit): Promise<any> {
  const response = await fetch(url, init)
  expect(response.ok).toBe(true)
  return response.json()
}

async function closeServer(
  server: ReturnType<typeof createLocalOpenPanelsServer>
): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()))
  })
}

function restoreEnv(name: string, value: string | undefined) {
  if (value === undefined) {
    delete process.env[name]
    return
  }
  process.env[name] = value
}
