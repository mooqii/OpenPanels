import getStroke from "perfect-freehand"
import type { BrushPoint } from "../types/shapes"

/**
 * Raw input point with timestamp for velocity calculation
 */
export interface RawBrushPoint {
  timestamp: number
  x: number
  y: number
}

/**
 * Options for the perfect-freehand stroke
 */
export interface StrokeOptions {
  end: {
    taper: number | boolean
    cap: boolean
  }
  simulatePressure: boolean
  size: number
  smoothing: number
  start: {
    taper: number | boolean
    cap: boolean
  }
  streamline: number
  thinning: number
}

/**
 * Get stroke options based on brush size
 */
export function getStrokeOptions(size: number): StrokeOptions {
  return {
    size,
    thinning: 0.5,
    smoothing: 0.5,
    streamline: 0.5,
    simulatePressure: true,
    start: {
      taper: 0,
      cap: true,
    },
    end: {
      taper: 0,
      cap: true,
    },
  }
}

/**
 * Convert raw input points to brush points
 * The pressure will be simulated by perfect-freehand based on velocity
 */
export function rawPointsToBrushPoints(
  rawPoints: RawBrushPoint[]
): BrushPoint[] {
  return rawPoints.map((p) => ({
    x: p.x,
    y: p.y,
    pressure: 0.5, // Default pressure, perfect-freehand will simulate based on velocity
  }))
}

/**
 * Generate stroke outline using perfect-freehand
 * Returns a flat array of points [x1, y1, x2, y2, ...] forming a closed polygon
 */
export function generateStrokeOutline(
  points: BrushPoint[],
  size: number
): number[] {
  if (points.length === 0) return []

  // Render a circular dot for single-click brush strokes.
  if (points.length === 1) {
    const point = points[0]
    const radius = size / 2
    const segments = 16
    const outline: number[] = []

    for (let index = 0; index < segments; index++) {
      const angle = (Math.PI * 2 * index) / segments
      outline.push(
        point.x + Math.cos(angle) * radius,
        point.y + Math.sin(angle) * radius
      )
    }

    return outline
  }

  // Convert BrushPoint to the format expected by perfect-freehand: [x, y, pressure]
  const inputPoints: [number, number, number][] = points.map((p) => [
    p.x,
    p.y,
    p.pressure,
  ])

  // Get stroke options for this size
  const options = getStrokeOptions(size)

  // Get the stroke outline from perfect-freehand
  const strokePoints = getStroke(inputPoints, options)

  if (strokePoints.length < 3) return []

  // Convert to flat array
  const outline: number[] = []
  for (const point of strokePoints) {
    outline.push(point[0], point[1])
  }

  return outline
}

/**
 * Generate SVG path data from stroke outline
 * This can be used for SVG rendering if needed
 */
export function getSvgPathFromStroke(stroke: number[][]): string {
  if (!stroke.length) return ""

  const d = stroke.reduce(
    (acc, [x0, y0], i, arr) => {
      const [x1, y1] = arr[(i + 1) % arr.length]
      acc.push(x0, y0, (x0 + x1) / 2, (y0 + y1) / 2)
      return acc
    },
    ["M", ...stroke[0], "Q"]
  )

  d.push("Z")
  return d.join(" ")
}

/**
 * Simplify points by removing points that are too close together
 */
export function simplifyPoints(
  points: RawBrushPoint[],
  minDistance = 2
): RawBrushPoint[] {
  if (points.length < 2) return points

  const simplified: RawBrushPoint[] = [points[0]]

  for (let i = 1; i < points.length; i++) {
    const last = simplified.at(-1)!
    const curr = points[i]
    const dx = curr.x - last.x
    const dy = curr.y - last.y
    const distance = Math.sqrt(dx * dx + dy * dy)

    if (distance >= minDistance) {
      simplified.push(curr)
    }
  }

  // Always include the last point if it's different from the last simplified point
  const lastPoint = points.at(-1)!
  const lastSimplified = simplified.at(-1)!
  if (lastPoint !== lastSimplified) {
    const dx = lastPoint.x - lastSimplified.x
    const dy = lastPoint.y - lastSimplified.y
    const distance = Math.sqrt(dx * dx + dy * dy)
    if (distance > 0.5) {
      simplified.push(lastPoint)
    }
  }

  return simplified
}
