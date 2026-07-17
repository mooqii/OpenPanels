import type { ShapeId } from "../types/ids"
import type { GroupShape, Shape } from "../types/shapes"
import { getShapeBounds } from "./coordinates"

export interface AnchorPoint {
  x: number
  y: number
}

/**
 * Rotates a point around an origin by a given angle.
 */
function rotatePoint(
  px: number,
  py: number,
  originX: number,
  originY: number,
  cos: number,
  sin: number
): AnchorPoint {
  const dx = px - originX
  const dy = py - originY
  return {
    x: originX + dx * cos - dy * sin,
    y: originY + dx * sin + dy * cos,
  }
}

/**
 * Gets the rotated anchor points for a rectangle.
 * Konva rotates rectangles around their origin point (x, y) which is the top-left corner.
 *
 * Returns: 4 corners + 4 edge midpoints + center = 9 points
 */
function getRectangleAnchorPoints(
  x: number,
  y: number,
  width: number,
  height: number,
  rotation: number,
  scaleX: number,
  scaleY: number
): AnchorPoint[] {
  const scaledW = width * scaleX
  const scaledH = height * scaleY

  // If no rotation, return simple anchor points
  if (rotation === 0) {
    return [
      // Corners
      { x, y }, // top-left
      { x: x + scaledW, y }, // top-right
      { x: x + scaledW, y: y + scaledH }, // bottom-right
      { x, y: y + scaledH }, // bottom-left
      // Edge midpoints
      { x: x + scaledW / 2, y }, // top-center
      { x: x + scaledW, y: y + scaledH / 2 }, // right-center
      { x: x + scaledW / 2, y: y + scaledH }, // bottom-center
      { x, y: y + scaledH / 2 }, // left-center
      // Center
      { x: x + scaledW / 2, y: y + scaledH / 2 },
    ]
  }

  const rad = (rotation * Math.PI) / 180
  const cos = Math.cos(rad)
  const sin = Math.sin(rad)

  // Local corners relative to origin (top-left at 0,0)
  // Konva rotates around the origin point (x, y)
  const localCorners = [
    { x: 0, y: 0 }, // top-left (origin)
    { x: scaledW, y: 0 }, // top-right
    { x: scaledW, y: scaledH }, // bottom-right
    { x: 0, y: scaledH }, // bottom-left
  ]

  // Transform corners
  const corners = localCorners.map((c) => ({
    x: x + c.x * cos - c.y * sin,
    y: y + c.x * sin + c.y * cos,
  }))

  // Edge midpoints (between corners)
  const edgeMidpoints = [
    {
      x: (corners[0].x + corners[1].x) / 2,
      y: (corners[0].y + corners[1].y) / 2,
    }, // top
    {
      x: (corners[1].x + corners[2].x) / 2,
      y: (corners[1].y + corners[2].y) / 2,
    }, // right
    {
      x: (corners[2].x + corners[3].x) / 2,
      y: (corners[2].y + corners[3].y) / 2,
    }, // bottom
    {
      x: (corners[3].x + corners[0].x) / 2,
      y: (corners[3].y + corners[0].y) / 2,
    }, // left
  ]

  // Center (average of all corners)
  const center = {
    x: (corners[0].x + corners[1].x + corners[2].x + corners[3].x) / 4,
    y: (corners[0].y + corners[1].y + corners[2].y + corners[3].y) / 4,
  }

  return [...corners, ...edgeMidpoints, center]
}

/**
 * Gets the rotated anchor points for an ellipse.
 * Konva rotates ellipses around their center point (x, y).
 *
 * For an ellipse, we return the 4 extreme points (where the ellipse touches
 * its rotated bounding box) plus the center.
 *
 * Returns: 4 extreme points + center = 5 points
 */
function getEllipseAnchorPoints(
  cx: number,
  cy: number,
  width: number,
  height: number,
  rotation: number
): AnchorPoint[] {
  const rx = width / 2
  const ry = height / 2

  // If no rotation, return cardinal extreme points
  if (rotation === 0) {
    return [
      { x: cx, y: cy - ry }, // top
      { x: cx + rx, y: cy }, // right
      { x: cx, y: cy + ry }, // bottom
      { x: cx - rx, y: cy }, // left
      { x: cx, y: cy }, // center
    ]
  }

  const rad = (rotation * Math.PI) / 180
  const cos = Math.cos(rad)
  const sin = Math.sin(rad)

  // For a rotated ellipse, the extreme points in the x and y directions
  // can be found using calculus. The parametric form of the ellipse is:
  // x(t) = cx + rx*cos(t)*cos(θ) - ry*sin(t)*sin(θ)
  // y(t) = cy + rx*cos(t)*sin(θ) + ry*sin(t)*cos(θ)
  //
  // The extreme points in x occur when dx/dt = 0:
  // -rx*sin(t)*cos(θ) - ry*cos(t)*sin(θ) = 0
  // tan(t) = -ry*tan(θ)/rx
  //
  // Similarly for y extremes when dy/dt = 0:
  // -rx*sin(t)*sin(θ) + ry*cos(t)*cos(θ) = 0
  // tan(t) = ry*cot(θ)/rx

  // Calculate parameter t for x extremes
  const tForXExtremes = Math.atan2(-ry * sin, rx * cos)
  // Calculate parameter t for y extremes
  const tForYExtremes = Math.atan2(ry * cos, rx * sin)

  // Get the 4 extreme points
  const extremes: AnchorPoint[] = []

  // X extremes (two points, t and t + π)
  for (const t of [tForXExtremes, tForXExtremes + Math.PI]) {
    const cosT = Math.cos(t)
    const sinT = Math.sin(t)
    extremes.push({
      x: cx + rx * cosT * cos - ry * sinT * sin,
      y: cy + rx * cosT * sin + ry * sinT * cos,
    })
  }

  // Y extremes (two points, t and t + π)
  for (const t of [tForYExtremes, tForYExtremes + Math.PI]) {
    const cosT = Math.cos(t)
    const sinT = Math.sin(t)
    extremes.push({
      x: cx + rx * cosT * cos - ry * sinT * sin,
      y: cy + rx * cosT * sin + ry * sinT * cos,
    })
  }

  // Add center
  extremes.push({ x: cx, y: cy })

  return extremes
}

/**
 * Gets the rotated anchor points for a line.
 * Konva rotates lines around their start point (x, y).
 *
 * Returns: start point + end point + midpoint = 3 points
 */
function getLineAnchorPoints(
  x: number,
  y: number,
  width: number,
  height: number,
  rotation: number,
  scaleX: number,
  scaleY: number
): AnchorPoint[] {
  const scaledW = width * scaleX
  const scaledH = height * scaleY

  // Start point is always at (x, y)
  const start = { x, y }

  // If no rotation, end point is at (x + width, y + height)
  if (rotation === 0) {
    const end = { x: x + scaledW, y: y + scaledH }
    const mid = { x: (start.x + end.x) / 2, y: (start.y + end.y) / 2 }
    return [start, end, mid]
  }

  // Rotate end point around start point
  const rad = (rotation * Math.PI) / 180
  const cos = Math.cos(rad)
  const sin = Math.sin(rad)

  const end = rotatePoint(x + scaledW, y + scaledH, x, y, cos, sin)
  const mid = { x: (start.x + end.x) / 2, y: (start.y + end.y) / 2 }

  return [start, end, mid]
}

/**
 * Gets the anchor points for a shape, accounting for rotation.
 * Unlike getShapeBounds() which returns AABB, this returns the actual
 * rotated corner/edge positions for accurate alignment detection.
 *
 * @param shape - The shape to get anchor points for
 * @returns Array of anchor points (corners, edge midpoints, center)
 */
export function getRotatedAnchorPoints(shape: Shape): AnchorPoint[] {
  switch (shape.type) {
    case "geo": {
      const props = shape.props

      if (props.geo === "ellipse") {
        const { x = 0, y = 0, width = 0, height = 0, rotation = 0 } = props
        return getEllipseAnchorPoints(x, y, width, height, rotation)
      }

      if (props.geo === "line") {
        const {
          x = 0,
          y = 0,
          width = 0,
          height = 0,
          rotation = 0,
          scaleX = 1,
          scaleY = 1,
        } = props
        return getLineAnchorPoints(
          x,
          y,
          width,
          height,
          rotation,
          scaleX,
          scaleY
        )
      }

      // Rectangle and other geo shapes
      const {
        x = 0,
        y = 0,
        width = 0,
        height = 0,
        rotation = 0,
        scaleX = 1,
        scaleY = 1,
      } = props
      return getRectangleAnchorPoints(
        x,
        y,
        width,
        height,
        rotation,
        scaleX,
        scaleY
      )
    }

    case "placeholder": {
      const props = shape.props
      const {
        x = 0,
        y = 0,
        width = 0,
        height = 0,
        rotation = 0,
        scaleX = 1,
        scaleY = 1,
      } = props
      return getRectangleAnchorPoints(
        x,
        y,
        width,
        height,
        rotation,
        scaleX,
        scaleY
      )
    }

    case "text":
    case "image": {
      const { x = 0, y = 0, scaleX = 1, scaleY = 1, rotation = 0 } = shape.props
      const width = (shape.props as any).width ?? 0
      const height = (shape.props as any).height ?? 0
      return getRectangleAnchorPoints(
        x,
        y,
        width,
        height,
        rotation,
        scaleX,
        scaleY
      )
    }

    // For draw, brush, path shapes - fall back to AABB anchor points
    // These shapes don't have a simple rotated geometry
    default: {
      const bounds = getShapeBounds(shape)
      return [
        // Corners
        { x: bounds.x, y: bounds.y },
        { x: bounds.x + bounds.width, y: bounds.y },
        { x: bounds.x + bounds.width, y: bounds.y + bounds.height },
        { x: bounds.x, y: bounds.y + bounds.height },
        // Edge midpoints
        { x: bounds.x + bounds.width / 2, y: bounds.y },
        { x: bounds.x + bounds.width, y: bounds.y + bounds.height / 2 },
        { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height },
        { x: bounds.x, y: bounds.y + bounds.height / 2 },
        // Center
        { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 },
      ]
    }
  }
}

// =============================================================================
// Absolute Coordinate Transformations (for shapes inside groups)
// =============================================================================

/**
 * Transform representing position, rotation, and scale of a group.
 */
interface ShapeTransform {
  rotation: number
  scaleX: number
  scaleY: number
  x: number
  y: number
}

/**
 * Applies a single transform to a point.
 * Transform order: scale -> rotate -> translate (matching Konva's transform order)
 *
 * @param point - The point to transform
 * @param transform - The transform to apply
 * @returns The transformed point
 */
function applyTransformToPoint(
  point: AnchorPoint,
  transform: ShapeTransform
): AnchorPoint {
  const { x, y, scaleX, scaleY, rotation } = transform

  // Apply scale
  let px = point.x * scaleX
  let py = point.y * scaleY

  // Apply rotation around origin
  if (rotation !== 0) {
    const rad = (rotation * Math.PI) / 180
    const cos = Math.cos(rad)
    const sin = Math.sin(rad)
    const rx = px * cos - py * sin
    const ry = px * sin + py * cos
    px = rx
    py = ry
  }

  // Apply translation
  return { x: px + x, y: py + y }
}

/**
 * Collects transforms from all ancestor groups of a shape.
 * Returns transforms in order from innermost (direct parent) to outermost (top-level group).
 *
 * @param shape - The shape to get ancestor transforms for
 * @param getShapeById - Function to resolve shapes by ID
 * @returns Array of transforms from ancestors
 */
function getAncestorTransforms(
  shape: Shape,
  getShapeById: (id: ShapeId) => Shape | undefined
): ShapeTransform[] {
  const transforms: ShapeTransform[] = []

  let currentParentId = shape.parentId

  // Walk up the hierarchy until we hit a page (non-shape parent)
  while (currentParentId.startsWith("shape:")) {
    const parent = getShapeById(currentParentId as ShapeId)
    if (!parent || parent.type !== "group") break

    const groupProps = parent.props as GroupShape["props"]
    transforms.push({
      x: groupProps.x ?? 0,
      y: groupProps.y ?? 0,
      rotation: groupProps.rotation ?? 0,
      scaleX: groupProps.scaleX ?? 1,
      scaleY: groupProps.scaleY ?? 1,
    })

    currentParentId = parent.parentId
  }

  return transforms
}

/**
 * Transforms a point from local coordinates to absolute canvas coordinates
 * by applying all ancestor transforms.
 *
 * @param point - The point in local coordinates
 * @param transforms - Array of ancestor transforms (innermost to outermost)
 * @returns The point in absolute canvas coordinates
 */
function transformPointToAbsolute(
  point: AnchorPoint,
  transforms: ShapeTransform[]
): AnchorPoint {
  let result = point

  // Apply transforms from innermost to outermost
  for (const transform of transforms) {
    result = applyTransformToPoint(result, transform)
  }

  return result
}

/**
 * Gets the rotated anchor points for a shape in absolute canvas coordinates.
 * For shapes inside groups, this transforms the local anchor points through
 * all ancestor group transforms to get absolute positions.
 *
 * @param shape - The shape to get anchor points for
 * @param getShapeById - Function to resolve shapes by ID (for finding parent groups)
 * @returns Array of anchor points in absolute canvas coordinates
 */
export function getAbsoluteRotatedAnchorPoints(
  shape: Shape,
  getShapeById: (id: ShapeId) => Shape | undefined
): AnchorPoint[] {
  // Get local anchor points
  const localAnchors = getRotatedAnchorPoints(shape)

  // If shape is directly on a page (not in a group), return local anchors as-is
  if (!shape.parentId.startsWith("shape:")) {
    return localAnchors
  }

  // Get transforms from all ancestor groups
  const transforms = getAncestorTransforms(shape, getShapeById)

  // If no ancestor transforms, return local anchors
  if (transforms.length === 0) {
    return localAnchors
  }

  // Transform each anchor point to absolute coordinates
  return localAnchors.map((anchor) =>
    transformPointToAbsolute(anchor, transforms)
  )
}
