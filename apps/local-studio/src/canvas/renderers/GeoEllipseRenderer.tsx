import type Konva from "konva"
import { useCallback, useEffect, useMemo, useState } from "react"
import { Shape } from "react-konva"
import type {
  EllipseShapeProps,
  GeoShape,
  StrokePosition,
} from "../types/shapes"
import { computeFillProps, getCanvasFillStyle } from "./geo-shape-utils"

interface GeoEllipseRendererProps {
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

export function GeoEllipseRenderer({
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
  resolveAsset,
}: GeoEllipseRendererProps) {
  const props = shape.props as EllipseShapeProps
  const strokePosition: StrokePosition = props.strokePosition || "center"
  const strokeWidth = (props.strokeWidth as number) || 0
  const width = props.width ?? 100
  const height = props.height ?? 100
  const shadowEnabled = props.shadowEnabled ?? (props.shadowBlur ?? 0) > 0

  const shapeFill = props.shapeFill

  // State for loaded pattern image
  const [patternImage, setPatternImage] = useState<HTMLImageElement | null>(
    null
  )

  // Load image for pattern fills
  useEffect(() => {
    if (shapeFill?.type === "image" && resolveAsset) {
      const src = resolveAsset(shapeFill.assetId as string)
      if (src) {
        const img = new Image()
        img.crossOrigin = "anonymous"
        img.onload = () => setPatternImage(img)
        img.onerror = () => setPatternImage(null)
        img.src = src
        return () => {
          img.onload = null
          img.onerror = null
        }
      }
    }
    setPatternImage(null)
  }, [shapeFill, resolveAsset])

  // Compute Konva fill properties based on fill type
  const fillProps = useMemo(
    () => computeFillProps(shapeFill, width, height, patternImage),
    [shapeFill, width, height, patternImage]
  )

  // Scene function for ellipse
  // Read dimensions from Konva node to support real-time Transformer updates
  const ellipseSceneFunc = useCallback(
    (ctx: Konva.Context, konvaShape: Konva.Shape) => {
      // Derive radii from Konva node dimensions for real-time transform updates
      const w = konvaShape.width()
      const h = konvaShape.height()
      const sw = konvaShape.strokeWidth()
      const rx = w / 2
      const ry = h / 2
      const nativeCtx = (ctx as any)._context as CanvasRenderingContext2D
      const stroke = konvaShape.stroke()
      const fillStyle = getCanvasFillStyle(nativeCtx, konvaShape)

      // Draw fill at original radii
      ctx.beginPath()
      ctx.ellipse(0, 0, rx, ry, 0, 0, Math.PI * 2)
      ctx.closePath()

      if (strokePosition === "inside" && sw > 0) {
        // For inside stroke: fill at full size, stroke at inset radii with doubled width
        // This avoids using clip and ensures visible stroke width matches user expectation
        if (fillStyle) {
          nativeCtx.fillStyle = fillStyle
          nativeCtx.fill()
        }

        // Draw stroke at inset radii with doubled width
        if (stroke) {
          const inset = sw / 2
          const insetRx = Math.max(0, rx - inset)
          const insetRy = Math.max(0, ry - inset)

          if (insetRx > 0 && insetRy > 0) {
            ctx.beginPath()
            ctx.ellipse(0, 0, insetRx, insetRy, 0, 0, Math.PI * 2)
            ctx.closePath()
            nativeCtx.strokeStyle = stroke
            nativeCtx.lineWidth = sw
            nativeCtx.stroke()
          }
        }
      } else if (strokePosition === "outside" && sw > 0) {
        // For outside stroke: fill at full size, stroke at outset radii with doubled width
        if (fillStyle) {
          nativeCtx.fillStyle = fillStyle
          nativeCtx.fill()
        }

        // Draw stroke at outset radii with doubled width
        if (stroke) {
          const offset = sw / 2
          ctx.beginPath()
          ctx.ellipse(0, 0, rx + offset, ry + offset, 0, 0, Math.PI * 2)
          ctx.closePath()
          nativeCtx.strokeStyle = stroke
          nativeCtx.lineWidth = sw
          nativeCtx.stroke()
        }
      } else {
        // Center mode - standard behavior
        ctx.fillStrokeShape(konvaShape)
      }
    },
    [strokePosition]
  )

  // Hit function for ellipse - uses logical bounds from Konva node
  const ellipseHitFunc = useCallback(
    (ctx: Konva.Context, konvaShape: Konva.Shape) => {
      const w = konvaShape.width()
      const h = konvaShape.height()
      const rx = w / 2
      const ry = h / 2

      ctx.beginPath()
      ctx.ellipse(0, 0, rx, ry, 0, 0, Math.PI * 2)
      ctx.closePath()
      ctx.fillStrokeShape(konvaShape)
    },
    []
  )

  // Ref callback to override getSelfRect for correct getClientRect behavior
  // getSelfRect reads from node dimensions dynamically for real-time Transformer updates
  const handleRef = useCallback(
    (node: Konva.Shape | null) => {
      if (node) {
        // For ellipse, getSelfRect should return bounds centered at origin
        // Read dimensions from node dynamically for transform support
        node.getSelfRect = () => {
          const w = node.width()
          const h = node.height()
          const rx = w / 2
          const ry = h / 2
          return {
            x: -rx,
            y: -ry,
            width: w,
            height: h,
          }
        }
      }
      // Call original ref if provided
      if (typeof ref === "function") ref(node)
    },
    [ref]
  )

  return (
    <Shape
      dash={props.dash as number[]}
      draggable={draggable}
      height={height}
      hitFunc={ellipseHitFunc}
      id={shape.id}
      listening={true}
      name="geo-ellipse"
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
      sceneFunc={ellipseSceneFunc}
      shadowBlur={props.shadowBlur}
      shadowColor={props.shadowColor}
      shadowEnabled={shadowEnabled}
      shadowOffsetX={props.shadowOffsetX}
      shadowOffsetY={props.shadowOffsetY}
      shadowOpacity={props.shadowOpacity}
      stroke={props.stroke as string}
      strokeScaleEnabled={true}
      strokeWidth={strokeWidth}
      width={width}
      x={props.x ?? 0}
      y={props.y ?? 0}
      {...fillProps}
    />
  )
}
