/**
 * Preview component for showing the connector being created.
 * Displays visual indicators on selected source and target shapes.
 */

import type Konva from "konva"
import { Circle, Group, Shape as KonvaShape } from "react-konva"
import type { Bounds, ConnectorBinding, Shape } from "../types/shapes"
import {
  calculateAnchorPoint,
  calculateBezierControlPointsWithEdge,
  calculateCentroid,
  calculateEdgeIntersection,
  getShapeBounds,
  getShapeCenter,
} from "../utils/connector-path"

interface ConnectorPreviewProps {
  /** Source shape bindings */
  fromBindings: ConnectorBinding[]
  /** ID of the shape currently being hovered (for preview indicator) */
  hoveredShapeId?: string
  /** Whether the connector tool is active */
  isActive: boolean
  /** Whether we're in drag mode (mousedown -> drag -> mouseup) */
  isDragging: boolean
  /** Current mouse position for preview line */
  mousePosition?: { x: number; y: number }
  /** Current phase of connector creation */
  phase: "from" | "to"
  /** Function to resolve shape by ID */
  resolveShape: (shapeId: string) => Shape | undefined
  /** Current scale for consistent sizing */
  scale?: number
  /** Target shape bindings */
  toBindings: ConnectorBinding[]
}

const PREVIEW_COLOR_FROM = "#3b82f6" // Blue for sources
const PREVIEW_COLOR_TO = "#10b981" // Green for targets
const INDICATOR_SIZE = 8
const LINE_DASH = [8, 4]
const BEZIER_CURVATURE = 0.4

interface BezierPreviewLineProps {
  dash: number[]
  from: { x: number; y: number }
  /** Optional bounds of the source shape for edge-aware curve direction */
  fromBounds?: Bounds | null
  opacity: number
  stroke: string
  strokeWidth: number
  to: { x: number; y: number }
  /** Optional bounds of the target shape for edge-aware curve direction */
  toBounds?: Bounds | null
}

/**
 * Component to render a dashed bezier curve preview line
 */
function BezierPreviewLine({
  from,
  to,
  stroke,
  strokeWidth,
  dash,
  opacity,
  fromBounds,
  toBounds,
}: BezierPreviewLineProps) {
  const { cp1, cp2 } = calculateBezierControlPointsWithEdge(
    from,
    fromBounds ?? null,
    to,
    toBounds ?? null,
    BEZIER_CURVATURE
  )

  const sceneFunc = (ctx: Konva.Context, _shape: Konva.Shape) => {
    const nativeCtx = (ctx as unknown as { _context: CanvasRenderingContext2D })
      ._context

    nativeCtx.beginPath()
    nativeCtx.moveTo(from.x, from.y)
    nativeCtx.bezierCurveTo(cp1.x, cp1.y, cp2.x, cp2.y, to.x, to.y)

    nativeCtx.strokeStyle = stroke
    nativeCtx.lineWidth = strokeWidth
    nativeCtx.lineCap = "round"
    nativeCtx.globalAlpha = opacity
    nativeCtx.setLineDash(dash)
    nativeCtx.stroke()
    nativeCtx.setLineDash([])
    nativeCtx.globalAlpha = 1
  }

  return <KonvaShape sceneFunc={sceneFunc} />
}

export function ConnectorPreview({
  isActive,
  isDragging,
  fromBindings,
  toBindings,
  phase,
  mousePosition,
  resolveShape,
  scale = 1,
  hoveredShapeId,
}: ConnectorPreviewProps) {
  if (!isActive) return null

  // Resolve shapes from bindings
  const fromShapes = fromBindings
    .map((b) => resolveShape(b.shapeId))
    .filter((s): s is Shape => s !== undefined)

  const toShapes = toBindings
    .map((b) => resolveShape(b.shapeId))
    .filter((s): s is Shape => s !== undefined)

  // Get bounds for shapes (used for edge-aware bezier control points)
  const fromShapeBounds = fromShapes.map(getShapeBounds)
  const toShapeBounds = toShapes.map(getShapeBounds)

  // Calculate anchor points
  const targetPoint = mousePosition ?? { x: 0, y: 0 }

  const fromAnchorPoints = fromShapes.map((shape, i) => {
    const anchor = fromBindings[i]?.anchor ?? "auto"
    const toCenter =
      toShapes.length > 0
        ? calculateCentroid(toShapes.map(getShapeCenter))
        : targetPoint
    return calculateAnchorPoint(shape, anchor, toCenter)
  })

  const toAnchorPoints = toShapes.map((shape, i) => {
    const anchor = toBindings[i]?.anchor ?? "auto"
    const fromCenter =
      fromShapes.length > 0
        ? calculateCentroid(fromShapes.map(getShapeCenter))
        : { x: 0, y: 0 }
    return calculateAnchorPoint(shape, anchor, fromCenter)
  })

  // Calculate preview line from sources to mouse/targets
  const fromCentroid =
    fromAnchorPoints.length > 0 ? calculateCentroid(fromAnchorPoints) : null

  const indicatorSize = INDICATOR_SIZE / scale

  // Calculate hover indicator point on shape edge
  const hoveredShape = hoveredShapeId ? resolveShape(hoveredShapeId) : undefined
  const isHoveredShapeAlreadyBound =
    hoveredShapeId &&
    (fromBindings.some((b) => b.shapeId === hoveredShapeId) ||
      toBindings.some((b) => b.shapeId === hoveredShapeId))

  // When in "to" phase (selecting targets), calculate anchor from source centroid
  // When in "from" phase (selecting sources), use mouse position
  const fromShapesCentroid =
    fromShapes.length > 0
      ? calculateCentroid(fromShapes.map(getShapeCenter))
      : null
  const hoverTargetPoint =
    phase === "to" && fromShapesCentroid ? fromShapesCentroid : mousePosition

  const hoverAnchorPoint =
    hoveredShape && hoverTargetPoint && !isHoveredShapeAlreadyBound
      ? calculateEdgeIntersection(hoveredShape, hoverTargetPoint)
      : null

  // Use green color for target hover indicator, blue for source
  const hoverIndicatorColor =
    phase === "to" ? PREVIEW_COLOR_TO : PREVIEW_COLOR_FROM

  return (
    <Group listening={false}>
      {/* Source shape indicators */}
      {fromAnchorPoints.map((point, i) => (
        <Circle
          fill={PREVIEW_COLOR_FROM}
          key={`from-${fromBindings[i].shapeId}`}
          opacity={0.8}
          radius={indicatorSize}
          stroke="white"
          strokeWidth={2 / scale}
          x={point.x}
          y={point.y}
        />
      ))}

      {/* Target shape indicators */}
      {toAnchorPoints.map((point, i) => (
        <Circle
          fill={PREVIEW_COLOR_TO}
          key={`to-${toBindings[i].shapeId}`}
          opacity={0.8}
          radius={indicatorSize}
          stroke="white"
          strokeWidth={2 / scale}
          x={point.x}
          y={point.y}
        />
      ))}

      {/* Preview bezier curve from source centroid to mouse position */}
      {/* Show during drag mode, or in 'to' phase with no targets yet */}
      {fromCentroid &&
        mousePosition &&
        (isDragging || (phase === "to" && toShapes.length === 0)) && (
          <BezierPreviewLine
            dash={LINE_DASH}
            from={fromCentroid}
            fromBounds={
              fromShapeBounds.length === 1 ? fromShapeBounds[0] : null
            }
            opacity={0.6}
            stroke={PREVIEW_COLOR_TO}
            strokeWidth={2 / scale}
            to={mousePosition}
            toBounds={null}
          />
        )}

      {/* Preview bezier curves to target anchors */}
      {fromCentroid &&
        toAnchorPoints.map((point, i) => (
          <BezierPreviewLine
            dash={LINE_DASH}
            from={fromCentroid}
            fromBounds={
              fromShapeBounds.length === 1 ? fromShapeBounds[0] : null
            }
            key={`line-to-${toBindings[i].shapeId}`}
            opacity={0.6}
            stroke={PREVIEW_COLOR_TO}
            strokeWidth={2 / scale}
            to={point}
            toBounds={toShapeBounds[i]}
          />
        ))}

      {/* Phase indicator text (optional visual feedback) */}
      {fromBindings.length > 0 && (
        <Circle
          fill={phase === "from" ? PREVIEW_COLOR_FROM : PREVIEW_COLOR_TO}
          opacity={0.3}
          radius={indicatorSize * 2}
          x={fromCentroid?.x ?? 0}
          y={fromCentroid?.y ?? 0}
        />
      )}

      {/* Hover indicator - shows anchor point on hovered shape edge */}
      {hoverAnchorPoint && (
        <>
          {/* Outer glow circle */}
          <Circle
            fill={hoverIndicatorColor}
            opacity={0.2}
            radius={indicatorSize * 2}
            x={hoverAnchorPoint.x}
            y={hoverAnchorPoint.y}
          />
          {/* Inner indicator circle */}
          <Circle
            fill={hoverIndicatorColor}
            opacity={0.8}
            radius={indicatorSize}
            stroke="white"
            strokeWidth={2 / scale}
            x={hoverAnchorPoint.x}
            y={hoverAnchorPoint.y}
          />
        </>
      )}
    </Group>
  )
}
