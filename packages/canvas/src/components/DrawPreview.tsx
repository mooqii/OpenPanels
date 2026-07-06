import type Konva from "konva"
import { Ellipse, Line, Rect } from "react-konva"
import { INITIAL_GEO_FILL, INITIAL_GEO_STROKE } from "../constants"
import type { Tool } from "../store"
import type { BrushPoint } from "../types/shapes"

interface DrawPreviewProps {
  brushPreviewShape?: {
    points: BrushPoint[]
    outlinePoints: number[]
    x: number
    y: number
    color: string
    size: number
  } | null
  isDrawing: boolean
  markerPreviewShape?: {
    points: number[]
    x: number
    y: number
    color: string
    size: number
    opacity: number
  } | null
  pencilPreviewShape?: {
    points: number[]
    x: number
    y: number
    color: string
    size: number
  } | null
  previewShapeRef: React.RefObject<Konva.Shape | null>
  tool: Tool
}

export function DrawPreview({
  tool,
  isDrawing,
  previewShapeRef,
  pencilPreviewShape,
  brushPreviewShape,
  markerPreviewShape,
}: DrawPreviewProps) {
  // Marker preview - constant-width stroked line with transparency
  // Note: Single preview stroke can apply opacity directly since there's no overlap concern
  if (
    tool.name === "marker" &&
    isDrawing &&
    markerPreviewShape &&
    markerPreviewShape.points.length >= 4
  ) {
    return (
      <Line
        closed={false}
        lineCap="round"
        lineJoin="round"
        listening={false}
        opacity={markerPreviewShape.opacity}
        points={markerPreviewShape.points}
        ref={previewShapeRef as React.RefObject<Konva.Line | null>}
        stroke={markerPreviewShape.color}
        strokeWidth={markerPreviewShape.size}
        tension={0.5}
        x={markerPreviewShape.x}
        y={markerPreviewShape.y}
      />
    )
  }

  // Pencil preview - constant-width stroked line with smoothing
  if (
    tool.name === "pencil" &&
    isDrawing &&
    pencilPreviewShape &&
    pencilPreviewShape.points.length >= 4
  ) {
    return (
      <Line
        closed={false}
        lineCap="round"
        lineJoin="round"
        listening={false}
        points={pencilPreviewShape.points}
        ref={previewShapeRef as React.RefObject<Konva.Line | null>}
        stroke={pencilPreviewShape.color}
        strokeScaleEnabled={false}
        strokeWidth={pencilPreviewShape.size}
        tension={0.5}
        x={pencilPreviewShape.x}
        y={pencilPreviewShape.y}
      />
    )
  }

  // Brush preview - rendered as a filled polygon with variable width
  if (
    tool.name === "brush" &&
    isDrawing &&
    brushPreviewShape &&
    brushPreviewShape.outlinePoints.length >= 6
  ) {
    return (
      <Line
        closed={true}
        fill={brushPreviewShape.color}
        lineCap="round"
        lineJoin="round"
        listening={false}
        points={brushPreviewShape.outlinePoints}
        ref={previewShapeRef as React.RefObject<Konva.Line | null>}
        x={brushPreviewShape.x}
        y={brushPreviewShape.y}
      />
    )
  }

  // Rectangle/Ellipse/Line preview
  if (!isDrawing || tool.name !== "draw") return null

  if (tool.shape === "rectangle") {
    return (
      <Rect
        fill={INITIAL_GEO_FILL}
        ref={previewShapeRef as React.RefObject<Konva.Rect | null>}
        stroke={INITIAL_GEO_STROKE}
        strokeWidth={0}
      />
    )
  }

  if (tool.shape === "ellipse") {
    return (
      <Ellipse
        fill={INITIAL_GEO_FILL}
        radiusX={0}
        radiusY={0}
        ref={previewShapeRef as React.RefObject<Konva.Ellipse | null>}
        stroke={INITIAL_GEO_STROKE}
        strokeWidth={0}
      />
    )
  }

  if (tool.shape === "line") {
    return (
      <Line
        closed={false}
        lineCap="round"
        listening={false}
        points={[0, 0, 0, 0]}
        ref={previewShapeRef as React.RefObject<Konva.Line | null>}
        stroke={INITIAL_GEO_STROKE}
        strokeWidth={2}
      />
    )
  }

  return null
}
