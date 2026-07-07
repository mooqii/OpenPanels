import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { createLocalOpenPanelsServer } from "./index"

describe("@openpanels/local-server", () => {
  let projectDir: string
  let server: ReturnType<typeof createLocalOpenPanelsServer>
  let baseUrl: string

  beforeEach(async () => {
    projectDir = await mkdtemp(join(tmpdir(), "openpanels-server-"))
    server = createLocalOpenPanelsServer({ projectDir })
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
    await rm(projectDir, { recursive: true, force: true })
  })

  it("bootstraps a canvas and persists selection assets", async () => {
    const bootstrap = await fetchJson(`${baseUrl}/api/bootstrap`)
    const tinyPng =
      "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="

    const result = await fetchJson(
      `${baseUrl}/api/panels/${bootstrap.session.id}/${bootstrap.panel.id}/selection`,
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
      `${baseUrl}/api/panels/${bootstrap.session.id}/${bootstrap.panel.id}/assets/__selection/current.png`
    )
    expect(assetResponse.headers.get("content-type")).toBe("image/png")
    expect(await assetResponse.arrayBuffer()).toHaveProperty("byteLength")
  })

  it("preserves selected image asset refs when no selection raster is available", async () => {
    const bootstrap = await fetchJson(`${baseUrl}/api/bootstrap`)
    const tinyPng =
      "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
    const uploaded = await fetchJson(
      `${baseUrl}/api/panels/${bootstrap.session.id}/${bootstrap.panel.id}/assets`,
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
      `${baseUrl}/api/panels/${bootstrap.session.id}/${bootstrap.panel.id}/selection`,
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
    server = createLocalOpenPanelsServer({ projectDir, staticDir })
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
