import { Path } from "react-konva"
import type { PathPoint, PathShape } from "../types/shapes"

interface PathShapeRendererProps {
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
  shape: PathShape
}

/**
 * Converts an array of PathPoints to an SVG path data string
 */
function pointsToSvgPath(points: PathPoint[], closed: boolean): string {
  if (points.length === 0) return ""

  const commands: string[] = []

  // Start with move to first point
  const first = points[0]
  commands.push(`M ${first.x} ${first.y}`)

  // Draw bezier curves or lines to subsequent points
  for (let i = 1; i < points.length; i++) {
    const prev = points[i - 1]
    const curr = points[i]

    // Calculate absolute positions for control points
    const cp1 = prev.handleOut
      ? { x: prev.x + prev.handleOut.x, y: prev.y + prev.handleOut.y }
      : { x: prev.x, y: prev.y }

    const cp2 = curr.handleIn
      ? { x: curr.x + curr.handleIn.x, y: curr.y + curr.handleIn.y }
      : { x: curr.x, y: curr.y }

    // Check if we need a bezier curve or a straight line
    const hasCurve =
      (prev.handleOut && (prev.handleOut.x !== 0 || prev.handleOut.y !== 0)) ||
      (curr.handleIn && (curr.handleIn.x !== 0 || curr.handleIn.y !== 0))

    if (hasCurve) {
      // Cubic bezier curve: C cp1x,cp1y cp2x,cp2y endx,endy
      commands.push(`C ${cp1.x} ${cp1.y} ${cp2.x} ${cp2.y} ${curr.x} ${curr.y}`)
    } else {
      // Straight line
      commands.push(`L ${curr.x} ${curr.y}`)
    }
  }

  // Close the path if needed
  if (closed && points.length > 2) {
    const last = points.at(points.length - 1)!

    // Calculate control points for closing segment
    const cp1 = last.handleOut
      ? { x: last.x + last.handleOut.x, y: last.y + last.handleOut.y }
      : { x: last.x, y: last.y }

    const cp2 = first.handleIn
      ? { x: first.x + first.handleIn.x, y: first.y + first.handleIn.y }
      : { x: first.x, y: first.y }

    const hasCurve =
      (last.handleOut && (last.handleOut.x !== 0 || last.handleOut.y !== 0)) ||
      (first.handleIn && (first.handleIn.x !== 0 || first.handleIn.y !== 0))

    if (hasCurve) {
      commands.push(
        `C ${cp1.x} ${cp1.y} ${cp2.x} ${cp2.y} ${first.x} ${first.y}`
      )
    }

    commands.push("Z")
  }

  return commands.join(" ")
}

export function PathShapeRenderer({
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
}: PathShapeRendererProps) {
  const props = shape.props
  const pathData = pointsToSvgPath(props.points, props.closed)

  if (props.points.length < 1) {
    return null
  }

  return (
    <Path
      data={pathData}
      draggable={draggable}
      fill={props.closed ? props.fill : undefined}
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
      ref={ref}
      scaleX={props.scaleX ?? 1}
      scaleY={props.scaleY ?? 1}
      stroke={props.stroke}
      strokeScaleEnabled={false}
      strokeWidth={props.strokeWidth}
      x={props.x ?? 0}
      y={props.y ?? 0}
    />
  )
}

// Export the utility function for use in preview rendering
export { pointsToSvgPath }
