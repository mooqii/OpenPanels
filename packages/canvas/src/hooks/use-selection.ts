import type Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import React from "react"
import type { Editor } from "../editor"
import type { ShapeId } from "../types/ids"
import { testShapeIntersection } from "../utils/hit-testing"

export function useSelection(editor: Editor) {
  const startPositionRef = React.useRef<{ x: number; y: number }>(null)

  const [isSelecting, setIsSelecting] = React.useState(false)

  const rectRef = React.useRef<Konva.Rect>(null)

  const selectShapeById = React.useCallback(
    (
      shapeId: ShapeId,
      options: { multi?: boolean; preserveExistingSelection?: boolean } = {}
    ) => {
      // Close opened group if selecting a shape outside of it
      const openedGroupId = editor.getOpenedGroupId()
      if (openedGroupId && !editor.isShapeInOpenedGroup(shapeId)) {
        editor.closeGroup()
      }

      // If shape is inside an opened group, select it directly
      // Otherwise, find top-level ancestor (the group or the shape itself if not grouped)
      const selectableId = editor.isShapeInOpenedGroup(shapeId)
        ? shapeId
        : (editor.getTopLevelAncestor(shapeId) ?? shapeId)

      const currentSelection = editor
        .getSelectedShapes()
        .map((shape) => shape.id)
      if (
        options.preserveExistingSelection &&
        currentSelection.includes(selectableId)
      ) {
        return
      }

      if (options.multi) {
        if (currentSelection.includes(selectableId)) {
          editor.setSelectedShapes(
            currentSelection.filter((id) => id !== selectableId)
          )
        } else {
          editor.setSelectedShapes([...currentSelection, selectableId])
        }
      } else {
        editor.setSelectedShapes([selectableId])
      }
    },
    [editor]
  )

  const handleMouseDown = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      // Do nothing if we mousedown on any shape
      if (e.target !== e.target.getStage()) {
        return
      }

      // Close opened group when clicking outside (on the stage)
      const openedGroupId = editor.getOpenedGroupId()
      if (openedGroupId) {
        editor.closeGroup()
      }

      const pos = e.target.getStage().getRelativePointerPosition()
      if (!pos) return

      startPositionRef.current = {
        x: pos.x,
        y: pos.y,
      }
    },
    [editor]
  )

  const handleMouseMove = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      // Do nothing if we didn't start selection
      if (!startPositionRef.current) return

      if (!isSelecting) {
        setIsSelecting(true)
      }

      if (!rectRef.current) return

      const pos = e.target.getStage()?.getRelativePointerPosition()
      if (!pos) return

      const startPos = startPositionRef.current

      const height = Math.abs(startPos.y - pos.y)
      const width = Math.abs(startPos.x - pos.x)
      const x = Math.min(startPos.x, pos.x)
      const y = Math.min(startPos.y, pos.y)

      rectRef.current.setPosition({ x, y })
      rectRef.current.setSize({ width, height })
    },
    [isSelecting]
  )

  const handleMouseUp = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      setIsSelecting(false)

      const stage = e.target.getStage()
      const pos = stage?.getRelativePointerPosition()
      if (!(pos && stage)) return

      if (!startPositionRef.current) return

      const startPos = startPositionRef.current

      startPositionRef.current = null

      // Selection box in canvas coordinates (getRelativePointerPosition already accounts for pan/zoom)
      const selBox = {
        x: Math.min(startPos.x, pos.x),
        y: Math.min(startPos.y, pos.y),
        width: Math.abs(startPos.x - pos.x),
        height: Math.abs(startPos.y - pos.y),
      }

      // Get shapes that intersect with the selection box using geometry-aware hit testing
      // Then map to top-level ancestors to handle grouped shapes
      const intersectingShapes = editor
        .getCurrentPageShapes()
        .filter((shape) => {
          // Use accurate geometry-based hit testing (handles rotation, ellipses, paths, etc.)
          // Pass editor for connector shape hit testing
          const result = testShapeIntersection(shape, selBox, editor)
          return result.intersects
        })

      // Map to top-level ancestors and deduplicate
      // But skip this mapping for shapes inside an opened group
      const topLevelIds = new Set<ShapeId>()
      for (const shape of intersectingShapes) {
        // If shape is inside an opened group, select it directly
        if (editor.isShapeInOpenedGroup(shape.id)) {
          topLevelIds.add(shape.id)
        } else {
          // Otherwise, map to top-level ancestor
          const topLevelId = editor.getTopLevelAncestor(shape.id) ?? shape.id
          topLevelIds.add(topLevelId)
        }
      }

      editor.setSelectedShapes(Array.from(topLevelIds))
    },
    [editor]
  )

  const handleShapeClick = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      e.cancelBubble = true
      const clickedId = e.target.id() as ShapeId

      const multi = e.evt.shiftKey || e.evt.ctrlKey || e.evt.metaKey

      selectShapeById(clickedId, { multi })
    },
    [selectShapeById]
  )

  const handleShapeDragStart = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      const draggedId = e.target.id() as ShapeId | undefined
      if (!draggedId) return

      selectShapeById(draggedId, { preserveExistingSelection: true })
    },
    [selectShapeById]
  )

  return {
    handleMouseMove,
    handleMouseDown,
    handleMouseUp,
    handleShapeClick,
    handleShapeDragStart,
    isSelecting,
    rectRef,
  }
}
