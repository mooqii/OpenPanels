import type Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import { type RefObject, useCallback, useRef, useState } from "react"
import type { Editor } from "../editor"
import { getPointerPosition } from "../utils/coordinates"

interface UsePencilOptions {
  editor: Editor
  previewShapeRef: RefObject<Konva.Shape | null>
}

interface PreviewShape {
  color: string
  points: number[]
  size: number
  x: number
  y: number
}

export function usePencil({ editor, previewShapeRef }: UsePencilOptions) {
  const hasDraggedRef = useRef(false)
  const isDrawingRef = useRef(false)
  const pointsRef = useRef<number[]>([])
  const [previewShape, setPreviewShape] = useState<PreviewShape | null>(null)
  const [isDrawing, setIsDrawing] = useState(false)

  const handleMouseDown = useCallback(
    (_e: KonvaEventObject<PointerEvent>) => {
      const tool = editor.getTool()
      if (tool.name !== "pencil") return

      const pos = getPointerPosition(editor.stage)
      if (!pos) return

      hasDraggedRef.current = false
      isDrawingRef.current = true
      setIsDrawing(true)
      setPreviewShape(null)
      pointsRef.current = [pos.x, pos.y]
    },
    [editor]
  )

  const handleMouseMove = useCallback(
    (_e: KonvaEventObject<PointerEvent>) => {
      if (!isDrawingRef.current) return

      const tool = editor.getTool()
      if (tool.name !== "pencil") return

      const pos = getPointerPosition(editor.stage)
      if (!pos) return

      if (!hasDraggedRef.current) {
        hasDraggedRef.current = true
        pointsRef.current = [...pointsRef.current, pos.x, pos.y]
        setPreviewShape({
          points: [...pointsRef.current],
          x: 0,
          y: 0,
          color: tool.color,
          size: tool.size,
        })
        return
      }

      pointsRef.current.push(pos.x, pos.y)

      // Update Konva line directly for performance
      if (previewShapeRef.current) {
        const line = previewShapeRef.current as Konva.Line
        line.points(pointsRef.current)
      } else {
        setPreviewShape({
          points: [...pointsRef.current],
          x: 0,
          y: 0,
          color: tool.color,
          size: tool.size,
        })
      }
    },
    [editor, previewShapeRef]
  )

  const createShape = useCallback(() => {
    if (pointsRef.current.length < 4) return

    const tool = editor.getTool()
    if (tool.name !== "pencil") return

    // Calculate bounding box from points (with stroke padding)
    const strokeWidth = tool.size
    const padding = strokeWidth / 2

    let minX = Number.POSITIVE_INFINITY
    let minY = Number.POSITIVE_INFINITY
    let maxX = Number.NEGATIVE_INFINITY
    let maxY = Number.NEGATIVE_INFINITY

    for (let i = 0; i < pointsRef.current.length; i += 2) {
      const x = pointsRef.current[i]
      const y = pointsRef.current[i + 1]
      minX = Math.min(minX, x)
      minY = Math.min(minY, y)
      maxX = Math.max(maxX, x)
      maxY = Math.max(maxY, y)
    }

    // Add stroke padding to bounds
    minX -= padding
    minY -= padding
    maxX += padding
    maxY += padding

    const width = maxX - minX
    const height = maxY - minY

    // Adjust points to be relative to the shape origin (accounting for padding)
    const relativePoints: number[] = []
    for (let i = 0; i < pointsRef.current.length; i += 2) {
      relativePoints.push(
        pointsRef.current[i] - minX,
        pointsRef.current[i + 1] - minY
      )
    }

    editor.createShape({
      type: "draw",
      props: {
        points: relativePoints,
        x: minX,
        y: minY,
        color: tool.color,
        size: tool.size,
        width,
        height,
      },
    })
  }, [editor])

  const handleMouseUp = useCallback(() => {
    if (!isDrawingRef.current) {
      return
    }

    createShape()
    setPreviewShape(null)

    // Clean up
    hasDraggedRef.current = false
    isDrawingRef.current = false
    pointsRef.current = []
    setIsDrawing(false)
  }, [createShape])

  return {
    handleMouseDown,
    handleMouseMove,
    handleMouseUp,
    isDrawing,
    previewShape,
  }
}
