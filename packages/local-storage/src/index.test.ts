import { access, mkdtemp, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { OpenPanelsRuntime } from "@openpanels/runtime"
import { describe, expect, it } from "vitest"
import { LocalOpenPanelsStorage } from "./index"

describe("@openpanels/local-storage", () => {
  it("persists sessions and panel state under .myopenpanels", async () => {
    const projectDir = await mkdtemp(join(tmpdir(), "openpanels-"))
    try {
      const storage = new LocalOpenPanelsStorage({ projectDir })
      const runtime = new OpenPanelsRuntime({ storage })
      const session = await runtime.createSession({ title: "Local" })
      const panel = await runtime.openPanel({
        sessionId: session.id,
        kind: "canvas",
      })
      await runtime.savePanelState(session.id, panel.id, { store: {} })

      const reloaded = new LocalOpenPanelsStorage({ projectDir })
      expect(await reloaded.readSession(session.id)).toMatchObject({
        title: "Local",
      })
      expect(await reloaded.readPanelState(session.id, panel.id)).toEqual({
        store: {},
      })
      await expect(access(join(projectDir, ".myopenpanels"))).resolves.toBe(
        undefined
      )
      await expect(
        access(join(projectDir, ".openpanels"))
      ).rejects.toMatchObject({ code: "ENOENT" })
    } finally {
      await rm(projectDir, { recursive: true, force: true })
    }
  })

  it("rejects non .myopenpanels roots", async () => {
    const projectDir = await mkdtemp(join(tmpdir(), "openpanels-"))
    try {
      expect(
        () =>
          new LocalOpenPanelsStorage({
            projectDir,
            storageDir: join(projectDir, "openpanels"),
          })
      ).toThrow(/\.myopenpanels/)
    } finally {
      await rm(projectDir, { recursive: true, force: true })
    }
  })

  it("persists panel selection and fixed selection assets", async () => {
    const projectDir = await mkdtemp(join(tmpdir(), "openpanels-"))
    try {
      const storage = new LocalOpenPanelsStorage({ projectDir })
      await storage.writePanelSelection({
        sessionId: "session:1",
        panelId: "panel:1",
        selectedShapeIds: ["shape:1"],
        selectedShapes: [{ id: "shape:1", type: "geo" }],
        assetRef: null,
        updatedAt: new Date().toISOString(),
      })
      const written = await storage.writeAssetFromBuffer({
        sessionId: "session:1",
        panelId: "panel:1",
        buffer: Buffer.from("png"),
        requestedName: "__selection/current.png",
        overwrite: true,
      })

      expect(
        await storage.readPanelSelection("session:1", "panel:1")
      ).toMatchObject({
        selectedShapeIds: ["shape:1"],
      })
      expect(written.assetRef).toContain("__selection/current.png")
      expect(await storage.readAsset(written.assetRef)).toEqual(
        Buffer.from("png")
      )
    } finally {
      await rm(projectDir, { recursive: true, force: true })
    }
  })
})
