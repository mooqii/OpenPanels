import type Konva from "konva"
import { useCallback, useEffect, useMemo, useState } from "react"
import { Shape } from "react-konva"
import type { GeoShape, RectShapeProps, StrokePosition } from "../types/shapes"
import {
  computeFillProps,
  drawRoundedRect,
  getCanvasFillStyle,
} from "./geo-shape-utils"

interface GeoRectangleRendererProps {
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

export function GeoRectangleRenderer({
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
}: GeoRectangleRendererProps) {
  const props = shape.props as RectShapeProps
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

  // Scene function for rectangle
  // Read dimensions from Konva node to support real-time Transformer updates
  const rectSceneFunc = useCallback(
    (ctx: Konva.Context, konvaShape: Konva.Shape) => {
      const w = konvaShape.width()
      const h = konvaShape.height()
      const sw = konvaShape.strokeWidth()
      // Support both uniform (number) and mixed (array) corner radius
      // Array format: [top-left, top-right, bottom-right, bottom-left]
      const rawCornerRadius = props.cornerRadius
      const cornerRadius: [number, number, number, number] = Array.isArray(
        rawCornerRadius
      )
        ? (rawCornerRadius as [number, number, number, number])
        : [
            (rawCornerRadius as number) || 0,
            (rawCornerRadius as number) || 0,
            (rawCornerRadius as number) || 0,
            (rawCornerRadius as number) || 0,
          ]
      const hasRadius = cornerRadius.some((r) => r > 0)
      const nativeCtx = (ctx as any)._context as CanvasRenderingContext2D
      const stroke = konvaShape.stroke()
      const fillStyle = getCanvasFillStyle(nativeCtx, konvaShape)

      ctx.beginPath()
      if (hasRadius) {
        drawRoundedRect(ctx, 0, 0, w, h, cornerRadius)
      } else {
        ctx.rect(0, 0, w, h)
      }

      if (strokePosition === "inside" && sw > 0) {
        // For inside stroke: fill at full size, stroke at inset path with doubled width
        // This avoids using clip and ensures visible stroke width matches user expectation

        // Draw fill at original bounds
        if (fillStyle) {
          nativeCtx.fillStyle = fillStyle
          nativeCtx.fill()
        }

        // Draw stroke at inset bounds with doubled width
        if (stroke) {
          const inset = sw / 2
          // Ensure we have enough space for the stroke
          const strokeW = Math.max(0, w - sw)
          const strokeH = Math.max(0, h - sw)

          if (strokeW > 0 && strokeH > 0) {
            const insetCr: [number, number, number, number] = [
              Math.max(0, cornerRadius[0] - inset),
              Math.max(0, cornerRadius[1] - inset),
              Math.max(0, cornerRadius[2] - inset),
              Math.max(0, cornerRadius[3] - inset),
            ]
            ctx.beginPath()
            if (hasRadius) {
              drawRoundedRect(ctx, inset, inset, strokeW, strokeH, insetCr)
            } else {
              ctx.rect(inset, inset, strokeW, strokeH)
            }
            nativeCtx.strokeStyle = stroke
            nativeCtx.lineWidth = sw
            nativeCtx.stroke()
          }
        }
      } else if (strokePosition === "outside" && sw > 0) {
        // For outside stroke: fill at full size, stroke at outset path with doubled width

        // Draw fill at original bounds
        if (fillStyle) {
          nativeCtx.fillStyle = fillStyle
          nativeCtx.fill()
        }

        // Draw stroke at outset bounds with doubled width
        if (stroke) {
          const offset = sw / 2
          const outsetCr: [number, number, number, number] = [
            cornerRadius[0] > 0 ? cornerRadius[0] + offset : 0,
            cornerRadius[1] > 0 ? cornerRadius[1] + offset : 0,
            cornerRadius[2] > 0 ? cornerRadius[2] + offset : 0,
            cornerRadius[3] > 0 ? cornerRadius[3] + offset : 0,
          ]
          ctx.beginPath()
          if (hasRadius) {
            drawRoundedRect(ctx, -offset, -offset, w + sw, h + sw, outsetCr)
          } else {
            ctx.rect(-offset, -offset, w + sw, h + sw)
          }
          nativeCtx.strokeStyle = stroke
          nativeCtx.lineWidth = sw
          nativeCtx.stroke()
        }
      } else {
        // Center mode - standard behavior
        ctx.strokeStyle = stroke
        ctx.lineWidth = sw
        ctx.fillStrokeShape(konvaShape)
      }
    },
    [strokePosition, props.cornerRadius]
  )

  // Hit function for rectangle - uses logical bounds from Konva node
  const rectHitFunc = useCallback(
    (ctx: Konva.Context, konvaShape: Konva.Shape) => {
      const w = konvaShape.width()
      const h = konvaShape.height()
      ctx.beginPath()
      ctx.rect(0, 0, w, h)
      ctx.fillStrokeShape(konvaShape)
    },
    []
  )

  // Ref callback to override getSelfRect for correct getClientRect behavior
  // getSelfRect reads from node dimensions dynamically for real-time Transformer updates
  const handleRef = useCallback(
    (node: Konva.Shape | null) => {
      if (node) {
        // For rectangle, getSelfRect returns bounds at origin
        // Read dimensions from node dynamically for transform support
        node.getSelfRect = () => ({
          x: 0,
          y: 0,
          width: node.width(),
          height: node.height(),
        })
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
      hitFunc={rectHitFunc}
      id={shape.id}
      listening={true}
      name="geo-rectangle"
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
      sceneFunc={rectSceneFunc}
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
