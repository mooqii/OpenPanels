import { describe, expect, it } from "vitest"
import { InMemoryOpenPanelsStorage, OpenPanelsRuntime } from "./index"

describe("@openpanels/runtime", () => {
  it("creates sessions and panels", async () => {
    const runtime = new OpenPanelsRuntime({
      storage: new InMemoryOpenPanelsStorage(),
    })
    const session = await runtime.createSession({ title: "Demo" })
    const panel = await runtime.openPanel({
      sessionId: session.id,
      kind: "canvas",
    })

    expect(panel.kind).toBe("canvas")
    expect((await runtime.getSession(session.id))?.panelIds).toContain(panel.id)
  })

  it("inserts image artifacts and creates a panel when missing", async () => {
    const runtime = new OpenPanelsRuntime({
      storage: new InMemoryOpenPanelsStorage(),
    })
    const session = await runtime.createSession({ title: "Demo" })
    const artifact = await runtime.insertArtifact({
      sessionId: session.id,
      artifact: {
        kind: "image",
        mimeType: "image/png",
        assetRef: "assets/image.png",
      },
    })

    expect(artifact.panelId).toBeTruthy()
    expect(
      await runtime.listArtifacts(session.id, artifact.panelId)
    ).toHaveLength(1)
  })
})
