/**
 * Utility functions for drawing connectors on canvas.
 * Shared between React rendering and imperative updates.
 */

import type { ConnectorShape } from "../types/shapes"
import {
  type BezierSegment,
  type ConnectorPath,
  calculateTangentAngle,
} from "./connector-path"

/**
 * Draw a bezier segment on the canvas
 */
export function drawBezierSegment(
  ctx: CanvasRenderingContext2D,
  segment: BezierSegment
): void {
  ctx.moveTo(segment.start.x, segment.start.y)
  ctx.bezierCurveTo(
    segment.cp1.x,
    segment.cp1.y,
    segment.cp2.x,
    segment.cp2.y,
    segment.end.x,
    segment.end.y
  )
}

/**
 * Get dash pattern for a line style
 */
export function getDashPattern(lineStyle: string): number[] {
  switch (lineStyle) {
    case "dashed":
      return [10, 5]
    case "dotted":
      return [3, 3]
    default:
      return []
  }
}

/**
 * Draw an arrowhead at a given position and angle
 */
export function drawArrowhead(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  angle: number,
  style: string,
  size: number,
  color: string
): void {
  if (style === "none") return

  ctx.save()
  ctx.translate(x, y)
  ctx.rotate(angle)
  ctx.fillStyle = color

  switch (style) {
    case "arrow": {
      // V-shaped arrow
      ctx.beginPath()
      ctx.moveTo(0, 0)
      ctx.lineTo(-size, -size * 0.6)
      ctx.lineTo(-size * 0.7, 0)
      ctx.lineTo(-size, size * 0.6)
      ctx.closePath()
      ctx.fill()
      break
    }
    case "triangle": {
      // Filled triangle
      ctx.beginPath()
      ctx.moveTo(0, 0)
      ctx.lineTo(-size, -size * 0.7)
      ctx.lineTo(-size, size * 0.7)
      ctx.closePath()
      ctx.fill()
      break
    }
    case "circle": {
      // Filled circle
      ctx.beginPath()
      ctx.arc(-size * 0.5, 0, size * 0.5, 0, Math.PI * 2)
      ctx.fill()
      break
    }
    default:
      // "none" - do nothing
      break
  }

  ctx.restore()
}

/**
 * Draw a connector path on a canvas context.
 * This is used for both React rendering and imperative updates.
 */
export function drawConnectorPath(
  ctx: CanvasRenderingContext2D,
  connectorPath: ConnectorPath,
  props: ConnectorShape["props"]
): void {
  const stroke = props.stroke || "#666666"
  const strokeWidth = props.strokeWidth || 2
  const lineStyle = props.lineStyle || "solid"
  const arrowStart = props.arrowStart || "none"
  const arrowEnd = props.arrowEnd || "arrow"
  const arrowSize = props.arrowSize ?? strokeWidth * 3

  // Set line styles
  ctx.strokeStyle = stroke
  ctx.lineWidth = strokeWidth
  ctx.lineCap = "round"
  ctx.lineJoin = "round"
  ctx.setLineDash(getDashPattern(lineStyle))

  // Draw all bezier segments
  ctx.beginPath()

  // Draw from-segments (source to hub)
  for (const segment of connectorPath.fromSegments) {
    drawBezierSegment(ctx, segment)
  }

  // Draw to-segments (hub to target)
  for (const segment of connectorPath.toSegments) {
    drawBezierSegment(ctx, segment)
  }

  ctx.stroke()

  // Reset dash for arrowheads
  ctx.setLineDash([])

  // Draw arrowheads at start points (source anchors)
  if (arrowStart !== "none") {
    for (const segment of connectorPath.fromSegments) {
      const angle = calculateTangentAngle(segment, true)
      drawArrowhead(
        ctx,
        segment.start.x,
        segment.start.y,
        angle + Math.PI, // Point away from the line
        arrowStart,
        arrowSize,
        stroke
      )
    }
  }

  // Draw arrowheads at end points (target anchors)
  if (arrowEnd !== "none") {
    // For simple connectors (no hub), use fromSegments
    const endSegments =
      connectorPath.toSegments.length > 0
        ? connectorPath.toSegments
        : connectorPath.fromSegments

    for (const segment of endSegments) {
      const angle = calculateTangentAngle(segment, false)
      drawArrowhead(
        ctx,
        segment.end.x,
        segment.end.y,
        angle,
        arrowEnd,
        arrowSize,
        stroke
      )
    }
  }

  // Draw hub point indicator (optional, for debugging/visualization)
  if (connectorPath.hubPoint) {
    ctx.fillStyle = stroke
    ctx.beginPath()
    ctx.arc(
      connectorPath.hubPoint.x,
      connectorPath.hubPoint.y,
      strokeWidth * 1.5,
      0,
      Math.PI * 2
    )
    ctx.fill()
  }
}
