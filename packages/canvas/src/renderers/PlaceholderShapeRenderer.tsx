import Konva from "konva"
import { useCallback, useEffect, useRef } from "react"
import { Group, Rect, Text } from "react-konva"
import { resolveCanvasPlaceholderFill } from "../constants"
import type { PlaceholderShape } from "../types/shapes"

interface PlaceholderShapeRendererProps {
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
  shape: PlaceholderShape
}

export function PlaceholderShapeRenderer({
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
  ref: refProp,
}: PlaceholderShapeRendererProps) {
  const props = shape.props
  const width = props.width ?? 100
  const height = props.height ?? 100
  const cornerRadius = props.cornerRadius ?? 0
  const fill = props.fill ?? resolveCanvasPlaceholderFill()
  const text = props.text

  const rectRef = useRef<Konva.Rect>(null)

  // Animate opacity (0.7–0.9) using Konva.Animation
  useEffect(() => {
    const node = rectRef.current
    if (!node) return

    const layer = node.getLayer()
    if (!layer) return

    const anim = new Konva.Animation((frame) => {
      const nodeCurrent = rectRef.current
      if (!nodeCurrent) return
      // Sine wave: keep the placeholder animated without making it pop too hard.
      const opacity = 0.82 + 0.08 * Math.sin((frame.time / 1600) * Math.PI * 2)
      nodeCurrent.opacity(opacity)
    }, layer)

    anim.start()
    return () => {
      anim.stop()
    }
  }, [])

  const handleGroupRef = useCallback(
    (node: Konva.Group | null) => {
      if (typeof refProp === "function") refProp(node)
    },
    [refProp]
  )

  const groupX = props.x ?? 0
  const groupY = props.y ?? 0
  const groupRotation = props.rotation ?? 0
  const groupScaleX = props.scaleX ?? 1
  const groupScaleY = props.scaleY ?? 1

  return (
    <Group
      draggable={draggable}
      id={shape.id}
      listening={true}
      name="placeholder"
      onClick={onClick}
      onDragEnd={onDragEnd}
      onDragMove={onDragMove}
      onDragStart={onDragStart}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      onTap={onTap}
      onTransformEnd={onTransformEnd}
      ref={handleGroupRef}
      rotation={groupRotation}
      scaleX={groupScaleX}
      scaleY={groupScaleY}
      x={groupX}
      y={groupY}
    >
      <Rect
        cornerRadius={cornerRadius}
        fill={fill}
        height={height}
        ref={rectRef}
        width={width}
        x={0}
        y={0}
      />
      {text != null && text !== "" && (
        <Text
          align="center"
          fill="#666"
          fontSize={14}
          height={height}
          listening={false}
          text={text}
          verticalAlign="middle"
          width={width}
          x={0}
          y={0}
        />
      )}
    </Group>
  )
}
