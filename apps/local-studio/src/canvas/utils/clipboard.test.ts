import { describe, expect, it } from "vitest"
import { resolveImageShapeGeometry } from "./clipboard"

describe("resolveImageShapeGeometry", () => {
  it("uses intrinsic image dimensions by default", () => {
    expect(
      resolveImageShapeGeometry({
        intrinsicSize: { height: 200, width: 300 },
        position: { x: 10, y: 20 },
      })
    ).toEqual({
      height: 200,
      position: { x: 10, y: 20 },
      width: 300,
    })
  })

  it("keeps a rasterized selection at the original canvas bounds", () => {
    expect(
      resolveImageShapeGeometry({
        displaySize: { height: 120, width: 180 },
        intrinsicSize: { height: 240, width: 360 },
        position: { x: 40, y: 60 },
      })
    ).toEqual({
      height: 120,
      position: { x: 40, y: 60 },
      width: 180,
    })
  })

  it("centers using display dimensions rather than intrinsic pixels", () => {
    expect(
      resolveImageShapeGeometry({
        center: true,
        displaySize: { height: 120, width: 180 },
        intrinsicSize: { height: 240, width: 360 },
        position: { x: 400, y: 300 },
      })
    ).toEqual({
      height: 120,
      position: { x: 310, y: 240 },
      width: 180,
    })
  })
})
