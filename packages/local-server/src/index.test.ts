import { mkdtemp, rm } from "node:fs/promises"
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
    await new Promise<void>((resolve, reject) => {
      server.close((error) => (error ? reject(error) : resolve()))
    })
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
})

async function fetchJson(url: string, init?: RequestInit): Promise<any> {
  const response = await fetch(url, init)
  expect(response.ok).toBe(true)
  return response.json()
}
