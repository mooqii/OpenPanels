/**
 * Geometry-aware hit testing for shape selection.
 * Provides accurate intersection detection for all shape types,
 * handling rotation, ellipses, lines, and complex paths.
 */

import type { Editor } from "../editor"
import type {
  Bounds,
  BrushShape,
  ConnectorShape,
  DrawShape,
  GeoShape,
  GeoShapeProps,
  GroupShape,
  ImageShape,
  MarkerShape,
  PathShape,
  Shape,
  TextShape,
} from "../types/shapes"
import { type ConnectorPath, calculateConnectorPath } from "./connector-path"
import {
  getRectEdges,
  isPolygonFullyContained,
  pointInRect,
  polygonsIntersect,
  rectToPolygon,
  segmentsIntersect,
} from "./polygon-geometry"

// =============================================================================
// Types
// =============================================================================

export interface Point {
  x: number
  y: number
}

export interface HitTestResult {
  /** True if the shape is fully contained within the selection rectangle */
  fullyContained: boolean
  /** True if the shape intersects with the selection rectangle */
  intersects: boolean
}

// =============================================================================
// Main Entry Point
// =============================================================================

/**
 * Tests if a shape intersects with a selection rectangle.
 * Uses geometry-aware algorithms for accurate hit detection.
 * @param editor Optional editor instance required for connector shape hit testing
 */
export function testShapeIntersection(
  shape: Shape,
  selectionRect: Bounds,
  editor?: Editor
): HitTestResult {
  switch (shape.type) {
    case "geo":
      return testGeoShapeIntersection(shape, selectionRect)
    case "text":
    case "image":
      return testRectangularShapeIntersection(shape, selectionRect)
    case "draw":
      return testDrawShapeIntersection(shape, selectionRect)
    case "path":
      return testPathShapeIntersection(shape, selectionRect)
    case "brush":
      return testBrushShapeIntersection(shape, selectionRect)
    case "marker":
      return testMarkerShapeIntersection(shape, selectionRect)
    case "connector":
      return testConnectorShapeIntersection(shape, selectionRect, editor)
    case "group":
      return testGroupShapeIntersection(shape, selectionRect, editor)
    case "placeholder": {
      const props = shape.props
      return testRotatedRectangleIntersection(
        props.x ?? 0,
        props.y ?? 0,
        props.width ?? 0,
        props.height ?? 0,
        props.rotation ?? 0,
        props.scaleX ?? 1,
        props.scaleY ?? 1,
        selectionRect
      )
    }
    default: {
      // Fallback to simple bounds check
      return { intersects: false, fullyContained: false }
    }
  }
}

// =============================================================================
// Geo Shape Hit Testing (Rectangle, Ellipse, Line, etc.)
// =============================================================================

function testGeoShapeIntersection(
  shape: GeoShape,
  selectionRect: Bounds
): HitTestResult {
  const props = shape.props

  switch (props.geo) {
    case "rectangle":
      return testRotatedRectangleIntersection(
        props.x ?? 0,
        props.y ?? 0,
        props.width ?? 0,
        props.height ?? 0,
        props.rotation ?? 0,
        props.scaleX ?? 1,
        props.scaleY ?? 1,
        selectionRect
      )

    case "ellipse":
      return testEllipseIntersection(
        props.x ?? 0,
        props.y ?? 0,
        props.width ?? 0,
        props.height ?? 0,
        props.rotation ?? 0,
        props.scaleX ?? 1,
        props.scaleY ?? 1,
        selectionRect
      )

    case "line":
      return testLineIntersection(
        props.x ?? 0,
        props.y ?? 0,
        props.width ?? 0,
        props.height ?? 0,
        props.rotation ?? 0,
        props.scaleX ?? 1,
        props.scaleY ?? 1,
        (props.strokeWidth as number) ?? 1,
        selectionRect
      )

    default: {
      const propsNever = props as GeoShapeProps
      // Fallback for unknown geo types - treat as rectangle
      return testRotatedRectangleIntersection(
        propsNever.x ?? 0,
        propsNever.y ?? 0,
        propsNever.width ?? 0,
        propsNever.height ?? 0,
        propsNever.rotation ?? 0,
        propsNever.scaleX ?? 1,
        propsNever.scaleY ?? 1,
        selectionRect
      )
    }
  }
}

// =============================================================================
// Rotated Rectangle Intersection
// =============================================================================

/**
 * Tests if a rotated rectangle intersects with a selection rectangle.
 * Converts the rectangle to a polygon and uses polygon intersection.
 */
function testRotatedRectangleIntersection(
  x: number,
  y: number,
  width: number,
  height: number,
  rotation: number,
  scaleX: number,
  scaleY: number,
  selectionRect: Bounds
): HitTestResult {
  const scaledWidth = width * scaleX
  const scaledHeight = height * scaleY

  // Get the rotated vertices
  const vertices = getRotatedRectVertices(
    x,
    y,
    scaledWidth,
    scaledHeight,
    rotation
  )

  // Convert selection rect to polygon
  const selectionPolygon = rectToPolygon(selectionRect)

  const intersects = polygonsIntersect(vertices, selectionPolygon)
  const fullyContained = isPolygonFullyContained(vertices, selectionRect)

  return { intersects, fullyContained }
}

/**
 * Gets the 4 corner vertices of a rotated rectangle.
 *
 * IMPORTANT: Konva rotates shapes around their origin point (x, y), which is
 * the top-left corner for rectangles. This is different from rotating around
 * the center!
 */
export function getRotatedRectVertices(
  x: number,
  y: number,
  width: number,
  height: number,
  rotation: number
): Point[] {
  // If no rotation, return simple corners
  if (rotation === 0) {
    return [
      { x, y },
      { x: x + width, y },
      { x: x + width, y: y + height },
      { x, y: y + height },
    ]
  }

  const rad = (rotation * Math.PI) / 180
  const cos = Math.cos(rad)
  const sin = Math.sin(rad)

  // Local corners relative to origin (top-left corner at 0,0)
  // Konva rotates around the origin point (x, y)
  const localCorners = [
    { x: 0, y: 0 }, // top-left (origin)
    { x: width, y: 0 }, // top-right
    { x: width, y: height }, // bottom-right
    { x: 0, y: height }, // bottom-left
  ]

  // Transform each corner: rotate around origin, then translate to (x, y)
  return localCorners.map((c) => ({
    x: x + c.x * cos - c.y * sin,
    y: y + c.x * sin + c.y * cos,
  }))
}

// =============================================================================
// Ellipse Intersection
// =============================================================================

/**
 * Tests if a (rotated) ellipse intersects with a selection rectangle.
 * Uses point sampling around the ellipse perimeter plus center point checks.
 */
function testEllipseIntersection(
  cx: number,
  cy: number,
  width: number,
  height: number,
  rotation: number,
  scaleX: number,
  scaleY: number,
  selectionRect: Bounds
): HitTestResult {
  const scaledWidth = width * scaleX
  const scaledHeight = height * scaleY
  const rx = scaledWidth / 2
  const ry = scaledHeight / 2

  // Sample points around the ellipse
  const sampleCount = 32 // More points = more accurate
  const points = sampleEllipsePoints(cx, cy, rx, ry, rotation, sampleCount)

  // Check if any ellipse point is inside the selection rect
  const anyPointInRect = points.some((p) => pointInRect(p, selectionRect))

  // Check if any selection rect corner is inside the ellipse
  const selectionCorners = rectToPolygon(selectionRect)
  const anyCornerInEllipse = selectionCorners.some((corner) =>
    pointInEllipse(corner, cx, cy, rx, ry, rotation)
  )

  // Check if ellipse center is in selection rect (handles case where ellipse is larger than rect)
  const centerInRect = pointInRect({ x: cx, y: cy }, selectionRect)

  const intersects = anyPointInRect || anyCornerInEllipse || centerInRect

  // For full containment, all sampled points must be in the rect
  const fullyContained = points.every((p) => pointInRect(p, selectionRect))

  return { intersects, fullyContained }
}

/**
 * Samples points around an ellipse perimeter.
 */
function sampleEllipsePoints(
  cx: number,
  cy: number,
  rx: number,
  ry: number,
  rotation: number,
  count: number
): Point[] {
  const points: Point[] = []
  const rotRad = (rotation * Math.PI) / 180
  const cosRot = Math.cos(rotRad)
  const sinRot = Math.sin(rotRad)

  for (let i = 0; i < count; i++) {
    const angle = (i / count) * Math.PI * 2
    // Point on unrotated ellipse
    const ex = rx * Math.cos(angle)
    const ey = ry * Math.sin(angle)

    // Rotate and translate to world space
    points.push({
      x: cx + ex * cosRot - ey * sinRot,
      y: cy + ex * sinRot + ey * cosRot,
    })
  }

  return points
}

/**
 * Tests if a point is inside a (rotated) ellipse.
 */
function pointInEllipse(
  point: Point,
  cx: number,
  cy: number,
  rx: number,
  ry: number,
  rotation: number
): boolean {
  // Transform point to ellipse's local coordinate system
  const dx = point.x - cx
  const dy = point.y - cy

  if (rotation !== 0) {
    const rad = (-rotation * Math.PI) / 180 // Inverse rotation
    const cos = Math.cos(rad)
    const sin = Math.sin(rad)
    const localX = dx * cos - dy * sin
    const localY = dx * sin + dy * cos
    return (localX * localX) / (rx * rx) + (localY * localY) / (ry * ry) <= 1
  }

  return (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry) <= 1
}

// =============================================================================
// Line Intersection
// =============================================================================

/**
 * Tests if a line segment intersects with a selection rectangle.
 * Uses a tolerance/thickness for the line based on stroke width.
 */
function testLineIntersection(
  x: number,
  y: number,
  width: number,
  height: number,
  rotation: number,
  scaleX: number,
  scaleY: number,
  strokeWidth: number,
  selectionRect: Bounds
): HitTestResult {
  // Line endpoints
  const scaledWidth = width * scaleX
  const scaledHeight = height * scaleY

  // Start at (x, y), end at (x + width, y + height)
  const x1 = x
  const y1 = y
  let x2 = x + scaledWidth
  let y2 = y + scaledHeight

  // Apply rotation around start point
  if (rotation !== 0) {
    const rad = (rotation * Math.PI) / 180
    const cos = Math.cos(rad)
    const sin = Math.sin(rad)
    const dx = scaledWidth
    const dy = scaledHeight
    x2 = x + dx * cos - dy * sin
    y2 = y + dx * sin + dy * cos
  }

  // Check if line segment intersects with rectangle
  const intersects = lineSegmentIntersectsRect(
    { x: x1, y: y1 },
    { x: x2, y: y2 },
    selectionRect,
    Math.max(strokeWidth, 2) // Minimum hit tolerance
  )

  // For full containment, both endpoints must be in rect
  const fullyContained =
    pointInRect({ x: x1, y: y1 }, selectionRect) &&
    pointInRect({ x: x2, y: y2 }, selectionRect)

  return { intersects, fullyContained }
}

/**
 * Tests if a line segment intersects with a rectangle.
 * Uses a tolerance for the line thickness.
 */
function lineSegmentIntersectsRect(
  p1: Point,
  p2: Point,
  rect: Bounds,
  tolerance: number
): boolean {
  // Check if either endpoint is in the rect (expanded by tolerance)
  const expandedRect: Bounds = {
    x: rect.x - tolerance / 2,
    y: rect.y - tolerance / 2,
    width: rect.width + tolerance,
    height: rect.height + tolerance,
  }

  if (pointInRect(p1, expandedRect) || pointInRect(p2, expandedRect)) {
    return true
  }

  // Check if line crosses any rect edge
  const rectEdges = getRectEdges(rect)
  for (const edge of rectEdges) {
    if (segmentsIntersect(p1, p2, edge[0], edge[1])) {
      return true
    }
  }

  return false
}

// =============================================================================
// Text/Image Shape Hit Testing (Rectangular)
// =============================================================================

function testRectangularShapeIntersection(
  shape: TextShape | ImageShape,
  selectionRect: Bounds
): HitTestResult {
  const props = shape.props
  const x = props.x ?? 0
  const y = props.y ?? 0
  const width = (props.width ?? 0) * (props.scaleX ?? 1)
  const height = (props.height ?? 0) * (props.scaleY ?? 1)
  const rotation = props.rotation ?? 0

  return testRotatedRectangleIntersection(
    x,
    y,
    width,
    height,
    rotation,
    1,
    1,
    selectionRect
  )
}

// =============================================================================
// Draw Shape Hit Testing (Pencil Lines)
// =============================================================================

function testDrawShapeIntersection(
  shape: DrawShape,
  selectionRect: Bounds
): HitTestResult {
  const props = shape.props
  const points = props.points ?? []
  const offsetX = props.x ?? 0
  const offsetY = props.y ?? 0
  const scaleX = props.scaleX ?? 1
  const scaleY = props.scaleY ?? 1

  if (points.length < 2) {
    return { intersects: false, fullyContained: false }
  }

  // Convert flat points array to Point objects with transform applied
  const worldPoints: Point[] = []
  for (let i = 0; i < points.length; i += 2) {
    worldPoints.push({
      x: offsetX + points[i] * scaleX,
      y: offsetY + points[i + 1] * scaleY,
    })
  }

  return testPointsIntersection(worldPoints, selectionRect, props.size ?? 4)
}

// =============================================================================
// Marker Shape Hit Testing (Highlighter-style strokes)
// =============================================================================

function testMarkerShapeIntersection(
  shape: MarkerShape,
  selectionRect: Bounds
): HitTestResult {
  const props = shape.props
  const points = props.points ?? []
  const offsetX = props.x ?? 0
  const offsetY = props.y ?? 0
  const scaleX = props.scaleX ?? 1
  const scaleY = props.scaleY ?? 1

  if (points.length < 2) {
    return { intersects: false, fullyContained: false }
  }

  // Convert flat points array to Point objects with transform applied
  const worldPoints: Point[] = []
  for (let i = 0; i < points.length; i += 2) {
    worldPoints.push({
      x: offsetX + points[i] * scaleX,
      y: offsetY + points[i + 1] * scaleY,
    })
  }

  return testPointsIntersection(worldPoints, selectionRect, props.size ?? 4)
}

// =============================================================================
// Path Shape Hit Testing (Bezier Paths)
// =============================================================================

function testPathShapeIntersection(
  shape: PathShape,
  selectionRect: Bounds
): HitTestResult {
  const props = shape.props
  const pathPoints = props.points ?? []
  const offsetX = props.x ?? 0
  const offsetY = props.y ?? 0
  const scaleX = props.scaleX ?? 1
  const scaleY = props.scaleY ?? 1

  if (pathPoints.length < 1) {
    return { intersects: false, fullyContained: false }
  }

  // Sample points along the bezier path
  const sampledPoints: Point[] = []

  for (let i = 0; i < pathPoints.length; i++) {
    const curr = pathPoints[i]
    // Add anchor point
    sampledPoints.push({
      x: offsetX + curr.x * scaleX,
      y: offsetY + curr.y * scaleY,
    })

    // Sample bezier curve to next point
    if (i < pathPoints.length - 1) {
      const next = pathPoints[i + 1]
      const bezierSamples = sampleBezierCurve(
        { x: curr.x, y: curr.y },
        curr.handleOut
          ? { x: curr.x + curr.handleOut.x, y: curr.y + curr.handleOut.y }
          : { x: curr.x, y: curr.y },
        next.handleIn
          ? { x: next.x + next.handleIn.x, y: next.y + next.handleIn.y }
          : { x: next.x, y: next.y },
        { x: next.x, y: next.y },
        8 // Sample count per segment
      )

      for (const p of bezierSamples) {
        sampledPoints.push({
          x: offsetX + p.x * scaleX,
          y: offsetY + p.y * scaleY,
        })
      }
    }
  }

  // Handle closed paths
  if (props.closed && pathPoints.length > 2) {
    const first = pathPoints[0]
    const last = pathPoints.at(-1)!
    const bezierSamples = sampleBezierCurve(
      { x: last.x, y: last.y },
      last.handleOut
        ? { x: last.x + last.handleOut.x, y: last.y + last.handleOut.y }
        : { x: last.x, y: last.y },
      first.handleIn
        ? { x: first.x + first.handleIn.x, y: first.y + first.handleIn.y }
        : { x: first.x, y: first.y },
      { x: first.x, y: first.y },
      8
    )

    for (const p of bezierSamples) {
      sampledPoints.push({
        x: offsetX + p.x * scaleX,
        y: offsetY + p.y * scaleY,
      })
    }
  }

  return testPointsIntersection(
    sampledPoints,
    selectionRect,
    props.strokeWidth ?? 2
  )
}

/**
 * Samples points along a cubic bezier curve.
 */
function sampleBezierCurve(
  p0: Point,
  p1: Point,
  p2: Point,
  p3: Point,
  count: number
): Point[] {
  const points: Point[] = []

  for (let i = 1; i <= count; i++) {
    const t = i / count
    const t2 = t * t
    const t3 = t2 * t
    const mt = 1 - t
    const mt2 = mt * mt
    const mt3 = mt2 * mt

    points.push({
      x: mt3 * p0.x + 3 * mt2 * t * p1.x + 3 * mt * t2 * p2.x + t3 * p3.x,
      y: mt3 * p0.y + 3 * mt2 * t * p1.y + 3 * mt * t2 * p2.y + t3 * p3.y,
    })
  }

  return points
}

// =============================================================================
// Brush Shape Hit Testing
// =============================================================================

function testBrushShapeIntersection(
  shape: BrushShape,
  selectionRect: Bounds
): HitTestResult {
  const props = shape.props
  const brushPoints = props.points ?? []
  const offsetX = props.x ?? 0
  const offsetY = props.y ?? 0
  const scaleX = props.scaleX ?? 1
  const scaleY = props.scaleY ?? 1

  if (brushPoints.length < 1) {
    return { intersects: false, fullyContained: false }
  }

  // Convert brush points to world coordinates
  const worldPoints: Point[] = brushPoints.map((p) => ({
    x: offsetX + p.x * scaleX,
    y: offsetY + p.y * scaleY,
  }))

  return testPointsIntersection(worldPoints, selectionRect, props.size ?? 4)
}

// =============================================================================
// Connector Shape Hit Testing
// =============================================================================

/**
 * Tests if a connector shape intersects with a selection rectangle.
 * Requires editor to resolve bound shapes for path calculation.
 */
function testConnectorShapeIntersection(
  shape: ConnectorShape,
  selectionRect: Bounds,
  editor?: Editor
): HitTestResult {
  const props = shape.props

  // If no editor provided, cannot resolve bound shapes - return no intersection
  if (!editor) {
    return { intersects: false, fullyContained: false }
  }

  // Resolve bound shapes
  const fromShapes = props.fromBindings
    .map((binding) => editor.getShape(binding.shapeId))
    .filter((s): s is Shape => s !== undefined)

  const toShapes = props.toBindings
    .map((binding) => editor.getShape(binding.shapeId))
    .filter((s): s is Shape => s !== undefined)

  // If we can't resolve all bound shapes, return no intersection
  if (fromShapes.length === 0 || toShapes.length === 0) {
    return { intersects: false, fullyContained: false }
  }

  // Extract anchor configurations
  const fromAnchors = props.fromBindings.map((b) => b.anchor)
  const toAnchors = props.toBindings.map((b) => b.anchor)

  // Calculate connector path
  let connectorPath: ConnectorPath
  try {
    connectorPath = calculateConnectorPath(
      fromShapes,
      toShapes,
      fromAnchors,
      toAnchors
    )
  } catch {
    // If path calculation fails, return no intersection
    return { intersects: false, fullyContained: false }
  }

  // Sample points along all bezier segments
  const sampledPoints: Point[] = []

  // Sample points from fromSegments
  for (const segment of connectorPath.fromSegments) {
    sampledPoints.push(segment.start)
    const bezierSamples = sampleBezierCurve(
      segment.start,
      segment.cp1,
      segment.cp2,
      segment.end,
      8 // Sample count per segment
    )
    sampledPoints.push(...bezierSamples)
    sampledPoints.push(segment.end)
  }

  // Sample points from toSegments
  for (const segment of connectorPath.toSegments) {
    sampledPoints.push(segment.start)
    const bezierSamples = sampleBezierCurve(
      segment.start,
      segment.cp1,
      segment.cp2,
      segment.end,
      8 // Sample count per segment
    )
    sampledPoints.push(...bezierSamples)
    sampledPoints.push(segment.end)
  }

  // Add anchor points and hub point
  sampledPoints.push(...connectorPath.fromAnchors)
  sampledPoints.push(...connectorPath.toAnchors)
  if (connectorPath.hubPoint) {
    sampledPoints.push(connectorPath.hubPoint)
  }

  // Use point-based intersection with stroke width tolerance
  return testPointsIntersection(
    sampledPoints,
    selectionRect,
    props.strokeWidth ?? 2
  )
}

// =============================================================================
// Group Shape Hit Testing
// =============================================================================

/**
 * Tests if a group shape intersects with a selection rectangle.
 * Uses Konva's getClientRect() API to calculate accurate bounds from the actual rendered node,
 * similar to how createOutlineShape works in use-hover.ts.
 * Requires editor to access the Konva node and resolve child shapes.
 */
function testGroupShapeIntersection(
  shape: GroupShape,
  selectionRect: Bounds,
  editor?: Editor
): HitTestResult {
  // If no editor provided, cannot access Konva node or resolve child shapes
  if (!editor) {
    return { intersects: false, fullyContained: false }
  }

  // Get the Konva node for this group shape
  const node = editor.getShapeNode(shape.id)
  if (!node) {
    return { intersects: false, fullyContained: false }
  }

  // Get the bounding box in the group's local coordinate system (without its own transform)
  const localRect = node.getClientRect({ skipTransform: true })

  // If the group has no meaningful bounds, return no intersection
  if (localRect.width <= 0 || localRect.height <= 0) {
    return { intersects: false, fullyContained: false }
  }

  // Get the group's transform properties from the Konva node
  const layer = node.getLayer()
  const position = layer ? node.getAbsolutePosition(layer) : { x: 0, y: 0 }
  const rotation = node.rotation()
  const scaleX = node.scaleX()
  const scaleY = node.scaleY()

  // Calculate the actual width and height accounting for local rect offset and scale
  const width = localRect.width
  const height = localRect.height

  // The group's bounding box needs to account for the local rect offset
  // (children may not start at 0,0 within the group)
  // We need to transform the local rect to world coordinates

  // Get the rotated vertices of the group's bounding box
  // The position is at the group's origin, and we need to offset by localRect.x/y
  const vertices = getRotatedGroupVertices(
    position.x,
    position.y,
    localRect.x,
    localRect.y,
    width,
    height,
    rotation,
    scaleX,
    scaleY
  )

  // Convert selection rect to polygon
  const selectionPolygon = rectToPolygon(selectionRect)

  const intersects = polygonsIntersect(vertices, selectionPolygon)
  const fullyContained = isPolygonFullyContained(vertices, selectionRect)

  return { intersects, fullyContained }
}

/**
 * Gets the 4 corner vertices of a rotated group's bounding box.
 * Similar to getRotatedRectVertices but accounts for the local rect offset
 * (children within the group may not start at 0,0).
 */
function getRotatedGroupVertices(
  groupX: number,
  groupY: number,
  localOffsetX: number,
  localOffsetY: number,
  width: number,
  height: number,
  rotation: number,
  scaleX: number,
  scaleY: number
): Point[] {
  const rad = (rotation * Math.PI) / 180
  const cos = Math.cos(rad)
  const sin = Math.sin(rad)

  // Local corners relative to group origin, offset by local rect position
  // and scaled by the group's scale factors
  const localCorners = [
    { x: localOffsetX * scaleX, y: localOffsetY * scaleY }, // top-left
    { x: (localOffsetX + width) * scaleX, y: localOffsetY * scaleY }, // top-right
    { x: (localOffsetX + width) * scaleX, y: (localOffsetY + height) * scaleY }, // bottom-right
    { x: localOffsetX * scaleX, y: (localOffsetY + height) * scaleY }, // bottom-left
  ]

  // Transform each corner: rotate around origin, then translate to group position
  return localCorners.map((c) => ({
    x: groupX + c.x * cos - c.y * sin,
    y: groupY + c.x * sin + c.y * cos,
  }))
}

// =============================================================================
// Common Point-Based Intersection
// =============================================================================

/**
 * Tests if a series of points (line/path) intersects with a selection rectangle.
 */
function testPointsIntersection(
  points: Point[],
  selectionRect: Bounds,
  strokeWidth: number
): HitTestResult {
  const tolerance = Math.max(strokeWidth / 2, 1)
  const expandedRect: Bounds = {
    x: selectionRect.x - tolerance,
    y: selectionRect.y - tolerance,
    width: selectionRect.width + tolerance * 2,
    height: selectionRect.height + tolerance * 2,
  }

  // Check if any point is inside the (expanded) selection rect
  const anyPointInRect = points.some((p) => pointInRect(p, expandedRect))

  // Check if any line segment crosses the selection rect
  let anySegmentCrosses = false
  const rectEdges = getRectEdges(selectionRect)

  for (let i = 0; i < points.length - 1 && !anySegmentCrosses; i++) {
    for (const edge of rectEdges) {
      if (segmentsIntersect(points[i], points[i + 1], edge[0], edge[1])) {
        anySegmentCrosses = true
        break
      }
    }
  }

  const intersects = anyPointInRect || anySegmentCrosses

  // For full containment, all points must be in the selection rect
  const fullyContained = points.every((p) => pointInRect(p, selectionRect))

  return { intersects, fullyContained }
}

// =============================================================================
// Polygon Intersection Utilities
// =============================================================================

export {
  pointInPolygon,
  pointInRect,
  segmentsIntersect,
} from "./polygon-geometry"
