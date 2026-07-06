import type Konva from "konva"
import type { ShapeFill } from "../types/shapes"
import { calculateLinearGradientPoints, flattenColorStops } from "../utils/fill"

/**
 * Get the appropriate fill style from a Konva shape for use with the native canvas context.
 * Handles solid colors, linear gradients, radial gradients, and patterns.
 */
export function getCanvasFillStyle(
  nativeCtx: CanvasRenderingContext2D,
  konvaShape: Konva.Shape
): string | CanvasGradient | CanvasPattern | null {
  const fillPriority = konvaShape.fillPriority()

  if (fillPriority === "linear-gradient") {
    const start = konvaShape.fillLinearGradientStartPoint()
    const end = konvaShape.fillLinearGradientEndPoint()
    const colorStops = konvaShape.fillLinearGradientColorStops()

    if (start && end && colorStops?.length) {
      const gradient = nativeCtx.createLinearGradient(
        start.x,
        start.y,
        end.x,
        end.y
      )
      for (let i = 0; i < colorStops.length; i += 2) {
        gradient.addColorStop(
          colorStops[i] as number,
          colorStops[i + 1] as string
        )
      }
      return gradient
    }
  }

  if (fillPriority === "radial-gradient") {
    const startPoint = konvaShape.fillRadialGradientStartPoint()
    const endPoint = konvaShape.fillRadialGradientEndPoint()
    const startRadius = konvaShape.fillRadialGradientStartRadius()
    const endRadius = konvaShape.fillRadialGradientEndRadius()
    const colorStops = konvaShape.fillRadialGradientColorStops()

    if (startPoint && endPoint && colorStops?.length) {
      const gradient = nativeCtx.createRadialGradient(
        startPoint.x,
        startPoint.y,
        startRadius,
        endPoint.x,
        endPoint.y,
        endRadius
      )
      for (let i = 0; i < colorStops.length; i += 2) {
        gradient.addColorStop(
          colorStops[i] as number,
          colorStops[i + 1] as string
        )
      }
      return gradient
    }
  }

  if (fillPriority === "pattern") {
    const patternImage = konvaShape.fillPatternImage()
    if (patternImage) {
      const pattern = nativeCtx.createPattern(patternImage, "repeat")
      return pattern
    }
  }

  // Default: solid color fill
  return konvaShape.fill()
}

/**
 * Helper to draw rounded rect path with individual corner radii.
 * cr: [top-left, top-right, bottom-right, bottom-left]
 */
export function drawRoundedRect(
  ctx: Konva.Context,
  x: number,
  y: number,
  w: number,
  h: number,
  cr: [number, number, number, number]
) {
  const maxRadius = Math.min(w / 2, h / 2)
  const tl = Math.min(cr[0], maxRadius) // top-left
  const tr = Math.min(cr[1], maxRadius) // top-right
  const br = Math.min(cr[2], maxRadius) // bottom-right
  const bl = Math.min(cr[3], maxRadius) // bottom-left

  ctx.moveTo(x + tl, y)
  ctx.lineTo(x + w - tr, y)
  ctx.arcTo(x + w, y, x + w, y + tr, tr)
  ctx.lineTo(x + w, y + h - br)
  ctx.arcTo(x + w, y + h, x + w - br, y + h, br)
  ctx.lineTo(x + bl, y + h)
  ctx.arcTo(x, y + h, x, y + h - bl, bl)
  ctx.lineTo(x, y + tl)
  ctx.arcTo(x, y, x + tl, y, tl)
  ctx.closePath()
}

/**
 * Compute Konva fill properties based on fill type.
 * Returns fill props that can be spread onto a Konva Shape component.
 */
export function computeFillProps(
  shapeFill: ShapeFill | undefined,
  width: number,
  height: number,
  patternImage: HTMLImageElement | null
) {
  if (!shapeFill) {
    return { fill: undefined }
  }

  switch (shapeFill.type) {
    case "solid":
      return {
        fill: shapeFill.color,
        fillPriority: "color" as const,
      }

    case "linear-gradient": {
      const { start, end } = calculateLinearGradientPoints(
        width,
        height,
        shapeFill.rotation
      )
      return {
        fillLinearGradientStartPoint: start,
        fillLinearGradientEndPoint: end,
        fillLinearGradientColorStops: flattenColorStops(shapeFill.colorStops),
        fillPriority: "linear-gradient" as const,
      }
    }

    case "radial-gradient": {
      const centerX = width / 2
      const centerY = height / 2
      const radius = Math.max(width, height) / 2
      return {
        fillRadialGradientStartPoint: { x: centerX, y: centerY },
        fillRadialGradientStartRadius: 0,
        fillRadialGradientEndPoint: { x: centerX, y: centerY },
        fillRadialGradientEndRadius: radius,
        fillRadialGradientColorStops: flattenColorStops(shapeFill.colorStops),
        fillPriority: "radial-gradient" as const,
      }
    }

    case "image": {
      if (patternImage) {
        return {
          fillPatternImage: patternImage,
          fillPatternScale: shapeFill.scale ?? { x: 1, y: 1 },
          fillPatternOffset: shapeFill.offset ?? { x: 0, y: 0 },
          fillPriority: "pattern" as const,
        }
      }
      // Fallback while image is loading
      return { fill: "#f0f0f0" }
    }

    default:
      return { fill: undefined }
  }
}
