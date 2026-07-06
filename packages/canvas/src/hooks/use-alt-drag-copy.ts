import type { KonvaEventObject } from "konva/lib/Node"
import { useCallback, useRef } from "react"
import type { Editor } from "../editor"
import type { ShapeId } from "../types/ids"
import { cloneShapesForPaste } from "../utils/shape-actions"

interface CopyState {
  /** Original shapes (these will become the "copies" at drag destination) */
  originalShapeIds: ShapeId[]
}

/**
 * Hook for Alt+Drag to copy shapes.
 *
 * When the user holds Alt while dragging:
 * - Clones are created at the original position (they stay in place)
 * - The original shapes are dragged to the new position (they become the copies)
 *
 * This is simpler than trying to reset positions - we just swap roles:
 * - Clone = stays at original position = becomes the "original"
 * - Original = gets dragged = becomes the "copy"
 */
export function useAltDragCopy(editor: Editor) {
  const copyStateRef = useRef<CopyState | null>(null)

  /**
   * Handle drag start - if Alt is pressed, create clones at original positions
   */
  const handleDragStart = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      // Only activate copy mode if Alt is pressed
      if (!e.evt.altKey) {
        copyStateRef.current = null
        return
      }

      const draggedId = e.target.id() as ShapeId
      const draggedShape = editor.getShape(draggedId)
      if (!draggedShape) {
        return
      }

      // Get shapes to clone - either selected shapes or the dragged shape
      const selectedShapes = editor.getSelectedShapes()

      // If draggedShape is in selectedShapes, clone all selectedShapes.
      // otherwise just clone the draggedShape
      const shapesToClone = selectedShapes.find(
        (shape) => shape.id === draggedId
      )
        ? selectedShapes
        : [draggedShape]

      if (shapesToClone.length === 0) {
        copyStateRef.current = null
        return
      }

      // Create clones at original positions with original indices (they stay in place)
      // The originals will be moved to higher indices (on top) since they become the "copies"
      const idFactory = () => `shape:${crypto.randomUUID()}` as ShapeId
      const clones = cloneShapesForPaste(shapesToClone, {
        idFactory,
        offset: { x: 0, y: 0 },
        baseIndex: 0, // Will be overwritten below
        parentId: editor.getCurrentPageId() ?? undefined,
      })

      // Get max index for placing originals on top
      const currentShapes = editor.getCurrentPageShapes()
      const maxIndex = currentShapes.length
        ? Math.max(...currentShapes.map((s) => s.index || 0))
        : 0

      // Add clones with original indices, move originals to top
      editor.run(() => {
        for (let i = 0; i < clones.length; i++) {
          const clone = clones[i]
          const original = shapesToClone[i]

          // Clone gets the original's index (stays in place, below)
          clone.index = original.index || 0
          editor.createShape(clone)

          // Original gets a higher index (on top, being dragged)
          editor.updateShape(original.id, { index: maxIndex + 1 + i })
        }
      })

      // Store state - the originals will be dragged and become the "copies"
      copyStateRef.current = {
        originalShapeIds: shapesToClone.map((s) => s.id),
      }
    },
    [editor]
  )

  /**
   * Handle drag move - nothing special needed, Konva drags the originals
   */
  const handleDragMove = useCallback((_e: KonvaEventObject<PointerEvent>) => {
    // Konva handles dragging the original shapes
    // The clones stay at the original positions
  }, [])

  /**
   * Handle drag end - commit the drag (originals are now at new position)
   *
   * @returns true if we handled the drag (copy mode was active), false otherwise
   */
  const handleDragEnd = useCallback(
    (_e: KonvaEventObject<PointerEvent>): boolean => {
      const copyState = copyStateRef.current
      if (!copyState) {
        return false
      }

      // Clear copy state
      copyStateRef.current = null

      // Return false to let the normal drag handler update the original shapes' positions
      // The originals are now at the drag destination (they are the "copies")
      // The clones are at the original position (they are the "originals")
      return false
    },
    []
  )

  return {
    handleDragStart,
    handleDragMove,
    handleDragEnd,
  }
}
