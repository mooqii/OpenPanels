import { Line } from "react-konva"
import type { DrawShape } from "../types/shapes"

interface DrawShapeRendererProps {
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
  shape: DrawShape
}

export function DrawShapeRenderer({
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
}: DrawShapeRendererProps) {
  const props = shape.props
  const points = props.points ?? []
  const color = props.color ?? "black"
  const strokeWidth = props.size ?? 4

  if (points.length < 4) {
    return null
  }

  return (
    <Line
      closed={false}
      draggable={draggable}
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
      points={points}
      ref={ref}
      scaleX={props.scaleX ?? 1}
      scaleY={props.scaleY ?? 1}
      stroke={color}
      strokeScaleEnabled={false}
      strokeWidth={strokeWidth}
      tension={0.5}
      x={props.x ?? 0}
      y={props.y ?? 0}
    />
  )
}
