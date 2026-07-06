import type Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import { useCallback, useRef, useState } from "react"
import type { Editor } from "../editor"
import type { ShapeId } from "../types/ids"
import type { PathPoint, PathPointType, PathShape } from "../types/shapes"

interface PathEditState {
  /** Index of the point being manipulated */
  dragPointIndex: number | null
  /** Type of element being dragged */
  dragType: "anchor" | "handleIn" | "handleOut" | null
  /** The path shape being edited */
  editingShapeId: ShapeId | null
  /** Whether we're currently dragging something */
  isDragging: boolean
  /** Currently selected anchor point index */
  selectedPointIndex: number | null
}

export function usePathEdit(editor: Editor) {
  const [editState, setEditState] = useState<PathEditState>({
    editingShapeId: null,
    selectedPointIndex: null,
    isDragging: false,
    dragType: null,
    dragPointIndex: null,
  })

  const isDraggingRef = useRef(false)

  const getEditingShape = useCallback((): PathShape | null => {
    if (!editState.editingShapeId) return null
    const shape = editor.getShape(editState.editingShapeId)
    if (!shape || shape.type !== "path") return null
    return shape as PathShape
  }, [editor, editState.editingShapeId])

  const startEditing = useCallback((shapeId: ShapeId) => {
    setEditState({
      editingShapeId: shapeId,
      selectedPointIndex: null,
      isDragging: false,
      dragType: null,
      dragPointIndex: null,
    })
  }, [])

  const stopEditing = useCallback(() => {
    setEditState({
      editingShapeId: null,
      selectedPointIndex: null,
      isDragging: false,
      dragType: null,
      dragPointIndex: null,
    })
  }, [])

  const selectPoint = useCallback((index: number | null) => {
    setEditState((prev) => ({
      ...prev,
      selectedPointIndex: index,
    }))
  }, [])

  // Anchor point drag handlers
  const handleAnchorDragStart = useCallback(
    (index: number, e: KonvaEventObject<DragEvent>) => {
      isDraggingRef.current = true
      setEditState((prev) => ({
        ...prev,
        isDragging: true,
        dragType: "anchor",
        dragPointIndex: index,
        selectedPointIndex: index,
      }))
      e.cancelBubble = true
    },
    []
  )

  const handleAnchorDragMove = useCallback(
    (index: number, e: KonvaEventObject<DragEvent>) => {
      if (!isDraggingRef.current) return

      const shape = getEditingShape()
      if (!shape) return

      const target = e.target as Konva.Circle
      const newX = target.x()
      const newY = target.y()

      // Update the point position
      const newPoints = [...shape.props.points]
      const point = newPoints[index]
      if (point) {
        newPoints[index] = {
          ...point,
          x: newX,
          y: newY,
        }

        editor.updateShape(shape.id, {
          props: {
            ...shape.props,
            points: newPoints,
          },
        })
      }

      e.cancelBubble = true
    },
    [editor, getEditingShape]
  )

  const handleAnchorDragEnd = useCallback(
    (_index: number, e: KonvaEventObject<DragEvent>) => {
      isDraggingRef.current = false
      setEditState((prev) => ({
        ...prev,
        isDragging: false,
        dragType: null,
        dragPointIndex: null,
      }))
      e.cancelBubble = true
    },
    []
  )

  // Handle drag handlers
  const handleHandleDragStart = useCallback(
    (
      pointIndex: number,
      handleType: "in" | "out",
      e: KonvaEventObject<DragEvent>
    ) => {
      isDraggingRef.current = true
      setEditState((prev) => ({
        ...prev,
        isDragging: true,
        dragType: handleType === "in" ? "handleIn" : "handleOut",
        dragPointIndex: pointIndex,
        selectedPointIndex: pointIndex,
      }))
      e.cancelBubble = true
    },
    []
  )

  const handleHandleDragMove = useCallback(
    (
      pointIndex: number,
      handleType: "in" | "out",
      e: KonvaEventObject<DragEvent>
    ) => {
      if (!isDraggingRef.current) return

      const shape = getEditingShape()
      if (!shape) return

      const target = e.target as Konva.Circle
      const point = shape.props.points[pointIndex]
      if (!point) return

      // Calculate handle offset relative to anchor point
      const handleOffset = {
        x: target.x() - point.x,
        y: target.y() - point.y,
      }

      // Update the handle
      const newPoints = [...shape.props.points]
      const updatedPoint: PathPoint = { ...point }

      if (handleType === "in") {
        updatedPoint.handleIn = handleOffset
        // Mirror to handleOut for smooth/symmetric points
        if (point.type === "smooth" || point.type === "symmetric") {
          const outLength =
            point.type === "symmetric"
              ? Math.sqrt(handleOffset.x ** 2 + handleOffset.y ** 2)
              : point.handleOut
                ? Math.sqrt(point.handleOut.x ** 2 + point.handleOut.y ** 2)
                : 0
          const angle = Math.atan2(-handleOffset.y, -handleOffset.x)
          updatedPoint.handleOut = {
            x: Math.cos(angle) * outLength,
            y: Math.sin(angle) * outLength,
          }
        }
      } else {
        updatedPoint.handleOut = handleOffset
        // Mirror to handleIn for smooth/symmetric points
        if (point.type === "smooth" || point.type === "symmetric") {
          const inLength =
            point.type === "symmetric"
              ? Math.sqrt(handleOffset.x ** 2 + handleOffset.y ** 2)
              : point.handleIn
                ? Math.sqrt(point.handleIn.x ** 2 + point.handleIn.y ** 2)
                : 0
          const angle = Math.atan2(-handleOffset.y, -handleOffset.x)
          updatedPoint.handleIn = {
            x: Math.cos(angle) * inLength,
            y: Math.sin(angle) * inLength,
          }
        }
      }

      newPoints[pointIndex] = updatedPoint

      editor.updateShape(shape.id, {
        props: {
          ...shape.props,
          points: newPoints,
        },
      })

      e.cancelBubble = true
    },
    [editor, getEditingShape]
  )

  const handleHandleDragEnd = useCallback(
    (
      _pointIndex: number,
      _handleType: "in" | "out",
      e: KonvaEventObject<DragEvent>
    ) => {
      isDraggingRef.current = false
      setEditState((prev) => ({
        ...prev,
        isDragging: false,
        dragType: null,
        dragPointIndex: null,
      }))
      e.cancelBubble = true
    },
    []
  )

  // Point manipulation functions
  const addPoint = useCallback(
    (afterIndex: number, position: { x: number; y: number }) => {
      const shape = getEditingShape()
      if (!shape) return

      const newPoint: PathPoint = {
        x: position.x - shape.props.x,
        y: position.y - shape.props.y,
        type: "corner",
      }

      const newPoints = [...shape.props.points]
      newPoints.splice(afterIndex + 1, 0, newPoint)

      editor.updateShape(shape.id, {
        props: {
          ...shape.props,
          points: newPoints,
        },
      })

      setEditState((prev) => ({
        ...prev,
        selectedPointIndex: afterIndex + 1,
      }))
    },
    [editor, getEditingShape]
  )

  const deletePoint = useCallback(
    (index: number) => {
      const shape = getEditingShape()
      if (!shape) return

      if (shape.props.points.length <= 2) {
        // Delete the entire shape if less than 2 points would remain
        editor.deleteShape(shape.id)
        stopEditing()
        return
      }

      const newPoints = shape.props.points.filter((_, i) => i !== index)

      editor.updateShape(shape.id, {
        props: {
          ...shape.props,
          points: newPoints,
        },
      })

      setEditState((prev) => ({
        ...prev,
        selectedPointIndex: null,
      }))
    },
    [editor, getEditingShape, stopEditing]
  )

  const changePointType = useCallback(
    (index: number, newType: PathPointType) => {
      const shape = getEditingShape()
      if (!shape) return

      const newPoints = [...shape.props.points]
      const point = newPoints[index]
      if (!point) return

      if (newType === "corner") {
        // Remove handles for corner points
        newPoints[index] = {
          ...point,
          type: "corner",
          handleIn: undefined,
          handleOut: undefined,
        }
      } else if (newType === "smooth" || newType === "symmetric") {
        // Convert to smooth/symmetric - handles will be created on next drag
        newPoints[index] = {
          ...point,
          type: newType,
        }
      }

      editor.updateShape(shape.id, {
        props: {
          ...shape.props,
          points: newPoints,
        },
      })
    },
    [editor, getEditingShape]
  )

  const toggleClosed = useCallback(() => {
    const shape = getEditingShape()
    if (!shape) return

    editor.updateShape(shape.id, {
      props: {
        ...shape.props,
        closed: !shape.props.closed,
      },
    })
  }, [editor, getEditingShape])

  return {
    editState,
    editingShape: getEditingShape(),
    startEditing,
    stopEditing,
    selectPoint,
    handleAnchorDragStart,
    handleAnchorDragMove,
    handleAnchorDragEnd,
    handleHandleDragStart,
    handleHandleDragMove,
    handleHandleDragEnd,
    addPoint,
    deletePoint,
    changePointType,
    toggleClosed,
  }
}
