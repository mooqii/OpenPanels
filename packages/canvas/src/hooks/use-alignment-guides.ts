import Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import React from "react"
import { ALIGNMENT_GUIDE_COLOR } from "../constants"
import type { Editor } from "../editor"
import type { Box, Transformer } from "../shapes/Transformer"
import type { ShapeId } from "../types/ids"
import {
  getAbsoluteRotatedAnchorPoints,
  stageToCanvas,
} from "../utils/coordinates"

const ALIGNMENT_TOLERANCE = 5 // pixels - distance to detect alignment
const SNAP_THRESHOLD = 5 // pixels - distance to snap to alignment
const SNAP_BREAK_THRESHOLD = 8 // pixels - distance to break free from snap
const CROSS_SIZE = 3 // half-size of the cross mark

// Initial pool sizes
const GUIDE_POOL_SIZE = 10
const CROSS_POOL_SIZE = 50 // Each cross needs 2 lines, so 25 crosses max

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

interface SnapInfo {
  dragAnchorPosition: number // The current position of the matched anchor on dragged shape
  targetPosition: number // The position to snap to (anchor on other shape)
}

interface StageTransform {
  scaleX: number
  scaleY: number
  x: number
  y: number
}

interface SnapState {
  x: { snappedNodeX: number } | null
  y: { snappedNodeY: number } | null
}

function roundForKey(value: number): number {
  return Math.round(value)
}

interface AnchorPoint {
  x: number
  y: number
}

interface VerticalMatch {
  diff: number
  dragAnchorX: number // Current x position of matched anchor on dragged shape
  dragAnchorY: number
  otherAnchorY: number
  x: number // Target x position (from other shape)
}

interface HorizontalMatch {
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
function getDraggedShapeRotatedAnchors(
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
function getGroupShapeAnchorsFromKonva(node: Konva.Node): AnchorPoint[] {
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
function getCombinedDraggedAnchors(
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
function collectExcludedShapeIds(
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

function detectAlignments(
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

function detectSnapWithAnchors(
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
interface ShapePool {
  crossLines: Konva.Line[]
  guideLines: Konva.Line[]
}

function createLineShape(): Konva.Line {
  return new Konva.Line({
    stroke: ALIGNMENT_GUIDE_COLOR,
    strokeWidth: 1,
    strokeScaleEnabled: false,
    visible: false,
    listening: false,
  })
}

function initializeShapePool(group: Konva.Group): ShapePool {
  const guideLines: Konva.Line[] = []
  const crossLines: Konva.Line[] = []

  // Pre-create guide lines
  for (let i = 0; i < GUIDE_POOL_SIZE; i++) {
    const line = createLineShape()
    guideLines.push(line)
    group.add(line)
  }

  // Pre-create cross lines (2 lines per cross)
  for (let i = 0; i < CROSS_POOL_SIZE; i++) {
    const line = createLineShape()
    crossLines.push(line)
    group.add(line)
  }

  return { guideLines, crossLines }
}

export function useAlignmentGuides({
  editor,
  guidesGroupRef,
}: {
  editor: Editor
  guidesGroupRef: React.RefObject<Konva.Group | null>
}) {
  const isDraggingRef = React.useRef(false)
  const draggedShapeIdRef = React.useRef<ShapeId | null>(null)
  const snapStateRef = React.useRef<SnapState>({ x: null, y: null })
  const shapePoolRef = React.useRef<ShapePool | null>(null)
  const currentScaleRef = React.useRef(1)

  // Transform-specific state
  const isTransformingRef = React.useRef(false)
  const transformingShapeIdsRef = React.useRef<ShapeId[]>([])
  const transformSnapStateRef = React.useRef<SnapState>({ x: null, y: null })
  const activeAnchorRef = React.useRef<string | null>(null)

  // Multi-shape drag state - tracks all shapes being dragged together
  const draggingShapeIdsRef = React.useRef<ShapeId[]>([])
  // Cached excluded shape IDs for drag operations (includes group descendants)
  const dragExcludeShapeIdsRef = React.useRef<ShapeId[]>([])
  // Cached excluded shape IDs for transform operations (includes group descendants)
  const transformExcludeShapeIdsRef = React.useRef<ShapeId[]>([])

  // Initialize shape pool when group is available
  const ensureShapePool = React.useCallback(() => {
    const group = guidesGroupRef.current
    if (!group || shapePoolRef.current) return

    shapePoolRef.current = initializeShapePool(group)
  }, [guidesGroupRef])

  // Hide all guide shapes imperatively
  const clearGuides = React.useCallback(() => {
    const pool = shapePoolRef.current
    if (!pool) return

    for (const line of pool.guideLines) {
      line.visible(false)
    }
    for (const line of pool.crossLines) {
      line.visible(false)
    }

    // Batch draw for performance
    const group = guidesGroupRef.current
    if (group) {
      group.getLayer()?.batchDraw()
    }
  }, [guidesGroupRef])

  // Draw guides imperatively using the shape pool
  const drawGuides = React.useCallback(
    (guides: AlignmentGuide[], crosses: AnchorCross[], scale: number) => {
      const pool = shapePoolRef.current
      const group = guidesGroupRef.current
      if (!(pool && group)) return

      const crossSize = CROSS_SIZE / scale

      // Update guide lines
      for (let i = 0; i < pool.guideLines.length; i++) {
        const line = pool.guideLines[i]
        if (i < guides.length) {
          const guide = guides[i]
          line.points([guide.x1, guide.y1, guide.x2, guide.y2])
          line.visible(true)
        } else {
          line.visible(false)
        }
      }

      // If we need more guide lines than pooled, create them dynamically
      if (guides.length > pool.guideLines.length) {
        for (let i = pool.guideLines.length; i < guides.length; i++) {
          const guide = guides[i]
          const line = createLineShape()
          line.points([guide.x1, guide.y1, guide.x2, guide.y2])
          line.visible(true)
          pool.guideLines.push(line)
          group.add(line)
        }
      }

      // Update cross lines (2 lines per cross)
      const crossLineCount = crosses.length * 2
      for (let i = 0; i < pool.crossLines.length; i++) {
        const line = pool.crossLines[i]
        const crossIndex = Math.floor(i / 2)
        const isFirstLine = i % 2 === 0

        if (crossIndex < crosses.length) {
          const cross = crosses[crossIndex]
          if (isFirstLine) {
            // Diagonal line: top-left to bottom-right
            line.points([
              cross.x - crossSize,
              cross.y - crossSize,
              cross.x + crossSize,
              cross.y + crossSize,
            ])
          } else {
            // Diagonal line: top-right to bottom-left
            line.points([
              cross.x + crossSize,
              cross.y - crossSize,
              cross.x - crossSize,
              cross.y + crossSize,
            ])
          }
          line.visible(true)
        } else {
          line.visible(false)
        }
      }

      // If we need more cross lines than pooled, create them dynamically
      if (crossLineCount > pool.crossLines.length) {
        for (let i = pool.crossLines.length; i < crossLineCount; i++) {
          const crossIndex = Math.floor(i / 2)
          const isFirstLine = i % 2 === 0
          const cross = crosses[crossIndex]
          const line = createLineShape()

          if (isFirstLine) {
            line.points([
              cross.x - crossSize,
              cross.y - crossSize,
              cross.x + crossSize,
              cross.y + crossSize,
            ])
          } else {
            line.points([
              cross.x + crossSize,
              cross.y - crossSize,
              cross.x - crossSize,
              cross.y + crossSize,
            ])
          }
          line.visible(true)
          pool.crossLines.push(line)
          group.add(line)
        }
      }

      // Batch draw for performance
      group.getLayer()?.batchDraw()
    },
    [guidesGroupRef]
  )

  const handleDragStart = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      ensureShapePool()
      const shapeId = e.target.id() as ShapeId
      isDraggingRef.current = true
      draggedShapeIdRef.current = shapeId
      snapStateRef.current = { x: null, y: null }

      // Check if this is a multi-selection drag
      const selectedShapes = editor.getSelectedShapes()
      if (selectedShapes.length > 1) {
        const selectedIds = selectedShapes.map((s) => s.id)
        // Only track multi-selection if the dragged shape is part of the selection
        if (selectedIds.includes(shapeId)) {
          draggingShapeIdsRef.current = selectedIds
        } else {
          draggingShapeIdsRef.current = []
        }
      } else {
        draggingShapeIdsRef.current = []
      }

      // Cache excluded shape IDs (including group descendants) for drag operations
      const draggedShapeIds =
        draggingShapeIdsRef.current.length > 0
          ? draggingShapeIdsRef.current
          : [shapeId]
      dragExcludeShapeIdsRef.current = collectExcludedShapeIds(
        editor,
        draggedShapeIds
      )

      clearGuides()
    },
    [editor, ensureShapePool, clearGuides]
  )

  const handleDragMove = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      if (!isDraggingRef.current) {
        clearGuides()
        return
      }

      const node = e.target as Konva.Shape
      const shapeId = node.id() as ShapeId

      if (draggedShapeIdRef.current !== shapeId) {
        clearGuides()
        return
      }

      const stage = editor.stage
      if (!stage) {
        clearGuides()
        return
      }

      const stageTransform: StageTransform = {
        x: stage.x(),
        y: stage.y(),
        scaleX: stage.scaleX(),
        scaleY: stage.scaleY(),
      }

      // Store current scale for cross size calculation
      currentScaleRef.current = stageTransform.scaleX

      // Determine if this is a multi-shape drag (needed for snap sync)
      const isMultiDrag = draggingShapeIdsRef.current.length > 1
      // Use cached excluded shape IDs (calculated in handleDragStart)
      const excludeShapeIds = dragExcludeShapeIdsRef.current

      // Get nodes for multi-drag (collect once, reuse for bounds calculation and snap sync)
      const draggingNodes = isMultiDrag
        ? draggingShapeIdsRef.current
            .map((id) => editor.getShapeNode(id))
            .filter((n): n is Konva.Node => n !== undefined)
        : null

      // Get current node position
      const currentX = node.x()
      const currentY = node.y()

      // Check if we need to break from current snap
      const snapState = snapStateRef.current

      if (snapState.x) {
        const distFromSnap = Math.abs(currentX - snapState.x.snappedNodeX)
        if (distFromSnap > SNAP_BREAK_THRESHOLD) {
          // Break free from X snap
          snapStateRef.current.x = null
        } else {
          // Keep snapped - apply adjustment delta to all dragging nodes
          const adjustmentDelta = snapState.x.snappedNodeX - currentX
          node.x(snapState.x.snappedNodeX)
          if (isMultiDrag && draggingNodes) {
            for (const otherNode of draggingNodes) {
              if (otherNode === node) continue
              otherNode.x(otherNode.x() + adjustmentDelta)
            }
          }
        }
      }

      if (snapState.y) {
        const distFromSnap = Math.abs(currentY - snapState.y.snappedNodeY)
        if (distFromSnap > SNAP_BREAK_THRESHOLD) {
          // Break free from Y snap
          snapStateRef.current.y = null
        } else {
          // Keep snapped - apply adjustment delta to all dragging nodes
          const adjustmentDelta = snapState.y.snappedNodeY - currentY
          node.y(snapState.y.snappedNodeY)
          if (isMultiDrag && draggingNodes) {
            for (const otherNode of draggingNodes) {
              if (otherNode === node) continue
              otherNode.y(otherNode.y() + adjustmentDelta)
            }
          }
        }
      }

      // Get rotated anchor points for dragged shapes
      let dragAnchors: AnchorPoint[]
      try {
        if (isMultiDrag && draggingNodes) {
          dragAnchors = getCombinedDraggedAnchors(
            editor,
            draggingShapeIdsRef.current,
            draggingNodes
          )
        } else {
          dragAnchors = getDraggedShapeRotatedAnchors(editor, shapeId, node)
        }
      } catch {
        clearGuides()
        return
      }

      if (dragAnchors.length === 0) {
        clearGuides()
        return
      }

      // Detect alignments (excluding all shapes being dragged)
      const result = detectAlignments(
        editor,
        dragAnchors,
        excludeShapeIds,
        ALIGNMENT_TOLERANCE
      )

      // Apply new snaps if not already snapped
      // The snap delta is: how much to move the node so that dragAnchor lands on targetPosition
      // newNodeX = currentNodeX + (targetPosition - dragAnchorPosition)
      if (!snapStateRef.current.x && result.snapX) {
        const snapDelta =
          result.snapX.targetPosition - result.snapX.dragAnchorPosition
        const newX = node.x() + snapDelta
        if (Math.abs(snapDelta) < SNAP_THRESHOLD) {
          node.x(newX)
          // Apply the same snap delta to all other dragging nodes
          if (isMultiDrag && draggingNodes) {
            for (const otherNode of draggingNodes) {
              if (otherNode === node) continue
              otherNode.x(otherNode.x() + snapDelta)
            }
          }
          snapStateRef.current.x = {
            snappedNodeX: newX,
          }
        }
      }

      if (!snapStateRef.current.y && result.snapY) {
        const snapDelta =
          result.snapY.targetPosition - result.snapY.dragAnchorPosition
        const newY = node.y() + snapDelta
        if (Math.abs(snapDelta) < SNAP_THRESHOLD) {
          node.y(newY)
          // Apply the same snap delta to all other dragging nodes
          if (isMultiDrag && draggingNodes) {
            for (const otherNode of draggingNodes) {
              if (otherNode === node) continue
              otherNode.y(otherNode.y() + snapDelta)
            }
          }
          snapStateRef.current.y = {
            snappedNodeY: newY,
          }
        }
      }

      // Re-detect alignments after snapping for accurate guide display
      if (snapStateRef.current.x || snapStateRef.current.y) {
        try {
          // Recalculate rotated anchor points after snapping
          if (isMultiDrag && draggingNodes) {
            dragAnchors = getCombinedDraggedAnchors(
              editor,
              draggingShapeIdsRef.current,
              draggingNodes
            )
          } else {
            dragAnchors = getDraggedShapeRotatedAnchors(editor, shapeId, node)
          }
        } catch {
          clearGuides()
          return
        }

        const finalResult = detectAlignments(
          editor,
          dragAnchors,
          excludeShapeIds,
          ALIGNMENT_TOLERANCE
        )

        drawGuides(
          finalResult.guides,
          finalResult.crosses,
          stageTransform.scaleX
        )
      } else {
        drawGuides(result.guides, result.crosses, stageTransform.scaleX)
      }
    },
    [editor, editor.stage, clearGuides, drawGuides]
  )

  const handleDragEnd = React.useCallback(() => {
    isDraggingRef.current = false
    draggedShapeIdRef.current = null
    snapStateRef.current = { x: null, y: null }
    draggingShapeIdsRef.current = []
    dragExcludeShapeIdsRef.current = []
    clearGuides()
  }, [clearGuides])

  // ============================================================================
  // Transform handlers (for resize/rotate operations)
  // ============================================================================

  const handleTransformStart = React.useCallback(
    (transformerRef: React.RefObject<Transformer | null>) => {
      ensureShapePool()
      const transformer = transformerRef.current
      if (!transformer) return

      const nodes = transformer.getNodes()
      const shapeIds = nodes.map((node) => node.id() as ShapeId)

      isTransformingRef.current = true
      transformingShapeIdsRef.current = shapeIds
      transformSnapStateRef.current = { x: null, y: null }
      activeAnchorRef.current = transformer.getActiveAnchor()

      // Cache excluded shape IDs (including group descendants) for transform operations
      transformExcludeShapeIdsRef.current = collectExcludedShapeIds(
        editor,
        shapeIds
      )

      clearGuides()
    },
    [editor, ensureShapePool, clearGuides]
  )

  // This is called AFTER transform is applied - used for drawing guides only
  const handleTransformMove = React.useCallback(
    (transformerRef: React.RefObject<Transformer | null>) => {
      if (!isTransformingRef.current) {
        clearGuides()
        return
      }

      const transformer = transformerRef.current
      if (!transformer) {
        clearGuides()
        return
      }

      // Skip alignment detection when rotating
      const activeAnchor = transformer.getActiveAnchor()
      if (activeAnchor?.startsWith("rotater-")) {
        clearGuides()
        return
      }

      const nodes = transformer.getNodes()
      if (nodes.length === 0) {
        clearGuides()
        return
      }

      const stage = editor.stage
      if (!stage) {
        clearGuides()
        return
      }

      const stageTransform: StageTransform = {
        x: stage.x(),
        y: stage.y(),
        scaleX: stage.scaleX(),
        scaleY: stage.scaleY(),
      }

      currentScaleRef.current = stageTransform.scaleX

      // Get rotated anchor points for all transforming shapes
      const transformAnchors = getCombinedDraggedAnchors(
        editor,
        transformingShapeIdsRef.current,
        nodes
      )

      if (transformAnchors.length === 0) {
        clearGuides()
        return
      }

      // Use cached excluded shape IDs (calculated in handleTransformStart)
      const excludeShapeIds = transformExcludeShapeIdsRef.current

      // Detect alignments excluding all transforming shapes
      const result = detectAlignments(
        editor,
        transformAnchors,
        excludeShapeIds,
        ALIGNMENT_TOLERANCE
      )

      // Draw guides (snapping is handled by boundBoxFunc)
      drawGuides(result.guides, result.crosses, stageTransform.scaleX)
    },
    [editor, clearGuides, drawGuides]
  )

  const handleTransformEnd = React.useCallback(() => {
    isTransformingRef.current = false
    transformingShapeIdsRef.current = []
    transformSnapStateRef.current = { x: null, y: null }
    activeAnchorRef.current = null
    transformExcludeShapeIdsRef.current = []
    clearGuides()
  }, [clearGuides])

  // Creates a boundBoxFunc for snapping during transform
  // This function is called BEFORE the transform is applied, allowing us to snap
  const createBoundBoxFunc = React.useCallback(
    () =>
      makeBoundBoxFunc(
        editor,
        isTransformingRef,
        activeAnchorRef,
        transformExcludeShapeIdsRef
      ),
    [editor]
  )

  return {
    handleDragStart,
    handleDragMove,
    handleDragEnd,
    handleTransformStart,
    handleTransformMove,
    handleTransformEnd,
    createBoundBoxFunc,
  }
}

/**
 * Determines which edges are fixed based on the active anchor being dragged.
 * This is deterministic and doesn't rely on comparing box positions.
 *
 * Anchor to fixed edge mapping:
 * - "top-left": right is fixed, bottom is fixed
 * - "top-center": bottom is fixed (left/right both move for centered resize)
 * - "top-right": left is fixed, bottom is fixed
 * - "middle-left": right is fixed
 * - "middle-right": left is fixed
 * - "bottom-left": right is fixed, top is fixed
 * - "bottom-center": top is fixed (left/right both move for centered resize)
 * - "bottom-right": left is fixed, top is fixed
 * - "rotater-*": all edges move (rotation mode, snapping disabled)
 */
function getFixedEdgesFromAnchor(anchorName: string | null): {
  leftFixed: boolean
  rightFixed: boolean
  topFixed: boolean
  bottomFixed: boolean
  isRotating: boolean
} {
  if (!anchorName) {
    // No anchor info - fall back to allowing all edges to move
    return {
      leftFixed: false,
      rightFixed: false,
      topFixed: false,
      bottomFixed: false,
      isRotating: false,
    }
  }

  // Rotater anchors - disable snapping during rotation
  if (anchorName.startsWith("rotater-")) {
    return {
      leftFixed: false,
      rightFixed: false,
      topFixed: false,
      bottomFixed: false,
      isRotating: true,
    }
  }

  // Anchors that keep the LEFT edge fixed (dragging from right side)
  const leftFixed =
    anchorName === "top-right" ||
    anchorName === "middle-right" ||
    anchorName === "bottom-right"

  // Anchors that keep the RIGHT edge fixed (dragging from left side)
  const rightFixed =
    anchorName === "top-left" ||
    anchorName === "middle-left" ||
    anchorName === "bottom-left"

  // Anchors that keep the TOP edge fixed (dragging from bottom side)
  const topFixed =
    anchorName === "bottom-left" ||
    anchorName === "bottom-center" ||
    anchorName === "bottom-right"

  // Anchors that keep the BOTTOM edge fixed (dragging from top side)
  const bottomFixed =
    anchorName === "top-left" ||
    anchorName === "top-center" ||
    anchorName === "top-right"

  return { leftFixed, rightFixed, topFixed, bottomFixed, isRotating: false }
}

function makeBoundBoxFunc(
  editor: Editor,
  isTransformingRef: React.RefObject<boolean>,
  activeAnchorRef: React.RefObject<string | null>,
  transformExcludeShapeIdsRef: React.RefObject<ShapeId[]>
) {
  return (oldBox: Box, newBox: Box): Box => {
    if (!isTransformingRef.current) return newBox

    // Use anchor-based edge detection for reliable fixed edge determination
    const { leftFixed, rightFixed, topFixed, bottomFixed, isRotating } =
      getFixedEdgesFromAnchor(activeAnchorRef.current)

    // Skip snapping when rotating (either via rotation anchor or rotation value changes)
    if (isRotating || oldBox.rotation !== newBox.rotation) return newBox

    const stage = editor.stage
    if (!stage) return newBox

    const stageTransform: StageTransform = {
      x: stage.x(),
      y: stage.y(),
      scaleX: stage.scaleX(),
      scaleY: stage.scaleY(),
    }

    // Convert box corners to canvas coordinates for alignment detection
    const topLeft = stageToCanvas(newBox.x, newBox.y, stageTransform)
    const bottomRight = stageToCanvas(
      newBox.x + newBox.width,
      newBox.y + newBox.height,
      stageTransform
    )

    let snappedX = newBox.x
    let snappedY = newBox.y
    let snappedWidth = newBox.width
    let snappedHeight = newBox.height
    let hasSnap = false

    const leftCanvas = topLeft.x
    const rightCanvas = bottomRight.x
    const topCanvas = topLeft.y
    const bottomCanvas = bottomRight.y
    const centerCanvasX = (leftCanvas + rightCanvas) / 2
    const centerCanvasY = (topCanvas + bottomCanvas) / 2

    // Determine X anchors based on which horizontal edge is fixed
    // Only snap to the moving edge's anchors to avoid center interference
    const xAnchors =
      leftFixed && !rightFixed
        ? [
            // Left is fixed, right edge is moving - only use right edge anchors
            { x: rightCanvas, y: topCanvas },
            { x: rightCanvas, y: centerCanvasY },
            { x: rightCanvas, y: bottomCanvas },
          ]
        : rightFixed && !leftFixed
          ? [
              // Right is fixed, left edge is moving - only use left edge anchors
              { x: leftCanvas, y: topCanvas },
              { x: leftCanvas, y: centerCanvasY },
              { x: leftCanvas, y: bottomCanvas },
            ]
          : [
              // Both edges moving (centered scaling) - use all X anchors
              { x: leftCanvas, y: centerCanvasY },
              { x: rightCanvas, y: centerCanvasY },
              { x: centerCanvasX, y: centerCanvasY },
            ]

    // Determine Y anchors based on which vertical edge is fixed
    const yAnchors =
      topFixed && !bottomFixed
        ? [
            // Top is fixed, bottom edge is moving - only use bottom edge anchors
            { x: leftCanvas, y: bottomCanvas },
            { x: centerCanvasX, y: bottomCanvas },
            { x: rightCanvas, y: bottomCanvas },
          ]
        : bottomFixed && !topFixed
          ? [
              // Bottom is fixed, top edge is moving - only use top edge anchors
              { x: leftCanvas, y: topCanvas },
              { x: centerCanvasX, y: topCanvas },
              { x: rightCanvas, y: topCanvas },
            ]
          : [
              // Both edges moving (centered scaling) - use all Y anchors
              { x: centerCanvasX, y: topCanvas },
              { x: centerCanvasX, y: bottomCanvas },
              { x: centerCanvasX, y: centerCanvasY },
            ]

    // Use cached excluded shape IDs (calculated in handleTransformStart)
    const excludeShapeIds = transformExcludeShapeIdsRef.current

    const snapXResult = detectSnapWithAnchors(
      editor,
      xAnchors,
      excludeShapeIds,
      ALIGNMENT_TOLERANCE
    )
    const snapYResult = detectSnapWithAnchors(
      editor,
      yAnchors,
      excludeShapeIds,
      ALIGNMENT_TOLERANCE
    )

    // Apply X snap if within threshold
    if (snapXResult.snapX) {
      const deltaCanvas =
        snapXResult.snapX.targetPosition - snapXResult.snapX.dragAnchorPosition
      if (Math.abs(deltaCanvas) < SNAP_THRESHOLD) {
        const delta = deltaCanvas * stageTransform.scaleX
        if (leftFixed && !rightFixed) {
          // Left edge is fixed: only adjust width, never move x
          snappedWidth = newBox.width + delta
          hasSnap = true
        } else if (rightFixed && !leftFixed) {
          // Right edge is fixed: move x and adjust width to keep right edge position
          snappedX = newBox.x + delta
          snappedWidth = newBox.width - delta
          hasSnap = true
        } else {
          // Both edges moving (centered scaling): adjust both x and width symmetrically
          snappedX = newBox.x + delta / 2
          snappedWidth = newBox.width + delta
          hasSnap = true
        }
      }
    }

    // Apply Y snap if within threshold
    if (snapYResult.snapY) {
      const deltaCanvas =
        snapYResult.snapY.targetPosition - snapYResult.snapY.dragAnchorPosition
      if (Math.abs(deltaCanvas) < SNAP_THRESHOLD) {
        const delta = deltaCanvas * stageTransform.scaleY
        if (topFixed && !bottomFixed) {
          // Top edge is fixed: only adjust height, never move y
          snappedHeight = newBox.height + delta
          hasSnap = true
        } else if (bottomFixed && !topFixed) {
          // Bottom edge is fixed: move y and adjust height to keep bottom edge position
          snappedY = newBox.y + delta
          snappedHeight = newBox.height - delta
          hasSnap = true
        } else {
          // Both edges moving (centered scaling): adjust both y and height symmetrically
          snappedY = newBox.y + delta / 2
          snappedHeight = newBox.height + delta
          hasSnap = true
        }
      }
    }

    // Return snapped box if any snap was applied
    if (hasSnap) {
      return {
        ...newBox,
        x: snappedX,
        y: snappedY,
        width: snappedWidth,
        height: snappedHeight,
      }
    }

    return newBox
  }
}
