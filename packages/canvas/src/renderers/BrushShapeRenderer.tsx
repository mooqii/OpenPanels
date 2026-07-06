import { Line } from "react-konva"
import type { BrushShape } from "../types/shapes"
import { generateStrokeOutline } from "../utils/brush"

interface BrushShapeRendererProps {
  draggable?: boolean
  onClick?: (e: any) => void
  onDragEnd?: (e: any) => void
  onDragMove?: (e: any) => void
  onDragStart?: (e: any) => void
  onMouseEnter?: (e: any) => void
  onMouseLeave?: (e: any) => void
  onTap?: (e: any) => void
  onTransformEnd?: (e: any) => void
  ref?: (node: any) => void
  shape: BrushShape
}

export function BrushShapeRenderer({
  shape,
  draggable = false,
  onClick,
  onDragStart,
  onDragEnd,
  onDragMove,
  onMouseEnter,
  onMouseLeave,
  onTap,
  onTransformEnd,
  ref,
}: BrushShapeRendererProps) {
  const props = shape.props
  const points = props.points ?? []
  const color = props.color ?? "black"
  const size = props.size ?? "m"

  // Generate outline polygon from brush points
  const outlinePoints = generateStrokeOutline(points, size)

  if (outlinePoints.length < 6) {
    return null
  }

  return (
    <Line
      closed={true}
      draggable={draggable}
      fill={color}
      id={shape.id}
      lineCap="round"
      lineJoin="round"
      listening={true}
      onClick={onClick}
      onDragEnd={onDragEnd}
      onDragMove={onDragMove}
      onDragStart={onDragStart}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      onTap={onTap}
      onTransformEnd={onTransformEnd}
      points={outlinePoints}
      ref={ref}
      scaleX={props.scaleX ?? 1}
      scaleY={props.scaleY ?? 1}
      strokeScaleEnabled={false}
      x={props.x ?? 0}
      y={props.y ?? 0}
    />
  )
}
