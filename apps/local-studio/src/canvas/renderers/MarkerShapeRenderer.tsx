import { Line } from "react-konva"
import type { MarkerShape } from "../types/shapes"

interface MarkerShapeRendererProps {
  draggable?: boolean
  /** Opacity override from MarkerGroup */
  markerOpacity?: number
  onClick?: (e: any) => void
  onDragEnd?: (e: any) => void
  onDragMove?: (e: any) => void
  onDragStart?: (e: any) => void
  onMouseEnter?: (e: any) => void
  onMouseLeave?: (e: any) => void
  onTap?: (e: any) => void
  onTransformEnd?: (e: any) => void
  ref?: (node: any) => void
  shape: MarkerShape
}

/**
 * Renders a marker shape as a Line with transparency.
 */
export function MarkerShapeRenderer({
  shape,
  draggable = false,
  markerOpacity,
  onClick,
  onDragStart,
  onDragEnd,
  onDragMove,
  onMouseEnter,
  onMouseLeave,
  onTap,
  onTransformEnd,
  ref,
}: MarkerShapeRendererProps) {
  const props = shape.props
  const points = props.points ?? []
  const color = props.color ?? "black"
  const strokeWidth = props.size ?? 4
  // Use markerOpacity from group if provided, otherwise use shape's stored opacity
  const opacity = markerOpacity ?? props.opacity ?? 0.5

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
      opacity={opacity}
      points={points}
      ref={ref}
      scaleX={props.scaleX ?? 1}
      scaleY={props.scaleY ?? 1}
      stroke={color}
      strokeScaleEnabled={true}
      strokeWidth={strokeWidth}
      tension={0.5}
      x={props.x ?? 0}
      y={props.y ?? 0}
    />
  )
}
