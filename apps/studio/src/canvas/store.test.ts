import { describe, expect, it } from "vitest"
import { createCanvasStore } from "./store"
import { createEmptySnapshot } from "./types/records"

describe("canvas camera state", () => {
  it("persists camera state in snapshots", () => {
    const store = createCanvasStore()

    store.getState().setCamera({ x: -120, y: 80, zoom: 1.75 })

    expect(store.getState().getSnapshot().camera).toEqual({
      x: -120,
      y: 80,
      zoom: 1.75,
    })
  })

  it("preserves the current camera when loading an older snapshot", () => {
    const store = createCanvasStore()
    const snapshot = createEmptySnapshot()

    store.getState().setCamera({ x: 40, y: -30, zoom: 0.6 })
    store.getState().loadSnapshot({
      ...snapshot,
      camera: undefined,
    })

    expect(store.getState().getSnapshot().camera).toEqual({
      x: 40,
      y: -30,
      zoom: 0.6,
    })
  })
})
