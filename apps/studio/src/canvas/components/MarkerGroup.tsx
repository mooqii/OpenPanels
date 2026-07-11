import { Group } from "react-konva"
import { MarkerShapeRenderer } from "../renderers/MarkerShapeRenderer"
import type { MarkerShape } from "../types/shapes"

interface MarkerGroupProps {
  /** Whether shapes should be draggable */
  draggable?: boolean
  /** Opacity to apply to the entire group (default: 0.5) */
  markerOpacity?: number
  /** Click handler for shapes */
  onClick?: (e: any) => void
  /** Drag end handler */
  onDragEnd?: (e: any) => void
  /** Drag move handler */
  onDragMove?: (e: any) => void
  /** Drag start handler */
  onDragStart?: (e: any) => void
  /** Mouse enter handler */
  onMouseEnter?: (e: any) => void
  /** Mouse leave handler */
  onMouseLeave?: (e: any) => void
  /** Tap handler (touch) */
  onTap?: (e: any) => void
  /** Transform end handler */
  onTransformEnd?: (e: any) => void
  /** Array of marker shapes to render */
  shapes: MarkerShape[]
}

/**
 * MarkerGroup renders all marker shapes in a Konva Group with shared opacity.
 *
 * Note: Without caching, overlapping marker strokes will accumulate color.
 * For true highlighter behavior (no accumulation), caching would be needed,
 * but it causes rendering issues. For now, we render with simple opacity.
 */
export function MarkerGroup({
  shapes,
  markerOpacity = 0.5,
  draggable = false,
  onClick,
  onDragStart,
  onDragEnd,
  onDragMove,
  onMouseEnter,
  onMouseLeave,
  onTap,
  onTransformEnd,
}: MarkerGroupProps) {
  // If no marker shapes, don't render anything
  if (shapes.length === 0) {
    return null
  }

  return (
    <Group listening={true}>
      {shapes.map((shape) => (
        <MarkerShapeRenderer
          draggable={draggable}
          key={shape.id}
          markerOpacity={markerOpacity}
          onClick={onClick}
          onDragEnd={onDragEnd}
          onDragMove={onDragMove}
          onDragStart={onDragStart}
          onMouseEnter={onMouseEnter}
          onMouseLeave={onMouseLeave}
          onTap={onTap}
          onTransformEnd={onTransformEnd}
          shape={shape}
        />
      ))}
    </Group>
  )
}
