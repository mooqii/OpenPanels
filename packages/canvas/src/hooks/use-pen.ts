import type { KonvaEventObject } from "konva/lib/Node"
import { useCallback, useRef, useState } from "react"
import {
  PEN_CLOSE_THRESHOLD,
  PEN_STROKE_COLOR,
  PEN_STROKE_WIDTH,
} from "../constants"
import type { Editor } from "../editor"
import type { ShapeId } from "../types/ids"
import type { PathPoint, PathShape } from "../types/shapes"
import { getPointerPosition } from "../utils/coordinates"

interface PenState {
  /** Index of the point being dragged (for handle manipulation) */
  dragPointIndex: number | null
  /** The starting position of the current drag */
  dragStartPos: { x: number; y: number } | null
  /** Whether we're currently dragging to create a curve */
  isDragging: boolean
  /** Whether the path is being created (pen tool active with points) */
  isDrawing: boolean
  /** Current points being drawn */
  points: PathPoint[]
}

export function usePen(editor: Editor) {
  const [penState, setPenState] = useState<PenState>({
    points: [],
    isDragging: false,
    dragPointIndex: null,
    dragStartPos: null,
    isDrawing: false,
  })

  // Track mouse position for rendering preview line
  const [mousePosition, setMousePosition] = useState<{
    x: number
    y: number
  } | null>(null)

  const isDraggingRef = useRef(false)
  const dragStartPosRef = useRef<{ x: number; y: number } | null>(null)
  const currentPointIndexRef = useRef<number | null>(null)

  const finishPath = useCallback(
    (closed = false) => {
      if (penState.points.length < 2) {
        // Not enough points, cancel
        setPenState({
          points: [],
          isDragging: false,
          dragPointIndex: null,
          dragStartPos: null,
          isDrawing: false,
        })
        setMousePosition(null)
        return null
      }

      // Create the path shape
      const shape = editor.createShape({
        type: "path",
        props: {
          x: 0,
          y: 0,
          points: penState.points,
          closed,
          stroke: PEN_STROKE_COLOR,
          strokeWidth: PEN_STROKE_WIDTH,
          fill: closed ? "transparent" : undefined,
        },
      }) as PathShape

      // Reset state
      setPenState({
        points: [],
        isDragging: false,
        dragPointIndex: null,
        dragStartPos: null,
        isDrawing: false,
      })
      setMousePosition(null)

      // Switch back to select tool and select the new shape
      editor.setTool({ name: "select" })
      editor.setSelectedShapes([shape.id as ShapeId])

      return shape
    },
    [editor, penState.points]
  )

  const isNearFirstPoint = useCallback(
    (pos: { x: number; y: number }, points: PathPoint[]) => {
      if (points.length < 2) return false
      const first = points[0]
      const dx = pos.x - first.x
      const dy = pos.y - first.y
      // Adjust threshold by zoom level so it feels consistent in screen pixels
      const scale = editor.stage?.scaleX() ?? 1
      return Math.sqrt(dx * dx + dy * dy) < PEN_CLOSE_THRESHOLD / scale
    },
    [editor.stage]
  )

  const handleMouseDown = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      const tool = editor.getTool()
      if (tool.name !== "pen") return

      const pos = getPointerPosition(editor.stage)
      if (!pos) return

      // Check if clicking near first point to close
      if (isNearFirstPoint(pos, penState.points)) {
        // Close the path
        finishPath(true)
        return
      }

      isDraggingRef.current = true
      dragStartPosRef.current = pos
      currentPointIndexRef.current = penState.points.length

      // Add new point
      const newPoint: PathPoint = {
        x: pos.x,
        y: pos.y,
        type: "corner", // Will become smooth/symmetric if dragged
        handleIn: undefined,
        handleOut: undefined,
      }

      setPenState((prev) => ({
        ...prev,
        points: [...prev.points, newPoint],
        isDragging: true,
        dragPointIndex: prev.points.length,
        dragStartPos: pos,
        isDrawing: true,
      }))

      e.cancelBubble = true
    },
    [editor, penState.points, isNearFirstPoint, finishPath]
  )

  const handleMouseMove = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      const tool = editor.getTool()
      if (tool.name !== "pen") return

      // Update mouse position for rendering preview line
      const pos = getPointerPosition(editor.stage)
      if (pos) {
        setMousePosition(pos)
      }

      if (!isDraggingRef.current) return
      if (!(pos && dragStartPosRef.current)) return

      const pointIndex = currentPointIndexRef.current
      if (pointIndex === null) return

      // Calculate handle offset from drag start position
      const handleOut = {
        x: pos.x - dragStartPosRef.current.x,
        y: pos.y - dragStartPosRef.current.y,
      }

      // Mirror the handle for smooth curves
      const handleIn = {
        x: -handleOut.x,
        y: -handleOut.y,
      }

      // Only update if there's meaningful drag distance
      const dragDist = Math.sqrt(
        handleOut.x * handleOut.x + handleOut.y * handleOut.y
      )

      if (dragDist > 3) {
        setPenState((prev) => {
          const newPoints = [...prev.points]
          if (newPoints[pointIndex]) {
            newPoints[pointIndex] = {
              ...newPoints[pointIndex],
              type: "smooth",
              handleIn,
              handleOut,
            }
          }
          return { ...prev, points: newPoints }
        })
      }

      e.cancelBubble = true
    },
    [editor]
  )

  const handleMouseUp = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      const tool = editor.getTool()
      if (tool.name !== "pen") return
      if (!isDraggingRef.current) return

      isDraggingRef.current = false
      dragStartPosRef.current = null

      setPenState((prev) => ({
        ...prev,
        isDragging: false,
        dragPointIndex: null,
        dragStartPos: null,
      }))

      e.cancelBubble = true
    },
    [editor]
  )

  const cancelPath = useCallback(() => {
    setPenState({
      points: [],
      isDragging: false,
      dragPointIndex: null,
      dragStartPos: null,
      isDrawing: false,
    })
    setMousePosition(null)
  }, [])

  const removeLastPoint = useCallback(() => {
    if (penState.points.length === 0) return

    if (penState.points.length === 1) {
      // Cancel if removing last point
      cancelPath()
      return
    }

    setPenState((prev) => ({
      ...prev,
      points: prev.points.slice(0, -1),
    }))
  }, [penState.points.length, cancelPath])

  // Rendering state - computed for the UI overlay
  const isNearFirstPointForRender = mousePosition
    ? isNearFirstPoint(mousePosition, penState.points)
    : false

  return {
    handleMouseDown,
    handleMouseMove,
    handleMouseUp,
    finishPath,
    cancelPath,
    removeLastPoint,
    penState,
    isDrawing: penState.isDrawing,
    previewPoints: penState.points,
    mousePosition,
    isNearFirstPoint: isNearFirstPointForRender,
  }
}
