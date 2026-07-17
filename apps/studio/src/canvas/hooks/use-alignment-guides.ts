import Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import React from "react"
import { ALIGNMENT_GUIDE_COLOR } from "../constants"
import type { Editor } from "../editor"
import type { Box, Transformer } from "../shapes/Transformer"
import type { ShapeId } from "../types/ids"
import { stageToCanvas } from "../utils/coordinates"

import {
  ALIGNMENT_TOLERANCE,
  type AlignmentGuide,
  type AnchorCross,
  type AnchorPoint,
  CROSS_POOL_SIZE,
  CROSS_SIZE,
  collectExcludedShapeIds,
  detectAlignments,
  detectSnapWithAnchors,
  GUIDE_POOL_SIZE,
  getCombinedDraggedAnchors,
  getDraggedShapeRotatedAnchors,
  SNAP_BREAK_THRESHOLD,
  SNAP_THRESHOLD,
  type SnapState,
  type StageTransform,
} from "./alignment-detection"

export type {
  AlignmentGuide,
  AlignmentResult,
  AnchorCross,
} from "./alignment-detection"

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
