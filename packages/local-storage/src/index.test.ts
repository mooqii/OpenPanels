import { access, mkdtemp, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { DatabaseSync } from "node:sqlite"
import { OpenPanelsRuntime } from "@openpanels/runtime"
import { describe, expect, it } from "vitest"
import { LocalOpenPanelsStorage } from "./index"

describe("@openpanels/local-storage", () => {
  it("persists sessions and panel state in SQLite under .myopenpanels", async () => {
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
        access(join(projectDir, ".myopenpanels", "myopenpanels.sqlite3"))
      ).resolves.toBe(undefined)
      await expect(
        access(
          join(
            projectDir,
            ".myopenpanels",
            "sessions",
            session.id,
            "session.json"
          )
        )
      ).rejects.toMatchObject({ code: "ENOENT" })
      await expect(
        access(
          join(
            projectDir,
            ".myopenpanels",
            "sessions",
            session.id,
            "panels",
            panel.id,
            "state.json"
          )
        )
      ).rejects.toMatchObject({ code: "ENOENT" })
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

  it("allows .myopenpanels roots outside the project directory", async () => {
    const projectDir = await mkdtemp(join(tmpdir(), "openpanels-project-"))
    const storageParent = await mkdtemp(join(tmpdir(), "openpanels-global-"))
    try {
      const storageDir = join(storageParent, ".myopenpanels")
      const storage = new LocalOpenPanelsStorage({ projectDir, storageDir })
      const runtime = new OpenPanelsRuntime({ storage })
      const session = await runtime.createSession({ title: "Shared" })

      expect(await storage.readSession(session.id)).toMatchObject({
        title: "Shared",
      })
      await expect(access(storageDir)).resolves.toBe(undefined)
    } finally {
      await rm(projectDir, { recursive: true, force: true })
      await rm(storageParent, { recursive: true, force: true })
    }
  })

  it("persists panel selection and fixed selection assets", async () => {
    const projectDir = await mkdtemp(join(tmpdir(), "openpanels-"))
    try {
      const storage = new LocalOpenPanelsStorage({ projectDir })
      const runtime = new OpenPanelsRuntime({ storage })
      const session = await runtime.createSession({ title: "Selection" })
      const panel = await runtime.openPanel({
        sessionId: session.id,
        kind: "canvas",
      })
      await storage.writePanelSelection({
        sessionId: session.id,
        panelId: panel.id,
        selectedShapeIds: ["shape:1"],
        selectedShapes: [{ id: "shape:1", type: "geo" }],
        assetRef: null,
        updatedAt: new Date().toISOString(),
      })
      const written = await storage.writeAssetFromBuffer({
        sessionId: session.id,
        panelId: panel.id,
        buffer: Buffer.from("png"),
        requestedName: "__selection/current.png",
        overwrite: true,
      })

      expect(
        await storage.readPanelSelection(session.id, panel.id)
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

  it("applies migrations and indexes wiki tasks from panel state", async () => {
    const projectDir = await mkdtemp(join(tmpdir(), "openpanels-"))
    try {
      const storage = new LocalOpenPanelsStorage({ projectDir })
      const runtime = new OpenPanelsRuntime({ storage })
      const session = await runtime.createSession({ title: "Wiki" })
      const panel = await runtime.openPanel({
        sessionId: session.id,
        kind: "wiki",
      })
      await runtime.savePanelState(session.id, panel.id, {
        schemaVersion: 2,
        tasks: [
          {
            id: "task:1",
            type: "rebuild_wiki_index",
            status: "queued",
            targetId: "index.md",
            documentId: null,
            wikiSpaceId: "wiki:default",
            markdownVersion: null,
            claimedByProcessId: null,
            createdAt: "2026-07-08T00:00:00.000Z",
            updatedAt: "2026-07-08T00:00:01.000Z",
          },
        ],
      })
      storage.close()

      const db = new DatabaseSync(storage.databasePath)
      try {
        const migration = db
          .prepare("SELECT id FROM schema_migrations WHERE id = ?")
          .get("0001_initial")
        expect(migration).toMatchObject({ id: "0001_initial" })
        const task = db
          .prepare("SELECT id, status FROM wiki_tasks WHERE id = ?")
          .get("task:1")
        expect(task).toMatchObject({ id: "task:1", status: "queued" })
      } finally {
        db.close()
      }
      await expect(
        access(
          join(
            projectDir,
            ".myopenpanels",
            "sessions",
            session.id,
            "panels",
            panel.id,
            "tasks",
            "task:1.json"
          )
        )
      ).rejects.toMatchObject({ code: "ENOENT" })
    } finally {
      await rm(projectDir, { recursive: true, force: true })
    }
  })
})
