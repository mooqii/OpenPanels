import type Konva from "konva"
import type {
  Bounds,
  BrushShape,
  DrawShape,
  PathShape,
  Shape,
} from "../types/shapes"

/**
 * Calculates the axis-aligned bounding box (AABB) of a rotated rectangle.
 * Given a rectangle's center, dimensions, and rotation angle, computes the
 * smallest axis-aligned box that contains the rotated rectangle.
 *
 * @param cx - Center X coordinate
 * @param cy - Center Y coordinate
 * @param width - Width of the rectangle (before rotation)
 * @param height - Height of the rectangle (before rotation)
 * @param rotation - Rotation angle in degrees
 * @returns The axis-aligned bounding box
 */
function getRotatedAABB(
  cx: number,
  cy: number,
  width: number,
  height: number,
  rotation: number
): Bounds {
  // No rotation - return simple bounds
  if (!rotation) {
    return {
      x: cx - width / 2,
      y: cy - height / 2,
      width,
      height,
    }
  }

  const rad = (rotation * Math.PI) / 180
  const cos = Math.cos(rad)
  const sin = Math.sin(rad)
  const hw = width / 2
  const hh = height / 2

  // Transform all 4 corners relative to center
  const corners = [
    { x: cx + (-hw * cos - -hh * sin), y: cy + (-hw * sin + -hh * cos) },
    { x: cx + (hw * cos - -hh * sin), y: cy + (hw * sin + -hh * cos) },
    { x: cx + (hw * cos - hh * sin), y: cy + (hw * sin + hh * cos) },
    { x: cx + (-hw * cos - hh * sin), y: cy + (-hw * sin + hh * cos) },
  ]

  // Find min/max from rotated corners
  let minX = corners[0].x
  let maxX = corners[0].x
  let minY = corners[0].y
  let maxY = corners[0].y

  for (let i = 1; i < corners.length; i++) {
    minX = Math.min(minX, corners[i].x)
    maxX = Math.max(maxX, corners[i].x)
    minY = Math.min(minY, corners[i].y)
    maxY = Math.max(maxY, corners[i].y)
  }

  return {
    x: minX,
    y: minY,
    width: maxX - minX,
    height: maxY - minY,
  }
}

/**
 * Transforms stage coordinates to world/canvas coordinates,
 * accounting for stage position (pan) and scale (zoom).
 *
 * @param stage - The Konva stage instance
 * @returns The transformed pointer position in world coordinates, or null if unavailable
 */
export function getPointerPosition(
  stage: Konva.Stage | null
): { x: number; y: number } | null {
  if (!stage) return null
  const pos = stage.getPointerPosition()
  if (!pos) return null

  // Account for stage position (pan) and scale (zoom)
  const stagePos = stage.position()
  const scale = stage.scaleX() // Assuming uniform scaling
  return {
    x: (pos.x - stagePos.x) / scale,
    y: (pos.y - stagePos.y) / scale,
  }
}

/**
 * Transforms stage coordinates to world/canvas coordinates using a transform object.
 * This is useful when you already have the transform values rather than a stage instance.
 *
 * @param stageX - Stage X coordinate
 * @param stageY - Stage Y coordinate
 * @param transform - Stage transform object with x, y, scaleX, and scaleY
 * @returns The transformed coordinates in world space
 */
export function stageToCanvas(
  stageX: number,
  stageY: number,
  transform: { x: number; y: number; scaleX: number; scaleY: number }
): { x: number; y: number } {
  return {
    x: (stageX - transform.x) / transform.scaleX,
    y: (stageY - transform.y) / transform.scaleY,
  }
}

/**
 * Transforms stage coordinates to world/canvas coordinates using a Konva stage instance.
 *
 * @param stage - The Konva stage instance
 * @param stageX - Stage X coordinate
 * @param stageY - Stage Y coordinate
 * @returns The transformed coordinates in world space
 */
export function getCanvasPosition(
  stage: Konva.Stage,
  stageX: number,
  stageY: number
): { x: number; y: number } {
  const stagePos = stage.position()
  const scale = stage.scaleX()
  return stageToCanvas(stageX, stageY, {
    x: stagePos.x,
    y: stagePos.y,
    scaleX: scale,
    scaleY: scale,
  })
}

/**
 * Gets the center point of the current viewport in canvas/world coordinates.
 *
 * @param stage - The Konva stage instance
 * @returns The center point in world coordinates, or null if stage is unavailable
 */
export function getViewportCenter(
  stage: Konva.Stage | null
): { x: number; y: number } | null {
  if (!stage) return null
  return getCanvasPosition(stage, stage.width() / 2, stage.height() / 2)
}

/**
 * Transforms world/canvas coordinates to stage coordinates,
 * accounting for stage position (pan) and scale (zoom).
 *
 * @param stage - The Konva stage instance
 * @param canvasX - Canvas X coordinate
 * @param canvasY - Canvas Y coordinate
 * @returns The transformed coordinates in stage space, or null if stage is unavailable
 */
export function canvasToStage(
  stage: Konva.Stage | null,
  canvasX: number,
  canvasY: number
): { x: number; y: number } | null {
  if (!stage) return null

  const stagePos = stage.position()
  const scale = stage.scaleX() // Assuming uniform scaling
  const stageBox = stage.container().getBoundingClientRect()

  return {
    x: stageBox.left + (canvasX * scale + stagePos.x),
    y: stageBox.top + (canvasY * scale + stagePos.y),
  }
}

/**
 * Transforms world/canvas coordinates to stage coordinates using a transform object.
 * This is useful when you already have the transform values rather than a stage instance.
 *
 * @param canvasX - Canvas X coordinate
 * @param canvasY - Canvas Y coordinate
 * @param transform - Stage transform object with x, y, scaleX, and scaleY
 * @param containerRect - The bounding rectangle of the stage container (for offset)
 * @returns The transformed coordinates in stage space
 */
export function canvasToStageWithTransform(
  canvasX: number,
  canvasY: number,
  transform: { x: number; y: number; scaleX: number; scaleY: number },
  containerRect: { left: number; top: number }
): { x: number; y: number } {
  return {
    x: containerRect.left + (canvasX * transform.scaleX + transform.x),
    y: containerRect.top + (canvasY * transform.scaleY + transform.y),
  }
}

/**
 * Transforms bounds from canvas/world coordinates to stage coordinates,
 * accounting for stage position (pan) and scale (zoom).
 *
 * @param stage - The Konva stage instance
 * @param bounds - Bounds in canvas coordinates
 * @returns The transformed bounds in stage space, or null if stage is unavailable
 */
export function canvasBoundsToStageBounds(
  stage: Konva.Stage | null,
  bounds: Bounds
): Bounds | null {
  if (!stage) return null

  const scale = stage.scaleX() // Assuming uniform scaling

  // Convert top-left corner
  const topLeft = canvasToStage(stage, bounds.x, bounds.y)
  if (!topLeft) return null

  // Scale width and height
  const stageWidth = bounds.width * scale
  const stageHeight = bounds.height * scale

  return {
    x: topLeft.x,
    y: topLeft.y,
    width: stageWidth,
    height: stageHeight,
  }
}

/**
 * Transforms bounds from canvas/world coordinates to stage coordinates using a transform object.
 * This is useful when you already have the transform values rather than a stage instance.
 *
 * @param bounds - Bounds in canvas coordinates
 * @param transform - Stage transform object with x, y, scaleX, and scaleY
 * @param containerRect - The bounding rectangle of the stage container (for offset)
 * @returns The transformed bounds in stage space
 */
export function canvasBoundsToStageBoundsWithTransform(
  bounds: Bounds,
  transform: { x: number; y: number; scaleX: number; scaleY: number },
  containerRect: { left: number; top: number }
): Bounds {
  // Convert top-left corner
  const topLeft = canvasToStageWithTransform(
    bounds.x,
    bounds.y,
    transform,
    containerRect
  )

  // Scale width and height
  const stageWidth = bounds.width * transform.scaleX
  const stageHeight = bounds.height * transform.scaleY

  return {
    x: topLeft.x,
    y: topLeft.y,
    width: stageWidth,
    height: stageHeight,
  }
}

/**
 * Calculates the bounding box for a single shape.
 * Handles all shape types correctly:
 * - Geo (rectangle): Uses x, y, width, height with scale
 * - Geo (ellipse): Uses center (x, y) and radiusX/radiusY
 * - Draw/Brush: Calculates bounds from flat points array
 * - Path: Calculates bounds from PathPoint array
 * - Text/Image: Uses x, y, width, height with scale
 *
 * @param shape - The shape to calculate bounds for
 * @returns The bounding box of the shape
 */
export function getShapeBounds(shape: Shape): Bounds {
  switch (shape.type) {
    case "geo": {
      const props = shape.props
      if (props.geo === "ellipse") {
        // Ellipse: (x, y) is the center, uses width/height
        const { x = 0, y = 0, width = 0, height = 0, rotation = 0 } = props
        // Ellipse center is already at (x, y)
        return getRotatedAABB(x, y, width, height, rotation)
      }
      if (props.geo === "line") {
        // Line: (x, y) is start point, width/height is end point relative to start
        const {
          x = 0,
          y = 0,
          width = 0,
          height = 0,
          scaleX = 1,
          scaleY = 1,
          arrowSize,
          strokeWidth = 1,
          rotation = 0,
        } = props
        const scaledW = width * scaleX
        const scaledH = height * scaleY
        const arrowPadding =
          (arrowSize ?? strokeWidth * 2) * Math.max(scaleX, scaleY)
        // Line bounds with arrow padding
        const boundsW = scaledW + arrowPadding * 2
        const boundsH = scaledH + arrowPadding * 2
        // Center of the line bounds (accounting for arrow padding offset)
        const cx = x - arrowPadding + boundsW / 2
        const cy = y - arrowPadding + boundsH / 2
        return getRotatedAABB(cx, cy, boundsW, boundsH, rotation)
      }
      // Rectangle: (x, y) is top-left, uses width/height with scale
      const {
        x = 0,
        y = 0,
        width = 0,
        height = 0,
        scaleX = 1,
        scaleY = 1,
        rotation = 0,
      } = props
      const scaledW = width * scaleX
      const scaledH = height * scaleY
      // Calculate center from top-left position
      const cx = x + scaledW / 2
      const cy = y + scaledH / 2
      return getRotatedAABB(cx, cy, scaledW, scaledH, rotation)
    }

    case "draw": {
      // Draw shape: flat points array [x1, y1, x2, y2, ...]
      return calculateDrawShapeBounds(shape)
    }

    case "brush": {
      // Brush shape: array of BrushPoint objects
      return calculateBrushShapeBounds(shape)
    }

    case "path": {
      // Path shape: array of PathPoint objects
      return calculatePathShapeBounds(shape)
    }

    case "placeholder": {
      const props = shape.props
      const {
        x = 0,
        y = 0,
        width = 0,
        height = 0,
        scaleX = 1,
        scaleY = 1,
        rotation = 0,
      } = props
      const scaledW = width * scaleX
      const scaledH = height * scaleY
      const cx = x + scaledW / 2
      const cy = y + scaledH / 2
      return getRotatedAABB(cx, cy, scaledW, scaledH, rotation)
    }

    case "text":
    case "image": {
      const { x = 0, y = 0, scaleX = 1, scaleY = 1, rotation = 0 } = shape.props
      const width = (shape.props as any).width ?? 0
      const height = (shape.props as any).height ?? 0
      const scaledW = width * scaleX
      const scaledH = height * scaleY
      // Calculate center from top-left position
      const cx = x + scaledW / 2
      const cy = y + scaledH / 2
      return getRotatedAABB(cx, cy, scaledW, scaledH, rotation)
    }

    default: {
      // Fallback for unknown shape types
      const { x = 0, y = 0 } = (shape as any).props ?? {}
      const width = ((shape as any).props as any)?.width ?? 0
      const height = ((shape as any).props as any)?.height ?? 0
      return { x, y, width, height }
    }
  }
}

/**
 * Calculates bounds for a draw shape from its flat points array.
 */
function calculateDrawShapeBounds(shape: DrawShape): Bounds {
  const {
    x = 0,
    y = 0,
    points,
    width,
    height,
    scaleX = 1,
    scaleY = 1,
  } = shape.props
  const rotation = (shape.props as any).rotation ?? 0

  // If width/height are already computed, use them
  if (width !== undefined && height !== undefined) {
    const scaledW = width * scaleX
    const scaledH = height * scaleY
    const cx = x + scaledW / 2
    const cy = y + scaledH / 2
    return getRotatedAABB(cx, cy, scaledW, scaledH, rotation)
  }

  // Calculate from points array [x1, y1, x2, y2, ...]
  if (!points || points.length < 2) {
    return { x, y, width: 0, height: 0 }
  }

  let minX = Number.POSITIVE_INFINITY
  let minY = Number.POSITIVE_INFINITY
  let maxX = Number.NEGATIVE_INFINITY
  let maxY = Number.NEGATIVE_INFINITY

  for (let i = 0; i < points.length; i += 2) {
    const px = points[i]
    const py = points[i + 1]
    minX = Math.min(minX, px)
    minY = Math.min(minY, py)
    maxX = Math.max(maxX, px)
    maxY = Math.max(maxY, py)
  }

  const scaledW = (maxX - minX) * scaleX
  const scaledH = (maxY - minY) * scaleY
  const boundsX = x + minX * scaleX
  const boundsY = y + minY * scaleY
  const cx = boundsX + scaledW / 2
  const cy = boundsY + scaledH / 2
  return getRotatedAABB(cx, cy, scaledW, scaledH, rotation)
}

/**
 * Calculates bounds for a brush shape from its BrushPoint array.
 */
function calculateBrushShapeBounds(shape: BrushShape): Bounds {
  const {
    x = 0,
    y = 0,
    points,
    width,
    height,
    scaleX = 1,
    scaleY = 1,
  } = shape.props

  // If width/height are already computed, use them
  if (width !== undefined && height !== undefined) {
    return {
      x,
      y,
      width: width * scaleX,
      height: height * scaleY,
    }
  }

  // Calculate from points array
  if (!points || points.length === 0) {
    return { x, y, width: 0, height: 0 }
  }

  let minX = Number.POSITIVE_INFINITY
  let minY = Number.POSITIVE_INFINITY
  let maxX = Number.NEGATIVE_INFINITY
  let maxY = Number.NEGATIVE_INFINITY

  for (const point of points) {
    minX = Math.min(minX, point.x)
    minY = Math.min(minY, point.y)
    maxX = Math.max(maxX, point.x)
    maxY = Math.max(maxY, point.y)
  }

  return {
    x: x + minX * scaleX,
    y: y + minY * scaleY,
    width: (maxX - minX) * scaleX,
    height: (maxY - minY) * scaleY,
  }
}

/**
 * Calculates bounds for a path shape from its PathPoint array.
 */
function calculatePathShapeBounds(shape: PathShape): Bounds {
  const {
    x = 0,
    y = 0,
    points,
    width,
    height,
    scaleX = 1,
    scaleY = 1,
  } = shape.props

  // If width/height are already computed, use them
  if (width !== undefined && height !== undefined) {
    return {
      x,
      y,
      width: width * scaleX,
      height: height * scaleY,
    }
  }

  // Calculate from points array
  if (!points || points.length === 0) {
    return { x, y, width: 0, height: 0 }
  }

  let minX = Number.POSITIVE_INFINITY
  let minY = Number.POSITIVE_INFINITY
  let maxX = Number.NEGATIVE_INFINITY
  let maxY = Number.NEGATIVE_INFINITY

  for (const point of points) {
    // Include the anchor point
    minX = Math.min(minX, point.x)
    minY = Math.min(minY, point.y)
    maxX = Math.max(maxX, point.x)
    maxY = Math.max(maxY, point.y)

    // Include control handles if present (they affect the visual bounds)
    if (point.handleIn) {
      const hx = point.x + point.handleIn.x
      const hy = point.y + point.handleIn.y
      minX = Math.min(minX, hx)
      minY = Math.min(minY, hy)
      maxX = Math.max(maxX, hx)
      maxY = Math.max(maxY, hy)
    }
    if (point.handleOut) {
      const hx = point.x + point.handleOut.x
      const hy = point.y + point.handleOut.y
      minX = Math.min(minX, hx)
      minY = Math.min(minY, hy)
      maxX = Math.max(maxX, hx)
      maxY = Math.max(maxY, hy)
    }
  }

  return {
    x: x + minX * scaleX,
    y: y + minY * scaleY,
    width: (maxX - minX) * scaleX,
    height: (maxY - minY) * scaleY,
  }
}

/**
 * Calculates the combined bounding box of multiple Konva shapes.
 * Accounts for rotation, scaling, and all transformations.
 *
 * @param shapes - Array of shape nodes
 * @returns The combined bounding box, or null if the array is empty
 */
export function getShapesBounds(shapes: Shape[]): Bounds {
  if (shapes.length === 0) {
    return {
      x: 0,
      y: 0,
      width: 0,
      height: 0,
    }
  }

  let minX = Number.POSITIVE_INFINITY
  let minY = Number.POSITIVE_INFINITY
  let maxX = Number.NEGATIVE_INFINITY
  let maxY = Number.NEGATIVE_INFINITY

  for (const shape of shapes) {
    const bounds = getShapeBounds(shape)
    minX = Math.min(minX, bounds.x)
    minY = Math.min(minY, bounds.y)
    maxX = Math.max(maxX, bounds.x + bounds.width)
    maxY = Math.max(maxY, bounds.y + bounds.height)
  }

  if (
    minX === Number.POSITIVE_INFINITY ||
    minY === Number.POSITIVE_INFINITY ||
    maxX === Number.NEGATIVE_INFINITY ||
    maxY === Number.NEGATIVE_INFINITY
  ) {
    return {
      x: 0,
      y: 0,
      width: 0,
      height: 0,
    }
  }

  return {
    x: minX,
    y: minY,
    width: maxX - minX,
    height: maxY - minY,
  }
}

// =============================================================================
// Rotated Anchor Points for Alignment
// =============================================================================

/**
 * Point type for anchor coordinates
 */

export {
  type AnchorPoint,
  getAbsoluteRotatedAnchorPoints,
  getRotatedAnchorPoints,
} from "./coordinate-anchors"
