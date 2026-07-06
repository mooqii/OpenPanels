import type Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import { type RefObject, useCallback, useRef, useState } from "react"
import type { Editor } from "../editor"
import type { BrushPoint } from "../types/shapes"
import {
  generateStrokeOutline,
  type RawBrushPoint,
  rawPointsToBrushPoints,
  simplifyPoints,
} from "../utils/brush"
import { getPointerPosition } from "../utils/coordinates"

interface UseBrushOptions {
  editor: Editor
  previewShapeRef: RefObject<Konva.Shape | null>
}

interface PreviewShape {
  color: string
  outlinePoints: number[]
  points: BrushPoint[]
  size: number
  x: number
  y: number
}

export function useBrush({ editor, previewShapeRef }: UseBrushOptions) {
  const isDrawingRef = useRef(false)
  const rawPointsRef = useRef<RawBrushPoint[]>([])
  const outlinePointsRef = useRef<number[]>([])
  const [previewShape, setPreviewShape] = useState<PreviewShape | null>(null)
  const [isDrawing, setIsDrawing] = useState(false)

  const handleMouseDown = useCallback(
    (_e: KonvaEventObject<PointerEvent>) => {
      const tool = editor.getTool()
      if (tool.name !== "brush") return

      const pos = getPointerPosition(editor.stage)
      if (!pos) return

      isDrawingRef.current = true
      setIsDrawing(true)

      const timestamp = performance.now()

      // Start with just one point - we'll add more as the user moves
      rawPointsRef.current = [{ x: pos.x, y: pos.y, timestamp }]
      const initialBrushPoints = rawPointsToBrushPoints(rawPointsRef.current)
      const initialOutlinePoints = generateStrokeOutline(
        initialBrushPoints,
        tool.size
      )
      outlinePointsRef.current = initialOutlinePoints

      // Initialize preview shape so brush clicks preview and commit as a dot.
      setPreviewShape({
        points: initialBrushPoints,
        outlinePoints: initialOutlinePoints,
        x: 0,
        y: 0,
        color: tool.color,
        size: tool.size,
      })
    },
    [editor]
  )

  const handleMouseMove = useCallback(
    (_e: KonvaEventObject<PointerEvent>) => {
      if (!isDrawingRef.current) return

      const tool = editor.getTool()
      if (tool.name !== "brush") return

      const pos = getPointerPosition(editor.stage)
      if (!pos) return

      // Check if we've moved enough from the last point
      const lastPoint = rawPointsRef.current.at(-1)!
      const dx = pos.x - lastPoint.x
      const dy = pos.y - lastPoint.y
      const distance = Math.sqrt(dx * dx + dy * dy)

      // Only add point if we've moved at least 1 pixel
      if (distance < 1) return

      // Add new point with timestamp
      const newPoint: RawBrushPoint = {
        x: pos.x,
        y: pos.y,
        timestamp: performance.now(),
      }

      rawPointsRef.current.push(newPoint)

      // Simplify points slightly (perfect-freehand handles smoothing)
      const simplified = simplifyPoints(rawPointsRef.current, 2)

      // Need at least 2 distinct points to generate outline
      if (simplified.length < 2) return

      const brushPoints = rawPointsToBrushPoints(simplified)
      const outlinePoints = generateStrokeOutline(brushPoints, tool.size)

      // Only update if we have valid outline
      if (outlinePoints.length < 6) return

      // Store for later use
      outlinePointsRef.current = outlinePoints

      // Update preview shape using Konva directly for performance
      if (previewShapeRef.current) {
        const line = previewShapeRef.current as Konva.Line
        line.points(outlinePoints)
      }

      // Update state for rendering
      setPreviewShape((prev) => {
        if (!prev) return null
        return {
          ...prev,
          points: brushPoints,
          outlinePoints,
        }
      })
    },
    [editor, previewShapeRef]
  )

  const createShape = useCallback(() => {
    if (rawPointsRef.current.length === 0) return

    const tool = editor.getTool()
    if (tool.name !== "brush") return

    // Final simplification and conversion
    const simplified = simplifyPoints(rawPointsRef.current, 2)
    const brushPoints = rawPointsToBrushPoints(simplified)

    if (brushPoints.length === 0) return

    const outlinePoints = generateStrokeOutline(brushPoints, tool.size)
    if (outlinePoints.length < 6) return

    // Calculate bounding box from outline points
    let minX = Number.POSITIVE_INFINITY
    let minY = Number.POSITIVE_INFINITY
    let maxX = Number.NEGATIVE_INFINITY
    let maxY = Number.NEGATIVE_INFINITY

    for (let i = 0; i < outlinePoints.length; i += 2) {
      const x = outlinePoints[i]
      const y = outlinePoints[i + 1]
      minX = Math.min(minX, x)
      minY = Math.min(minY, y)
      maxX = Math.max(maxX, x)
      maxY = Math.max(maxY, y)
    }

    const width = maxX - minX
    const height = maxY - minY

    // Adjust points to be relative to the shape origin (minX, minY)
    const relativePoints: BrushPoint[] = brushPoints.map((p) => ({
      x: p.x - minX,
      y: p.y - minY,
      pressure: p.pressure,
    }))

    editor.createShape({
      type: "brush",
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
    isDrawingRef.current = false
    rawPointsRef.current = []
    outlinePointsRef.current = []
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
