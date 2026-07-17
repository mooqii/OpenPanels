import type Konva from "konva"
import type { Editor } from "../editor"
import type { ShapeId } from "../types/ids"
import { getAbsoluteRotatedAnchorPoints } from "../utils/coordinates"

export const ALIGNMENT_TOLERANCE = 5 // pixels - distance to detect alignment
export const SNAP_THRESHOLD = 5 // pixels - distance to snap to alignment
export const SNAP_BREAK_THRESHOLD = 8 // pixels - distance to break free from snap
export const CROSS_SIZE = 3 // half-size of the cross mark

// Initial pool sizes
export const GUIDE_POOL_SIZE = 10
export const CROSS_POOL_SIZE = 50 // Each cross needs 2 lines, so 25 crosses max

export interface AlignmentGuide {
  type: "h" | "v" // horizontal or vertical
  x1: number
  x2: number
  y1: number
  y2: number
}

export interface AnchorCross {
  x: number
  y: number
}

export interface AlignmentResult {
  crosses: AnchorCross[]
  guides: AlignmentGuide[]
  snapX: SnapInfo | null
  snapY: SnapInfo | null
}

export interface SnapInfo {
  dragAnchorPosition: number // The current position of the matched anchor on dragged shape
  targetPosition: number // The position to snap to (anchor on other shape)
}

export interface StageTransform {
  scaleX: number
  scaleY: number
  x: number
  y: number
}

export interface SnapState {
  x: { snappedNodeX: number } | null
  y: { snappedNodeY: number } | null
}

export function roundForKey(value: number): number {
  return Math.round(value)
}

export interface AnchorPoint {
  x: number
  y: number
}

export interface VerticalMatch {
  diff: number
  dragAnchorX: number // Current x position of matched anchor on dragged shape
  dragAnchorY: number
  otherAnchorY: number
  x: number // Target x position (from other shape)
}

export interface HorizontalMatch {
  diff: number
  dragAnchorX: number
  dragAnchorY: number // Current y position of matched anchor on dragged shape
  otherAnchorX: number
  y: number // Target y position (from other shape)
}

/**
 * Gets the rotated anchor points for a dragged shape in absolute canvas coordinates,
 * accounting for its current Konva node position (which may differ from the stored
 * shape data during drag) and any parent group transforms.
 *
 * For group shapes, uses Konva's getClientRect() API to calculate accurate bounds
 * from the actual rendered children, similar to createOutlineShape in use-hover.ts.
 */
export function getDraggedShapeRotatedAnchors(
  editor: Editor,
  shapeId: ShapeId,
  node: Konva.Node
): AnchorPoint[] {
  const shape = editor.getShape(shapeId)
  if (!shape) return []

  // For group shapes, use Konva API to get accurate bounds from rendered children
  if (shape.type === "group") {
    return getGroupShapeAnchorsFromKonva(node)
  }

  // Get the current position from the Konva node (which is being dragged)
  // This is the local position within the parent (group or page)
  const currentX = node.x()
  const currentY = node.y()

  const tempProps: any = {
    ...shape.props,
    x: currentX,
    y: currentY,
    rotation: node.rotation(),
  }

  // Incorporate node scaling into width/height for shape calculation
  if (typeof node.width() === "number") {
    tempProps.width = Math.abs(node.width() * node.scaleX())
  }
  if (typeof node.height() === "number") {
    tempProps.height = Math.abs(node.height() * node.scaleY())
  }
  if ("scaleX" in tempProps) tempProps.scaleX = 1
  if ("scaleY" in tempProps) tempProps.scaleY = 1

  // Create a temporary shape with the current position for anchor calculation
  // We need to update x, y in props to reflect the current drag position
  const tempShape = {
    ...shape,
    props: tempProps,
  } as typeof shape

  // Use absolute anchor points to account for parent group transforms
  // This ensures shapes inside groups align correctly with shapes outside groups
  return getAbsoluteRotatedAnchorPoints(tempShape, (id) => editor.getShape(id))
}

/**
 * Gets anchor points for a group shape using Konva's API.
 * Uses getClientRect() to calculate accurate bounds from the actual rendered children,
 * then transforms those bounds to world coordinates accounting for rotation and scale.
 */
export function getGroupShapeAnchorsFromKonva(node: Konva.Node): AnchorPoint[] {
  // Get the bounding box in the group's local coordinate system (without its own transform)
  const localRect = node.getClientRect({ skipTransform: true })

  if (localRect.width <= 0 || localRect.height <= 0) {
    return []
  }

  // Get the group's transform properties from the Konva node
  const layer = node.getLayer()
  const position = layer ? node.getAbsolutePosition(layer) : { x: 0, y: 0 }
  const rotation = node.rotation()
  const scaleX = node.scaleX()
  const scaleY = node.scaleY()

  // Calculate the rotated corners of the group's bounding box
  const rad = (rotation * Math.PI) / 180
  const cos = Math.cos(rad)
  const sin = Math.sin(rad)

  // Local corners relative to group origin, offset by local rect position
  // and scaled by the group's scale factors
  const { x: localOffsetX, y: localOffsetY, width, height } = localRect

  // Define anchor points in local space (corners, edge midpoints, and center)
  const localAnchors = [
    // Corners
    { x: localOffsetX, y: localOffsetY }, // top-left
    { x: localOffsetX + width, y: localOffsetY }, // top-right
    { x: localOffsetX + width, y: localOffsetY + height }, // bottom-right
    { x: localOffsetX, y: localOffsetY + height }, // bottom-left
    // Edge midpoints
    { x: localOffsetX + width / 2, y: localOffsetY }, // top-center
    { x: localOffsetX + width, y: localOffsetY + height / 2 }, // right-center
    { x: localOffsetX + width / 2, y: localOffsetY + height }, // bottom-center
    { x: localOffsetX, y: localOffsetY + height / 2 }, // left-center
    // Center
    { x: localOffsetX + width / 2, y: localOffsetY + height / 2 },
  ]

  // Transform each anchor to world coordinates:
  // 1. Apply scale
  // 2. Apply rotation around origin
  // 3. Translate to group position
  return localAnchors.map((anchor) => {
    const scaledX = anchor.x * scaleX
    const scaledY = anchor.y * scaleY
    return {
      x: position.x + scaledX * cos - scaledY * sin,
      y: position.y + scaledX * sin + scaledY * cos,
    }
  })
}

/**
 * Gets combined rotated anchor points for multiple dragged shapes.
 */
export function getCombinedDraggedAnchors(
  editor: Editor,
  shapeIds: ShapeId[],
  nodes: Konva.Node[]
): AnchorPoint[] {
  const allAnchors: AnchorPoint[] = []

  for (let i = 0; i < shapeIds.length; i++) {
    const shapeId = shapeIds[i]
    const node = nodes[i]
    if (node) {
      const anchors = getDraggedShapeRotatedAnchors(editor, shapeId, node)
      allAnchors.push(...anchors)
    }
  }

  return allAnchors
}

/**
 * Collects all shape IDs to exclude from alignment detection, including
 * all descendants of any group shapes in the list.
 */
export function collectExcludedShapeIds(
  editor: Editor,
  shapeIds: ShapeId[]
): ShapeId[] {
  const excludeSet = new Set<ShapeId>()

  for (const shapeId of shapeIds) {
    // Add the shape itself
    excludeSet.add(shapeId)

    // If it's a group, add all its descendants
    const shape = editor.getShape(shapeId)
    if (shape && shape.type === "group") {
      const descendants = editor.getShapeDescendants(shapeId)
      for (const descendant of descendants) {
        excludeSet.add(descendant.id)
      }
    }
  }

  return Array.from(excludeSet)
}

export function detectAlignments(
  editor: Editor,
  dragAnchors: AnchorPoint[],
  excludeShapeIds: ShapeId | ShapeId[],
  tolerance: number
): AlignmentResult {
  const shapes = editor.getCurrentPageShapes()
  if (shapes.length === 0)
    return { guides: [], crosses: [], snapX: null, snapY: null }

  const excludeSet = new Set(
    Array.isArray(excludeShapeIds) ? excludeShapeIds : [excludeShapeIds]
  )

  const verticalMatches: VerticalMatch[] = []
  const horizontalMatches: HorizontalMatch[] = []
  const crossSet = new Set<string>()
  const crosses: AnchorCross[] = []

  // Create a shape resolver function for absolute coordinate calculation
  const getShapeById = (id: ShapeId) => editor.getShape(id)

  for (const shape of shapes) {
    if (excludeSet.has(shape.id)) continue

    // Use absolute rotated anchor points for accurate alignment with rotated shapes
    // This returns the actual corners/edges in absolute canvas coordinates,
    // accounting for parent group transforms when shapes are inside groups
    let otherAnchors: AnchorPoint[]
    try {
      otherAnchors = getAbsoluteRotatedAnchorPoints(shape, getShapeById)
    } catch {
      continue
    }

    for (const dragAnchor of dragAnchors) {
      for (const otherAnchor of otherAnchors) {
        // Check vertical alignments (same x-coordinate)
        const xDiff = Math.abs(dragAnchor.x - otherAnchor.x)
        if (xDiff < tolerance) {
          verticalMatches.push({
            x: otherAnchor.x,
            dragAnchorX: dragAnchor.x,
            dragAnchorY: dragAnchor.y,
            otherAnchorY: otherAnchor.y,
            diff: xDiff,
          })

          const dragCrossKey = `${roundForKey(otherAnchor.x)}-${roundForKey(dragAnchor.y)}`
          if (!crossSet.has(dragCrossKey)) {
            crossSet.add(dragCrossKey)
            crosses.push({ x: otherAnchor.x, y: dragAnchor.y })
          }

          const otherCrossKey = `${roundForKey(otherAnchor.x)}-${roundForKey(otherAnchor.y)}`
          if (!crossSet.has(otherCrossKey)) {
            crossSet.add(otherCrossKey)
            crosses.push({ x: otherAnchor.x, y: otherAnchor.y })
          }
        }

        // Check horizontal alignments (same y-coordinate)
        const yDiff = Math.abs(dragAnchor.y - otherAnchor.y)
        if (yDiff < tolerance) {
          horizontalMatches.push({
            y: otherAnchor.y,
            dragAnchorY: dragAnchor.y,
            dragAnchorX: dragAnchor.x,
            otherAnchorX: otherAnchor.x,
            diff: yDiff,
          })

          const dragCrossKey = `${roundForKey(dragAnchor.x)}-${roundForKey(otherAnchor.y)}`
          if (!crossSet.has(dragCrossKey)) {
            crossSet.add(dragCrossKey)
            crosses.push({ x: dragAnchor.x, y: otherAnchor.y })
          }

          const otherCrossKey = `${roundForKey(otherAnchor.x)}-${roundForKey(otherAnchor.y)}`
          if (!crossSet.has(otherCrossKey)) {
            crossSet.add(otherCrossKey)
            crosses.push({ x: otherAnchor.x, y: otherAnchor.y })
          }
        }
      }
    }
  }

  // Find best snap for X (smallest diff)
  let snapX: SnapInfo | null = null
  if (verticalMatches.length > 0) {
    const bestMatch = verticalMatches.reduce((best, current) =>
      current.diff < best.diff ? current : best
    )
    snapX = {
      targetPosition: bestMatch.x,
      dragAnchorPosition: bestMatch.dragAnchorX,
    }
  }

  // Find best snap for Y (smallest diff)
  let snapY: SnapInfo | null = null
  if (horizontalMatches.length > 0) {
    const bestMatch = horizontalMatches.reduce((best, current) =>
      current.diff < best.diff ? current : best
    )
    snapY = {
      targetPosition: bestMatch.y,
      dragAnchorPosition: bestMatch.dragAnchorY,
    }
  }

  // Group vertical matches by x-position
  const verticalGuideMap = new Map<number, { minY: number; maxY: number }>()
  for (const match of verticalMatches) {
    const key = roundForKey(match.x)
    const existing = verticalGuideMap.get(key)
    const minY = Math.min(match.dragAnchorY, match.otherAnchorY)
    const maxY = Math.max(match.dragAnchorY, match.otherAnchorY)

    if (existing) {
      existing.minY = Math.min(existing.minY, minY)
      existing.maxY = Math.max(existing.maxY, maxY)
    } else {
      verticalGuideMap.set(key, { minY, maxY })
    }
  }

  // Group horizontal matches by y-position
  const horizontalGuideMap = new Map<number, { minX: number; maxX: number }>()
  for (const match of horizontalMatches) {
    const key = roundForKey(match.y)
    const existing = horizontalGuideMap.get(key)
    const minX = Math.min(match.dragAnchorX, match.otherAnchorX)
    const maxX = Math.max(match.dragAnchorX, match.otherAnchorX)

    if (existing) {
      existing.minX = Math.min(existing.minX, minX)
      existing.maxX = Math.max(existing.maxX, maxX)
    } else {
      horizontalGuideMap.set(key, { minX, maxX })
    }
  }

  // Convert to guide lines
  const guides: AlignmentGuide[] = []

  for (const [xKey, range] of verticalGuideMap.entries()) {
    const match = verticalMatches.find((m) => roundForKey(m.x) === xKey)
    if (match) {
      guides.push({
        x1: match.x,
        y1: range.minY,
        x2: match.x,
        y2: range.maxY,
        type: "v",
      })
    }
  }

  for (const [yKey, range] of horizontalGuideMap.entries()) {
    const match = horizontalMatches.find((m) => roundForKey(m.y) === yKey)
    if (match) {
      guides.push({
        x1: range.minX,
        y1: match.y,
        x2: range.maxX,
        y2: match.y,
        type: "h",
      })
    }
  }

  return { guides, crosses, snapX, snapY }
}

export function detectSnapWithAnchors(
  editor: Editor,
  dragAnchors: AnchorPoint[],
  excludeShapeIds: ShapeId | ShapeId[],
  tolerance: number
): { snapX: SnapInfo | null; snapY: SnapInfo | null } {
  const shapes = editor.getCurrentPageShapes()
  if (shapes.length === 0) return { snapX: null, snapY: null }

  const excludeSet = new Set(
    Array.isArray(excludeShapeIds) ? excludeShapeIds : [excludeShapeIds]
  )

  const verticalMatches: VerticalMatch[] = []
  const horizontalMatches: HorizontalMatch[] = []

  // Create a shape resolver function for absolute coordinate calculation
  const getShapeById = (id: ShapeId) => editor.getShape(id)

  for (const shape of shapes) {
    if (excludeSet.has(shape.id)) continue

    // Use absolute rotated anchor points for accurate alignment with rotated shapes
    // This accounts for parent group transforms when shapes are inside groups
    let otherAnchors: AnchorPoint[]
    try {
      otherAnchors = getAbsoluteRotatedAnchorPoints(shape, getShapeById)
    } catch {
      continue
    }

    for (const dragAnchor of dragAnchors) {
      for (const otherAnchor of otherAnchors) {
        const xDiff = Math.abs(dragAnchor.x - otherAnchor.x)
        if (xDiff < tolerance) {
          verticalMatches.push({
            x: otherAnchor.x,
            dragAnchorX: dragAnchor.x,
            dragAnchorY: dragAnchor.y,
            otherAnchorY: otherAnchor.y,
            diff: xDiff,
          })
        }

        const yDiff = Math.abs(dragAnchor.y - otherAnchor.y)
        if (yDiff < tolerance) {
          horizontalMatches.push({
            y: otherAnchor.y,
            dragAnchorY: dragAnchor.y,
            dragAnchorX: dragAnchor.x,
            otherAnchorX: otherAnchor.x,
            diff: yDiff,
          })
        }
      }
    }
  }

  let snapX: SnapInfo | null = null
  if (verticalMatches.length > 0) {
    const bestMatch = verticalMatches.reduce((best, current) =>
      current.diff < best.diff ? current : best
    )
    snapX = {
      targetPosition: bestMatch.x,
      dragAnchorPosition: bestMatch.dragAnchorX,
    }
  }

  let snapY: SnapInfo | null = null
  if (horizontalMatches.length > 0) {
    const bestMatch = horizontalMatches.reduce((best, current) =>
      current.diff < best.diff ? current : best
    )
    snapY = {
      targetPosition: bestMatch.y,
      dragAnchorPosition: bestMatch.dragAnchorY,
    }
  }

  return { snapX, snapY }
}

// Shape pool for reusing Konva Line shapes
