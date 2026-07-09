import type Konva from "konva"
import { Group } from "react-konva"
import { useEditor } from "../EditorContext"
import { useOpenedGroupId, useShapeChildren } from "../hooks/use-editor-state"
import type { Asset } from "../types/assets"
import type { ShapeId } from "../types/ids"
import type { GroupShape, Shape, TextShapeProps } from "../types/shapes"
import { ShapeRenderer } from "./ShapeRenderer"

interface GroupShapeRendererProps {
  asset?: Asset
  draggable?: boolean
  editingShapeId?: ShapeId | null
  editingTextNodeRef?: (node: Konva.Text | null) => void
  editingTextProps?: TextShapeProps | null
  isEditing?: boolean
  onClick?: (e: any) => void
  onDoubleClick?: (e: any) => void
  onDragEnd?: (e: any) => void
  onDragMove?: (e: any) => void
  onDragStart?: (e: any) => void
  onMouseEnter?: (e: any) => void
  onMouseLeave?: (e: any) => void
  onTap?: (e: any) => void
  onTransformEnd?: (e: any) => void
  ref?: (node: any) => void
  /** Callback to register connector Konva Shape refs for direct manipulation during transforms */
  registerConnectorRef?: (connectorId: ShapeId, ref: Konva.Shape | null) => void
  /** Function to resolve asset URL for image fills */
  resolveAsset?: (assetId: string) => string | undefined
  /** Function to resolve shape by ID for connector bindings */
  resolveShape?: (shapeId: string) => Shape | undefined
  shape: GroupShape
}

export function GroupShapeRenderer({
  shape,
  ...props
}: GroupShapeRendererProps) {
  const editor = useEditor()
  const children = useShapeChildren(editor, shape.id)

  // Check if this group is currently open for editing
  const openedGroupId = useOpenedGroupId(editor)
  const isGroupOpen = openedGroupId === shape.id

  return (
    <Group
      draggable={props.draggable}
      id={shape.id}
      onClick={props.onClick}
      onDblClick={props.onDoubleClick}
      onDragEnd={props.onDragEnd}
      onDragMove={props.onDragMove}
      onDragStart={props.onDragStart}
      onMouseEnter={props.onMouseEnter}
      onMouseLeave={props.onMouseLeave}
      onTap={props.onTap}
      onTransformEnd={props.onTransformEnd}
      opacity={shape.props.opacity ?? 1}
      rotation={shape.props.rotation ?? 0}
      scaleX={shape.props.scaleX ?? 1}
      scaleY={shape.props.scaleY ?? 1}
      x={shape.props.x}
      y={shape.props.y}
    >
      {children.map((child) => (
        <ShapeRenderer
          {...props}
          draggable={isGroupOpen ? props.draggable : false}
          editor={editor}
          key={child.id}
          shape={child}
        />
      ))}
    </Group>
  )
}
