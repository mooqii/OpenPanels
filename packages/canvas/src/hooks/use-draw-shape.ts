import type Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import React from "react"
import { INITIAL_GEO_FILL, INITIAL_GEO_STROKE } from "../constants"
import type { Editor } from "../editor"
import type { Tool } from "../store"
import { createSolidFill } from "../utils/fill"

export function useDrawShape({
  editor,
  previewShapeRef,
  isShiftPressed,
}: {
  editor: Editor
  previewShapeRef: React.RefObject<Konva.Shape | null>
  isShiftPressed: boolean
}) {
  const startPositionRef = React.useRef<{ x: number; y: number } | null>(null)
  const [isDrawing, setIsDrawing] = React.useState(false)
  const currentToolRef = React.useRef<Tool>({ name: "select" })

  // Track current tool
  React.useEffect(() => {
    currentToolRef.current = editor.getTool()
  }, [editor])

  const handleMouseDown = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      const tool = editor.getTool()

      // Only handle draw tool
      if (tool.name !== "draw") {
        return
      }

      const pos = e.target.getStage()?.getRelativePointerPosition()
      if (!pos) return

      startPositionRef.current = { x: pos.x, y: pos.y }
      currentToolRef.current = tool
      setIsDrawing(true)

      // Initialize preview shape
      const previewShape = previewShapeRef.current
      if (previewShape) {
        if (tool.shape === "rectangle") {
          previewShape.setPosition({ x: pos.x, y: pos.y })
          previewShape.setSize({ width: 0, height: 0 })
        } else if (tool.shape === "ellipse") {
          previewShape.setPosition({ x: pos.x, y: pos.y })
          ;(previewShape as Konva.Ellipse).radiusX(0)
          ;(previewShape as Konva.Ellipse).radiusY(0)
        } else if (tool.shape === "line") {
          previewShape.setPosition({ x: pos.x, y: pos.y })
          previewShape.setSize({ width: 0, height: 0 })
        }
        previewShape.visible(true)
      }
    },
    [editor, previewShapeRef]
  )

  const handleMouseMove = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      if (!(isDrawing && startPositionRef.current)) return

      const pos = e.target.getStage()?.getRelativePointerPosition()
      const previewShape = previewShapeRef.current
      const tool = editor.getTool()
      if (!(pos && previewShape) || tool.name !== "draw") return

      const startPos = startPositionRef.current
      let dx = pos.x - startPos.x
      let dy = pos.y - startPos.y

      // Lock to 1:1 aspect ratio when Shift is held (for shapes)
      // For lines, snap to 45-degree increments (0, 45, 90, 135, etc.)
      if (isShiftPressed) {
        if (tool.shape === "line") {
          // Calculate angle and snap to nearest 45 degrees
          const angle = Math.atan2(dy, dx)
          const snappedAngle = Math.round(angle / (Math.PI / 4)) * (Math.PI / 4)
          const length = Math.sqrt(dx * dx + dy * dy)
          dx = Math.cos(snappedAngle) * length
          dy = Math.sin(snappedAngle) * length
        } else {
          const size = Math.max(Math.abs(dx), Math.abs(dy))
          dx = size * Math.sign(dx || 1)
          dy = size * Math.sign(dy || 1)
        }
      }

      if (tool.shape === "rectangle") {
        // Rectangle: position is top-left, size is width/height
        const x = dx < 0 ? startPos.x + dx : startPos.x
        const y = dy < 0 ? startPos.y + dy : startPos.y
        previewShape.setPosition({ x, y })
        previewShape.setSize({
          width: Math.abs(dx),
          height: Math.abs(dy),
        })
      } else if (tool.shape === "ellipse") {
        // Ellipse: position is center, size uses radiusX/radiusY
        const centerX = startPos.x + dx / 2
        const centerY = startPos.y + dy / 2
        const radius = Math.abs(dx) / 2
        previewShape.setPosition({ x: centerX, y: centerY })
        ;(previewShape as Konva.Ellipse).radiusX(radius)
        ;(previewShape as Konva.Ellipse).radiusY(Math.abs(dy) / 2)
      } else if (tool.shape === "line") {
        // Line: position is start point, width/height is end point relative to start
        previewShape.setPosition({ x: startPos.x, y: startPos.y })
        previewShape.setSize({
          width: dx,
          height: dy,
        })
        // Update line points: [startX, startY, endX, endY] in local coordinates
        const line = previewShape as Konva.Line
        line.points([0, 0, dx, dy])
      }
    },
    [isDrawing, editor, previewShapeRef, isShiftPressed]
  )

  const handleMouseUp = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      const previewShape = previewShapeRef.current
      if (!(isDrawing && startPositionRef.current)) {
        setIsDrawing(false)
        startPositionRef.current = null
        if (previewShape) {
          previewShape.visible(false)
        }
        return
      }

      const stage = e.target.getStage()
      const pos = stage?.getRelativePointerPosition()
      if (!(pos && previewShape)) {
        setIsDrawing(false)
        startPositionRef.current = null
        return
      }

      const startPos = startPositionRef.current
      const tool = currentToolRef.current
      const shiftHeld = e.evt.shiftKey

      // Calculate final dimensions
      let dx = pos.x - startPos.x
      let dy = pos.y - startPos.y

      // Lock to 1:1 aspect ratio when Shift is held (for shapes)
      // For lines, snap to 45-degree increments (0, 45, 90, 135, etc.)
      if (shiftHeld) {
        if (tool.name === "draw" && tool.shape === "line") {
          // Calculate angle and snap to nearest 45 degrees
          const angle = Math.atan2(dy, dx)
          const snappedAngle = Math.round(angle / (Math.PI / 4)) * (Math.PI / 4)
          const length = Math.sqrt(dx * dx + dy * dy)
          dx = Math.cos(snappedAngle) * length
          dy = Math.sin(snappedAngle) * length
        } else {
          const size = Math.max(Math.abs(dx), Math.abs(dy))
          dx = size * Math.sign(dx || 1)
          dy = size * Math.sign(dy || 1)
        }
      }

      const width = Math.abs(dx)
      const height = Math.abs(dy)
      const x = dx < 0 ? startPos.x + dx : startPos.x
      const y = dy < 0 ? startPos.y + dy : startPos.y

      // Only create shape if it has minimum size
      // For lines, use line length; for rectangles/ellipses, use min dimension
      let minSize: number
      if (tool.name === "draw" && tool.shape === "line") {
        // For lines, calculate the actual line length
        minSize = Math.sqrt(width * width + height * height)
      } else {
        // For rectangles and ellipses, use locked size for check
        minSize = shiftHeld ? Math.max(width, height) : Math.min(width, height)
      }
      if (minSize >= 5 && tool.name === "draw") {
        if (tool.shape === "rectangle") {
          editor.createShape({
            type: "geo",
            props: {
              geo: "rectangle",
              x,
              y,
              width,
              height,
              shapeFill: createSolidFill(INITIAL_GEO_FILL),
              stroke: INITIAL_GEO_STROKE,
            },
          })
        } else if (tool.shape === "ellipse") {
          editor.createShape({
            type: "geo",
            props: {
              geo: "ellipse",
              x: x + width / 2,
              y: y + height / 2,
              width,
              height,
              shapeFill: createSolidFill(INITIAL_GEO_FILL),
              stroke: INITIAL_GEO_STROKE,
            },
          })
        } else if (tool.shape === "line") {
          // Line: x, y is start point, width/height is end point relative to start
          editor.createShape({
            type: "geo",
            props: {
              geo: "line",
              x: startPos.x,
              y: startPos.y,
              width: dx,
              height: dy,
              stroke: INITIAL_GEO_STROKE,
              strokeWidth: 2,
            },
          })
        }
      }

      // Reset tool to select after drawing
      editor.setTool({ name: "select" })

      // Clean up
      setIsDrawing(false)
      startPositionRef.current = null
      if (previewShape) {
        previewShape.visible(false)
      }
    },
    [isDrawing, editor, previewShapeRef]
  )

  return {
    handleMouseDown,
    handleMouseMove,
    handleMouseUp,
    isDrawing,
  }
}
