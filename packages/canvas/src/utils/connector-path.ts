/**
 * Utilities for calculating connector paths between shapes.
 * Handles bezier curve generation, anchor point calculation, and branching paths.
 */

import type { Bounds, ConnectorAnchor, GeoShape, Shape } from "../types/shapes"

/**
 * A 2D point
 */
export interface Point {
  x: number
  y: number
}

/**
 * A bezier curve segment with start, end, and two control points
 */
export interface BezierSegment {
  cp1: Point
  cp2: Point
  end: Point
  start: Point
}

/**
 * Complete path data for a connector
 */
export interface ConnectorPath {
  /** Computed anchor points on source shapes */
  fromAnchors: Point[]
  /** Bezier segments from sources to hub (or directly to targets if simple) */
  fromSegments: BezierSegment[]
  /** The hub point where paths converge (for branching connectors) */
  hubPoint: Point | null
  /** Computed anchor points on target shapes */
  toAnchors: Point[]
  /** Bezier segments from hub to targets */
  toSegments: BezierSegment[]
}

/**
 * Get the bounding box of a shape
 */
export function getShapeBounds(shape: Shape): Bounds {
  const props = shape.props as Record<string, unknown>
  const x = (props.x as number) ?? 0
  const y = (props.y as number) ?? 0
  const width = (props.width as number) ?? 100
  const height = (props.height as number) ?? 100
  const scaleX = (props.scaleX as number) ?? 1
  const scaleY = (props.scaleY as number) ?? 1

  return {
    x,
    y,
    width: width * scaleX,
    height: height * scaleY,
  }
}

/**
 * Get the center point of a shape
 */
export function getShapeCenter(shape: Shape): Point {
  const bounds = getShapeBounds(shape)

  // Handle ellipse which stores center position
  if (shape.type === "geo" && (shape as GeoShape).props.geo === "ellipse") {
    return { x: bounds.x, y: bounds.y }
  }

  // For other shapes, calculate center from bounds
  return {
    x: bounds.x + bounds.width / 2,
    y: bounds.y + bounds.height / 2,
  }
}

/**
 * Calculate the intersection point of a ray from shape center to target point
 * with the shape's edge. Returns the point on the shape edge closest to the target.
 */
export function calculateEdgeIntersection(
  shape: Shape,
  targetPoint: Point
): Point {
  const bounds = getShapeBounds(shape)
  const center = getShapeCenter(shape)

  // Handle ellipse geometry
  if (shape.type === "geo" && (shape as GeoShape).props.geo === "ellipse") {
    return calculateEllipseIntersection(center, bounds, targetPoint)
  }

  // Default to rectangle intersection
  return calculateRectangleIntersection(bounds, center, targetPoint)
}

/**
 * Calculate intersection with a rectangle edge
 */
function calculateRectangleIntersection(
  bounds: Bounds,
  center: Point,
  target: Point
): Point {
  const dx = target.x - center.x
  const dy = target.y - center.y

  // Avoid division by zero
  if (dx === 0 && dy === 0) {
    return { x: bounds.x + bounds.width / 2, y: bounds.y }
  }

  const halfWidth = bounds.width / 2
  const halfHeight = bounds.height / 2

  // Calculate parametric t for each edge
  let t = Number.POSITIVE_INFINITY

  // Right edge
  if (dx > 0) {
    const tRight = halfWidth / dx
    if (tRight < t) t = tRight
  }
  // Left edge
  if (dx < 0) {
    const tLeft = -halfWidth / dx
    if (tLeft < t) t = tLeft
  }
  // Bottom edge
  if (dy > 0) {
    const tBottom = halfHeight / dy
    if (tBottom < t) t = tBottom
  }
  // Top edge
  if (dy < 0) {
    const tTop = -halfHeight / dy
    if (tTop < t) t = tTop
  }

  return {
    x: center.x + dx * t,
    y: center.y + dy * t,
  }
}

/**
 * Calculate intersection with an ellipse edge
 */
function calculateEllipseIntersection(
  center: Point,
  bounds: Bounds,
  target: Point
): Point {
  const dx = target.x - center.x
  const dy = target.y - center.y

  // Avoid division by zero
  if (dx === 0 && dy === 0) {
    return { x: center.x + bounds.width / 2, y: center.y }
  }

  const rx = bounds.width / 2
  const ry = bounds.height / 2

  // Normalize direction
  const angle = Math.atan2(dy, dx)

  // Point on ellipse at this angle
  return {
    x: center.x + rx * Math.cos(angle),
    y: center.y + ry * Math.sin(angle),
  }
}

/**
 * Calculate the anchor point on a shape given an anchor configuration
 */
export function calculateAnchorPoint(
  shape: Shape,
  anchor: ConnectorAnchor,
  targetPoint: Point
): Point {
  if (anchor === "auto") {
    return calculateEdgeIntersection(shape, targetPoint)
  }

  // Explicit anchor position (normalized 0-1)
  const bounds = getShapeBounds(shape)
  return {
    x: bounds.x + bounds.width * anchor.x,
    y: bounds.y + bounds.height * anchor.y,
  }
}

/**
 * Calculate bezier control points for a smooth curve between two points
 * The control points are positioned to create a natural curve based on
 * the direction and distance between points.
 */
export function calculateBezierControlPoints(
  from: Point,
  to: Point,
  curvature = 0.5
): { cp1: Point; cp2: Point } {
  const dx = to.x - from.x
  const dy = to.y - from.y
  const distance = Math.sqrt(dx * dx + dy * dy)

  // Control point offset based on distance and curvature
  const offset = distance * curvature

  // Determine primary direction to create smooth curves
  const isMoreHorizontal = Math.abs(dx) > Math.abs(dy)

  if (isMoreHorizontal) {
    // Horizontal-dominant: control points extend horizontally
    return {
      cp1: { x: from.x + offset, y: from.y },
      cp2: { x: to.x - offset, y: to.y },
    }
  }
  // Vertical-dominant: control points extend vertically
  return {
    cp1: { x: from.x, y: from.y + (dy > 0 ? offset : -offset) },
    cp2: { x: to.x, y: to.y + (dy > 0 ? -offset : offset) },
  }
}

/**
 * Edge type for rectangular shapes
 */
export type EdgeType = "top" | "right" | "bottom" | "left"

/**
 * Detect which edge of a bounding box a point is closest to.
 * Used to determine the outward direction for bezier control points.
 */
export function detectEdge(bounds: Bounds, point: Point): EdgeType {
  // Calculate distances to each edge
  const distTop = Math.abs(point.y - bounds.y)
  const distBottom = Math.abs(point.y - (bounds.y + bounds.height))
  const distLeft = Math.abs(point.x - bounds.x)
  const distRight = Math.abs(point.x - (bounds.x + bounds.width))

  const minDist = Math.min(distTop, distBottom, distLeft, distRight)

  // Return the edge with minimum distance
  // Priority order for ties: right, left, bottom, top (to match common connector patterns)
  if (minDist === distRight) return "right"
  if (minDist === distLeft) return "left"
  if (minDist === distBottom) return "bottom"
  return "top"
}

/**
 * Get the outward normal direction for a given edge.
 * Returns a unit vector pointing away from the shape.
 */
export function getEdgeOutwardNormal(edge: EdgeType): Point {
  switch (edge) {
    case "top":
      return { x: 0, y: -1 }
    case "right":
      return { x: 1, y: 0 }
    case "bottom":
      return { x: 0, y: 1 }
    case "left":
      return { x: -1, y: 0 }
    default:
      return { x: 1, y: 0 } // Default to right
  }
}

/**
 * Calculate bezier control points with edge-aware direction.
 * Control points extend perpendicular to the connected edge for natural curves.
 *
 * @param from - Start point of the curve
 * @param fromBounds - Bounding box of the source shape (null for hub points)
 * @param to - End point of the curve
 * @param toBounds - Bounding box of the target shape (null for hub points)
 * @param curvature - How much the curve bends (0-1, default 0.5)
 */
export function calculateBezierControlPointsWithEdge(
  from: Point,
  fromBounds: Bounds | null,
  to: Point,
  toBounds: Bounds | null,
  curvature = 0.5
): { cp1: Point; cp2: Point } {
  const dx = to.x - from.x
  const dy = to.y - from.y
  const dist = Math.sqrt(dx * dx + dy * dy)
  const offset = dist * curvature

  // Calculate cp1 direction based on source edge
  let cp1Dir: Point
  if (fromBounds) {
    const fromEdge = detectEdge(fromBounds, from)
    cp1Dir = getEdgeOutwardNormal(fromEdge)
  } else {
    // Fallback for hub points: use direction toward target
    const len = dist || 1
    cp1Dir = { x: dx / len, y: dy / len }
  }

  // Calculate cp2 direction based on target edge (outward from target)
  let cp2Dir: Point
  if (toBounds) {
    const toEdge = detectEdge(toBounds, to)
    cp2Dir = getEdgeOutwardNormal(toEdge)
  } else {
    // Fallback for hub points: use direction from source
    const len = dist || 1
    cp2Dir = { x: -dx / len, y: -dy / len }
  }

  return {
    cp1: { x: from.x + cp1Dir.x * offset, y: from.y + cp1Dir.y * offset },
    cp2: { x: to.x + cp2Dir.x * offset, y: to.y + cp2Dir.y * offset },
  }
}

/**
 * Calculate the centroid (average position) of a set of points
 */
export function calculateCentroid(points: Point[]): Point {
  if (points.length === 0) {
    return { x: 0, y: 0 }
  }

  const sum = points.reduce((acc, p) => ({ x: acc.x + p.x, y: acc.y + p.y }), {
    x: 0,
    y: 0,
  })

  return {
    x: sum.x / points.length,
    y: sum.y / points.length,
  }
}

/**
 * Calculate the hub point for a branching connector.
 * This is the point where multiple source paths converge before
 * diverging to multiple targets.
 */
export function calculateHubPoint(
  fromAnchors: Point[],
  toAnchors: Point[]
): Point {
  const fromCentroid = calculateCentroid(fromAnchors)
  const toCentroid = calculateCentroid(toAnchors)

  // Hub is at the midpoint between centroids
  return {
    x: (fromCentroid.x + toCentroid.x) / 2,
    y: (fromCentroid.y + toCentroid.y) / 2,
  }
}

/**
 * Create a bezier segment between two points.
 * When bounds are provided, control points extend perpendicular to the connected edges.
 *
 * @param from - Start point of the curve
 * @param to - End point of the curve
 * @param curvature - How much the curve bends (0-1, default 0.4)
 * @param fromBounds - Optional bounding box of the source shape for edge-aware direction
 * @param toBounds - Optional bounding box of the target shape for edge-aware direction
 */
export function createBezierSegment(
  from: Point,
  to: Point,
  curvature = 0.4,
  fromBounds?: Bounds | null,
  toBounds?: Bounds | null
): BezierSegment {
  // Use edge-aware calculation when bounds are provided
  const { cp1, cp2 } =
    fromBounds !== undefined || toBounds !== undefined
      ? calculateBezierControlPointsWithEdge(
          from,
          fromBounds ?? null,
          to,
          toBounds ?? null,
          curvature
        )
      : calculateBezierControlPoints(from, to, curvature)

  return {
    start: from,
    cp1,
    cp2,
    end: to,
  }
}

/**
 * Calculate the complete connector path for connecting multiple shapes.
 *
 * For simple 1-to-1 connections, creates a single bezier curve.
 * For branching/merging topologies, creates a hub-based path structure.
 */
export function calculateConnectorPath(
  fromShapes: Shape[],
  toShapes: Shape[],
  fromAnchors: ConnectorAnchor[],
  toAnchors: ConnectorAnchor[]
): ConnectorPath {
  // Calculate target centroids for auto anchor calculation
  const toShapeCenters = toShapes.map(getShapeCenter)
  const fromShapeCenters = fromShapes.map(getShapeCenter)
  const toCentroid = calculateCentroid(toShapeCenters)
  const fromCentroid = calculateCentroid(fromShapeCenters)

  // Get bounds for all shapes (used for edge-aware bezier control points)
  const fromShapeBounds = fromShapes.map(getShapeBounds)
  const toShapeBounds = toShapes.map(getShapeBounds)

  // Calculate anchor points on source shapes
  const fromAnchorPoints = fromShapes.map((shape, i) =>
    calculateAnchorPoint(shape, fromAnchors[i] ?? "auto", toCentroid)
  )

  // Calculate anchor points on target shapes
  const toAnchorPoints = toShapes.map((shape, i) =>
    calculateAnchorPoint(shape, toAnchors[i] ?? "auto", fromCentroid)
  )

  // Simple 1-to-1 connection
  if (fromShapes.length === 1 && toShapes.length === 1) {
    const segment = createBezierSegment(
      fromAnchorPoints[0],
      toAnchorPoints[0],
      0.4,
      fromShapeBounds[0],
      toShapeBounds[0]
    )
    return {
      fromSegments: [segment],
      toSegments: [],
      hubPoint: null,
      fromAnchors: fromAnchorPoints,
      toAnchors: toAnchorPoints,
    }
  }

  // Branching/merging connection with hub
  const hubPoint = calculateHubPoint(fromAnchorPoints, toAnchorPoints)

  // Create segments from sources to hub (with source bounds, no target bounds for hub)
  const fromSegments = fromAnchorPoints.map((anchor, i) =>
    createBezierSegment(anchor, hubPoint, 0.3, fromShapeBounds[i], null)
  )

  // Create segments from hub to targets (no source bounds for hub, with target bounds)
  const toSegments = toAnchorPoints.map((anchor, i) =>
    createBezierSegment(hubPoint, anchor, 0.3, null, toShapeBounds[i])
  )

  return {
    fromSegments,
    toSegments,
    hubPoint,
    fromAnchors: fromAnchorPoints,
    toAnchors: toAnchorPoints,
  }
}

/**
 * Get all anchor points from a connector path (for arrowhead rendering)
 */
export function getPathEndpoints(path: ConnectorPath): {
  starts: Point[]
  ends: Point[]
} {
  return {
    starts: path.fromAnchors,
    ends: path.toAnchors,
  }
}

/**
 * Calculate the tangent angle at a bezier curve endpoint.
 * Used for rotating arrowheads to align with the curve direction.
 */
export function calculateTangentAngle(
  segment: BezierSegment,
  atStart: boolean
): number {
  if (atStart) {
    // Tangent at start: direction from start to cp1
    const dx = segment.cp1.x - segment.start.x
    const dy = segment.cp1.y - segment.start.y
    return Math.atan2(dy, dx)
  }
  // Tangent at end: direction from cp2 to end
  const dx = segment.end.x - segment.cp2.x
  const dy = segment.end.y - segment.cp2.y
  return Math.atan2(dy, dx)
}

/**
 * Calculate distance between two points
 */
export function distance(a: Point, b: Point): number {
  const dx = b.x - a.x
  const dy = b.y - a.y
  return Math.sqrt(dx * dx + dy * dy)
}

/**
 * Get bounding box that encompasses all points in the connector path
 */
export function getConnectorBounds(path: ConnectorPath): Bounds {
  const allPoints: Point[] = [
    ...path.fromAnchors,
    ...path.toAnchors,
    ...path.fromSegments.flatMap((s) => [s.start, s.cp1, s.cp2, s.end]),
    ...path.toSegments.flatMap((s) => [s.start, s.cp1, s.cp2, s.end]),
  ]

  if (path.hubPoint) {
    allPoints.push(path.hubPoint)
  }

  if (allPoints.length === 0) {
    return { x: 0, y: 0, width: 0, height: 0 }
  }

  let minX = allPoints[0].x
  let maxX = allPoints[0].x
  let minY = allPoints[0].y
  let maxY = allPoints[0].y

  for (const p of allPoints) {
    minX = Math.min(minX, p.x)
    maxX = Math.max(maxX, p.x)
    minY = Math.min(minY, p.y)
    maxY = Math.max(maxY, p.y)
  }

  return {
    x: minX,
    y: minY,
    width: maxX - minX,
    height: maxY - minY,
  }
}
