import type Konva from "konva"
import { Text } from "react-konva"
import { TEXT_DEFAULT_LINE_HEIGHT } from "../constants"
import type { TextShape } from "../types/shapes"

interface TextShapeRendererProps {
  draggable?: boolean
  isEditing?: boolean
  listening?: boolean
  onClick?: (e: any) => void
  onDoubleClick?: (e: any) => void
  onDragEnd?: (e: any) => void
  onDragMove?: (e: any) => void
  onDragStart?: (e: any) => void
  onMouseEnter?: (e: any) => void
  onMouseLeave?: (e: any) => void
  onTap?: (e: any) => void
  onTransformEnd?: (e: any) => void
  shape: TextShape
  textNodeRef?: (node: Konva.Text | null) => void
}

export function TextShapeRenderer({
  shape,
  draggable = false,
  listening = true,
  isEditing = false,
  onClick,
  onDoubleClick,
  onDragStart,
  onDragEnd,
  onDragMove,
  onMouseEnter,
  onMouseLeave,
  onTap,
  onTransformEnd,
  textNodeRef,
}: TextShapeRendererProps) {
  const props = shape.props
  const text = (props.text as string) ?? ""
  const fill = (props.fill as string) ?? "black"
  const fontSize = (props.fontSize as number) ?? 16
  const fontFamily = (props.fontFamily as string) ?? "Arial"
  const fontStyle = (props.fontStyle as string) ?? "normal"
  const align = (props.align as string) ?? "left"
  const width = (props.width as number) ?? 200
  const height = (props.height as number) ?? 50
  const lineHeight = (props.lineHeight as number) ?? TEXT_DEFAULT_LINE_HEIGHT
  const stroke = (props.stroke as string) ?? undefined
  const strokeWidth = (props.strokeWidth as number) ?? undefined
  const rotation = (props.rotation as number) ?? 0
  const scaleX = (props.scaleX as number) ?? 1
  const scaleY = (props.scaleY as number) ?? 1
  const verticalAlign = (props.verticalAlign as string) ?? "top"

  return (
    <Text
      align={align}
      draggable={draggable && !isEditing}
      fill={fill}
      fontFamily={fontFamily}
      fontSize={fontSize}
      fontStyle={fontStyle}
      height={height}
      id={shape.id}
      lineHeight={lineHeight}
      listening={listening}
      onClick={onClick}
      onDblClick={onDoubleClick}
      onDblTap={onDoubleClick}
      onDragEnd={onDragEnd}
      onDragMove={onDragMove}
      onDragStart={onDragStart}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      onTap={onTap}
      onTransformEnd={onTransformEnd}
      opacity={isEditing ? 0 : props.opacity}
      // Keep the editing ref stable. Re-wrapping it per render causes
      // react-konva to detach/attach the text node repeatedly and can loop.
      ref={textNodeRef}
      rotation={rotation}
      scaleX={scaleX}
      scaleY={scaleY}
      stroke={stroke}
      strokeWidth={strokeWidth}
      text={text}
      verticalAlign={verticalAlign as any}
      width={width}
      wrap={(props.wrap as any) ?? "word"}
      x={props.x}
      y={props.y}
    />
  )
}
