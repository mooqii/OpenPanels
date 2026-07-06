import Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import { useCallback, useEffect, useRef } from "react"
import { HOVER_OUTLINE_COLOR, HOVER_OUTLINE_WIDTH } from "../constants"
import type { Editor } from "../editor"
import type { ShapeId } from "../types/ids"
import type { GeoShape, Shape } from "../types/shapes"

interface UseHoverOptions {
  editor: Editor
  groupRef: React.RefObject<Konva.Group | null>
}

/**
 * Creates the appropriate outline shape for a given shape type.
 * The outline copies the shape's transform (position, size, rotation, scale)
 * to properly follow rotated shapes.
 *
 * For groups, uses Konva's getClientRect() API to calculate bounds.
 * For regular shapes, uses node transform properties to preserve shape-specific outlines.
 */
function createOutlineShape(
  shape: Shape,
  node: Konva.Node
): Konva.Shape | null {
  const strokeWidth = HOVER_OUTLINE_WIDTH
  const layer = node.getLayer()
  if (!layer) return null

  // For group shapes, calculate local bounds then apply group's transform
  if (shape.type === "group") {
    // Get the bounding box in the group's local coordinate system (without its own transform)
    const localRect = node.getClientRect({ skipTransform: true })

    if (localRect.width <= 0 || localRect.height <= 0) return null

    // Get the group's position relative to layer
    const position = node.getAbsolutePosition(layer)

    // The outline rect needs to be positioned at the group's origin,
    // then offset by the local rect's position within the group
    return new Konva.Rect({
      x: position.x,
      y: position.y,
      // Offset to account for children's position within the group
      offsetX: -localRect.x,
      offsetY: -localRect.y,
      width: localRect.width,
      height: localRect.height,
      rotation: node.rotation(),
      scaleX: node.scaleX(),
      scaleY: node.scaleY(),
      stroke: HOVER_OUTLINE_COLOR,
      strokeWidth,
      fill: undefined,
      listening: false,
      strokeScaleEnabled: false,
    })
  }

  // For Line-based shapes (draw, brush, marker), use getClientRect for proper bounds
  // because node.width()/height() don't include stroke width for Lines
  if (
    shape.type === "draw" ||
    shape.type === "brush" ||
    shape.type === "marker"
  ) {
    // Get the bounding box relative to the layer (includes stroke width)
    const clientRect = node.getClientRect({ relativeTo: layer })
    if (clientRect.width <= 0 || clientRect.height <= 0) return null

    return new Konva.Rect({
      x: clientRect.x,
      y: clientRect.y,
      width: clientRect.width,
      height: clientRect.height,
      stroke: HOVER_OUTLINE_COLOR,
      strokeWidth,
      fill: undefined,
      listening: false,
      strokeScaleEnabled: false,
    })
  }

  // For regular shapes, get position relative to layer and preserve transform
  const position = node.getAbsolutePosition(layer)
  const x = position.x
  const y = position.y
  const width = node.width()
  const height = node.height()
  const rotation = node.rotation()
  const scaleX = node.scaleX()
  const scaleY = node.scaleY()

  // For ellipse shapes, draw an ellipse outline
  if (shape.type === "geo") {
    const geoShape = shape as GeoShape
    if (geoShape.props.geo === "ellipse") {
      return new Konva.Ellipse({
        x,
        y,
        radiusX: width / 2,
        radiusY: height / 2,
        rotation,
        scaleX,
        scaleY,
        stroke: HOVER_OUTLINE_COLOR,
        strokeWidth,
        fill: undefined,
        listening: false,
        strokeScaleEnabled: false,
      })
    }

    // For rectangle shapes, try to preserve corner radius
    if (geoShape.props.geo === "rectangle") {
      const props = geoShape.props
      let cornerRadius = 0
      if (props.cornerRadius) {
        cornerRadius = Array.isArray(props.cornerRadius)
          ? Math.max(...props.cornerRadius)
          : props.cornerRadius
      }
      return new Konva.Rect({
        x,
        y,
        width,
        height,
        rotation,
        scaleX,
        scaleY,
        stroke: HOVER_OUTLINE_COLOR,
        strokeWidth,
        fill: undefined,
        cornerRadius,
        listening: false,
        strokeScaleEnabled: false,
      })
    }
  }

  // For placeholder shapes, same as rectangle with optional corner radius
  if (shape.type === "placeholder") {
    const props = shape.props
    const cornerRadius = props.cornerRadius ?? 0
    return new Konva.Rect({
      x,
      y,
      width,
      height,
      rotation,
      scaleX,
      scaleY,
      stroke: HOVER_OUTLINE_COLOR,
      strokeWidth,
      fill: undefined,
      cornerRadius,
      listening: false,
      strokeScaleEnabled: false,
    })
  }

  // For all other shapes, draw a rectangular outline with the same transform
  return new Konva.Rect({
    x,
    y,
    width,
    height,
    rotation,
    scaleX,
    scaleY,
    stroke: HOVER_OUTLINE_COLOR,
    strokeWidth,
    fill: undefined,
    listening: false,
    strokeScaleEnabled: false,
  })
}

export function useHover({ editor, groupRef }: UseHoverOptions) {
  const hoveredShapeIdRef = useRef<ShapeId | null>(null)
  const draggingRef = useRef<boolean>(false)
  const outlineShapeRef = useRef<Konva.Shape | null>(null)
  const selectedShapeIds = editor.useStore((state) => state.selectedShapeIds)
  const openedGroupId = editor.useStore((state) => state.openedGroupId)

  // Clear the outline
  const clearOutline = useCallback(() => {
    const group = groupRef.current
    if (!group) return

    if (outlineShapeRef.current) {
      outlineShapeRef.current.destroy()
      outlineShapeRef.current = null
    }

    group.getLayer()?.batchDraw()
  }, [groupRef])

  const drawOutline = useCallback(() => {
    const group = groupRef.current
    if (!group) return

    const hoveredShapeId = hoveredShapeIdRef.current

    // Clear previous outline
    if (outlineShapeRef.current) {
      outlineShapeRef.current.destroy()
      outlineShapeRef.current = null
    }

    if (!hoveredShapeId || draggingRef.current) {
      clearOutline()
      return
    }

    // Skip if shape is selected
    const selectedIds = editor.getSelectedShapes().map((s) => s.id)
    if (selectedIds.includes(hoveredShapeId)) {
      clearOutline()
      return
    }

    // Skip if shape is the currently opened group
    if (hoveredShapeId === openedGroupId) {
      clearOutline()
      return
    }

    // Get shape and node
    const shape = editor.getShape(hoveredShapeId)
    if (!shape) {
      clearOutline()
      return
    }

    // Skip connectors - they don't need hover outline
    if (shape.type === "connector") {
      clearOutline()
      return
    }

    const node = editor.getShapeNode(hoveredShapeId)
    if (!node) {
      clearOutline()
      return
    }

    // Create and add outline shape
    const outlineShape = createOutlineShape(shape, node)
    if (outlineShape) {
      outlineShapeRef.current = outlineShape
      group.add(outlineShape)
    }

    group.getLayer()?.batchDraw()
  }, [editor, groupRef, clearOutline, openedGroupId])

  // Also redraw when selection changes (to hide outline when shape becomes selected)
  useEffect(() => {
    const hoveredShapeId = hoveredShapeIdRef.current
    if (!hoveredShapeId) return

    if (selectedShapeIds.has(hoveredShapeId)) {
      // Clear outline if hovered shape is now selected
      clearOutline()
    }
  }, [selectedShapeIds, clearOutline])

  // Clear outline when opened group changes (to hide outline for the opened group)
  useEffect(() => {
    const hoveredShapeId = hoveredShapeIdRef.current
    if (!hoveredShapeId) return

    if (hoveredShapeId === openedGroupId) {
      clearOutline()
    }
  }, [openedGroupId, clearOutline])

  const handleMouseEnter = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      const shapeId = e.target.id() as ShapeId
      if (shapeId) {
        // If the shape is inside an opened group, hover it directly
        // Otherwise, if the shape is inside a group, highlight the top-level group instead
        if (editor.isShapeInOpenedGroup(shapeId)) {
          hoveredShapeIdRef.current = shapeId
        } else {
          const topLevelId = editor.getTopLevelAncestor(shapeId)
          hoveredShapeIdRef.current = topLevelId ?? shapeId
        }
        drawOutline()
      }
    },
    [editor, drawOutline]
  )

  const handleMouseLeave = useCallback(() => {
    hoveredShapeIdRef.current = null
    drawOutline()
  }, [drawOutline])

  const handleDragStart = useCallback(() => {
    draggingRef.current = true
    drawOutline()
  }, [drawOutline])

  const handleDragEnd = useCallback(() => {
    draggingRef.current = false
    drawOutline()
  }, [drawOutline])

  return {
    handleMouseEnter,
    handleMouseLeave,
    handleDragStart,
    handleDragEnd,
  }
}
