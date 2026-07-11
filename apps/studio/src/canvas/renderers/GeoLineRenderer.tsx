import type Konva from "konva"
import { useCallback } from "react"
import { Shape } from "react-konva"
import type {
  ArrowheadStyle,
  GeoShape,
  LineCap,
  LineShapeProps,
  LineStyle,
} from "../types/shapes"

interface GeoLineRendererProps {
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
  /** Function to resolve asset URL for image fills */
  resolveAsset?: (assetId: string) => string | undefined
  shape: GeoShape
}

export function GeoLineRenderer({
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
}: GeoLineRendererProps) {
  const props = shape.props as LineShapeProps
  const strokeWidth = props.strokeWidth || 0
  const width = props.width ?? 100
  const height = props.height ?? 100

  // Helper function to draw arrowheads
  const drawArrowhead = useCallback(
    (
      ctx: CanvasRenderingContext2D,
      x: number,
      y: number,
      angle: number,
      style: ArrowheadStyle,
      size: number,
      strokeColor: string,
      strokeWidth: number
    ) => {
      if (style === "none") return

      ctx.save()
      ctx.translate(x, y)
      ctx.rotate(angle)
      ctx.fillStyle = strokeColor
      ctx.strokeStyle = strokeColor
      ctx.lineWidth = strokeWidth

      switch (style) {
        case "arrow": {
          // V-shaped arrow
          ctx.beginPath()
          ctx.moveTo(0, 0)
          ctx.lineTo(-size, -size * 0.6)
          ctx.lineTo(-size * 0.7, 0)
          ctx.lineTo(-size, size * 0.6)
          ctx.closePath()
          ctx.fill()
          break
        }
        case "triangle": {
          // Filled triangle
          ctx.beginPath()
          ctx.moveTo(0, 0)
          ctx.lineTo(-size, -size * 0.7)
          ctx.lineTo(-size, size * 0.7)
          ctx.closePath()
          ctx.fill()
          break
        }
        case "circle": {
          // Filled circle
          ctx.beginPath()
          ctx.arc(-size * 0.5, 0, size * 0.5, 0, Math.PI * 2)
          ctx.fill()
          break
        }
        default:
          // "none" or unknown style - do nothing
          break
      }

      ctx.restore()
    },
    []
  )

  // Scene function for line
  const lineSceneFunc = useCallback(
    (ctx: Konva.Context, konvaShape: Konva.Shape) => {
      const w = konvaShape.width()
      const h = konvaShape.height()
      const sw = konvaShape.strokeWidth()
      const stroke = konvaShape.stroke() as string
      const nativeCtx = (ctx as any)._context as CanvasRenderingContext2D

      // Calculate line angle
      const angle = Math.atan2(h, w)
      const lineLength = Math.sqrt(w * w + h * h)

      // Get line style and convert to dash pattern
      const lineStyle: LineStyle = props.lineStyle || "solid"
      let dashPattern: number[] | undefined
      switch (lineStyle) {
        case "dashed":
          dashPattern = [10, 5]
          break
        case "dotted":
          dashPattern = [2, 2]
          break
        default:
          // "solid" or unknown - no dash pattern
          dashPattern = undefined
          break
      }

      // Get line cap
      const lineCap: LineCap = props.lineCap || "butt"
      nativeCtx.lineCap = lineCap

      // Get arrowhead settings
      const arrowStart: ArrowheadStyle = props.arrowStart || "none"
      const arrowEnd: ArrowheadStyle = props.arrowEnd || "none"
      const arrowSize = props.arrowSize ?? sw * 2

      // Calculate arrowhead offsets (how much to shorten the line)
      const startOffset = arrowStart === "none" ? 0 : arrowSize
      const endOffset = arrowEnd === "none" ? 0 : arrowSize

      // Draw the line
      nativeCtx.strokeStyle = stroke
      nativeCtx.lineWidth = sw
      if (dashPattern) {
        nativeCtx.setLineDash(dashPattern)
      } else {
        nativeCtx.setLineDash([])
      }

      ctx.beginPath()
      const startX = (startOffset / lineLength) * w
      const startY = (startOffset / lineLength) * h
      const endX = w - (endOffset / lineLength) * w
      const endY = h - (endOffset / lineLength) * h
      ctx.moveTo(startX, startY)
      ctx.lineTo(endX, endY)
      nativeCtx.stroke()

      // Draw arrowheads
      if (arrowStart !== "none") {
        drawArrowhead(
          nativeCtx,
          startX,
          startY,
          angle + Math.PI,
          arrowStart,
          arrowSize,
          stroke,
          sw
        )
      }

      if (arrowEnd !== "none") {
        drawArrowhead(
          nativeCtx,
          endX,
          endY,
          angle,
          arrowEnd,
          arrowSize,
          stroke,
          sw
        )
      }
    },
    [props, drawArrowhead]
  )

  // Hit function for line - uses wider hit area for easier selection
  const lineHitFunc = useCallback(
    (ctx: Konva.Context, konvaShape: Konva.Shape) => {
      const w = konvaShape.width()
      const h = konvaShape.height()
      const sw = Math.max(konvaShape.strokeWidth(), 10) // Minimum hit area

      // Create a wider hit area around the line
      const padding = sw / 2 + 5
      const angle = Math.atan2(h, w)
      const lineLength = Math.sqrt(w * w + h * h)

      // Calculate hit area dimensions
      const hitHeight = sw + padding * 2

      // Create a rectangle that encompasses the line with padding
      // The rectangle must be centered on y = 0 (where the line lies in rotated space)
      ctx.save()
      ctx.translate(w / 2, h / 2)
      ctx.rotate(angle)
      ctx.beginPath()
      ctx.rect(
        -lineLength / 2 - padding,
        -hitHeight / 2, // Center vertically on the line
        lineLength + padding * 2,
        hitHeight
      )
      ctx.closePath()
      ctx.fillStrokeShape(konvaShape)
      ctx.restore()
    },
    []
  )

  // Ref callback to override getSelfRect for correct getClientRect behavior
  // getSelfRect returns bounds that encompass the line and arrowheads
  const handleRef = useCallback(
    (node: Konva.Shape | null) => {
      if (node) {
        node.getSelfRect = () => {
          const w = node.width()
          const h = node.height()
          const sw = node.strokeWidth()

          const hasStart = props.arrowStart && props.arrowStart !== "none"
          const hasEnd = props.arrowEnd && props.arrowEnd !== "none"
          const arrowSize = props.arrowSize ?? sw * 2

          // Padding should be at least half of stroke width to cover line thickness
          let padding = sw / 2

          if (hasStart || hasEnd) {
            // Arrow styles (triangle/circle/arrow) have different widths
            // but arrowSize is a good approximation of the radius needed
            padding = Math.max(padding, arrowSize)
          }

          return {
            x: Math.min(0, w) - padding,
            y: Math.min(0, h) - padding,
            width: Math.abs(w) + padding * 2,
            height: Math.abs(h) + padding * 2,
          }
        }
      }
      // Call original ref if provided
      if (typeof ref === "function") ref(node)
    },
    [ref, props.arrowSize, props.arrowStart, props.arrowEnd]
  )

  return (
    <Shape
      draggable={draggable}
      height={height}
      hitFunc={lineHitFunc}
      id={shape.id}
      listening={true}
      name="geo-line"
      onClick={onClick}
      onDragEnd={onDragEnd}
      onDragMove={onDragMove}
      onDragStart={onDragStart}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      onTap={onTap}
      onTransformEnd={onTransformEnd}
      opacity={props.opacity}
      ref={handleRef}
      rotation={props.rotation}
      scaleX={props.scaleX}
      scaleY={props.scaleY}
      sceneFunc={lineSceneFunc}
      stroke={props.stroke as string}
      strokeScaleEnabled={true}
      strokeWidth={strokeWidth}
      width={width}
      x={props.x ?? 0}
      y={props.y ?? 0}
    />
  )
}
