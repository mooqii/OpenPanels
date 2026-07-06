import type Konva from "konva"
import type { Editor } from "../editor"
import type { Asset } from "../types/assets"
import type { ShapeId } from "../types/ids"
import type {
  GroupShape,
  PlaceholderShape,
  Shape,
  TextShapeProps,
} from "../types/shapes"
import { BrushShapeRenderer } from "./BrushShapeRenderer"
import { ConnectorShapeRenderer } from "./ConnectorShapeRenderer"
import { DrawShapeRenderer } from "./DrawShapeRenderer"
import { GeoEllipseRenderer } from "./GeoEllipseRenderer"
import { GeoLineRenderer } from "./GeoLineRenderer"
import { GeoRectangleRenderer } from "./GeoRectangleRenderer"
import { GroupShapeRenderer } from "./GroupShapeRenderer"
import { ImageShapeRenderer } from "./ImageShapeRenderer"
import { MarkerShapeRenderer } from "./MarkerShapeRenderer"
import { PathShapeRenderer } from "./PathShapeRenderer"
import { PlaceholderShapeRenderer } from "./PlaceholderShapeRenderer"
import { TextShapeRenderer } from "./TextShapeRenderer"

interface ShapeRendererProps {
  asset?: Asset
  /** Natural size of source image when cropping (for proper display) */
  cropNaturalSize?: { width: number; height: number } | null
  draggable?: boolean
  editingShapeId?: ShapeId | null
  editingTextNodeRef?: (node: Konva.Text | null) => void
  editingTextProps?: TextShapeProps | null
  /** Editor instance for connector subscriptions */
  editor?: Editor
  /** Whether this shape is currently being cropped (for image shapes) */
  isCropping?: boolean
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
  shape: Shape
}

export function ShapeRenderer(props: ShapeRendererProps) {
  const { shape, resolveAsset, resolveShape, registerConnectorRef, editor } =
    props
  const isEditingTextShape =
    shape.type === "text" && props.editingShapeId === shape.id

  // Route to appropriate renderer based on shape type
  switch (shape.type) {
    case "geo": {
      const geoShape = shape as any
      const geoType = geoShape.props?.geo

      // Route to specific geo renderer based on geo type
      switch (geoType) {
        case "ellipse":
          return (
            <GeoEllipseRenderer
              {...props}
              resolveAsset={resolveAsset}
              shape={geoShape}
            />
          )
        case "line":
          return (
            <GeoLineRenderer
              {...props}
              resolveAsset={resolveAsset}
              shape={geoShape}
            />
          )
        case "rectangle":
          return (
            <GeoRectangleRenderer
              {...props}
              resolveAsset={resolveAsset}
              shape={geoShape}
            />
          )
        default:
          return null
      }
    }
    case "image":
      return (
        <ImageShapeRenderer
          {...props}
          cropNaturalSize={props.cropNaturalSize}
          isCropping={props.isCropping}
          shape={shape as any}
        />
      )
    case "text":
      return (
        <TextShapeRenderer
          {...props}
          isEditing={isEditingTextShape}
          shape={
            isEditingTextShape && props.editingTextProps
              ? {
                  ...shape,
                  props: props.editingTextProps,
                }
              : (shape as any)
          }
          textNodeRef={
            isEditingTextShape ? props.editingTextNodeRef : undefined
          }
        />
      )
    case "draw":
      return <DrawShapeRenderer {...props} shape={shape as any} />
    case "path":
      return <PathShapeRenderer {...props} shape={shape as any} />
    case "brush":
      return <BrushShapeRenderer {...props} shape={shape as any} />
    case "marker":
      return <MarkerShapeRenderer {...props} shape={shape as any} />
    case "connector":
      return (
        <ConnectorShapeRenderer
          {...props}
          editor={editor}
          registerRef={registerConnectorRef}
          resolveShape={resolveShape}
          shape={shape as any}
        />
      )
    case "group":
      return <GroupShapeRenderer {...props} shape={shape as GroupShape} />
    case "placeholder":
      return (
        <PlaceholderShapeRenderer
          {...props}
          shape={shape as PlaceholderShape}
        />
      )
    default: {
      const exhaustiveCheck: never = shape
      console.warn(`Unknown shape type: ${(exhaustiveCheck as Shape).type}`)
      return null
    }
  }
}
