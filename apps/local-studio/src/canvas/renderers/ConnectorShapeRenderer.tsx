import type Konva from "konva"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { Group, Shape } from "react-konva"
import type { Editor } from "../editor"
import type { ShapeId } from "../types/ids"
import type { Shape as CanvasShape, ConnectorShape } from "../types/shapes"
import {
  drawBezierSegment,
  drawConnectorPath,
} from "../utils/connector-drawing"
import {
  type ConnectorPath,
  calculateConnectorPath,
  getConnectorBounds,
} from "../utils/connector-path"

interface ConnectorShapeRendererProps {
  /** Editor instance for subscribing to bound shape changes */
  editor?: Editor
  onClick?: (e: Konva.KonvaEventObject<PointerEvent>) => void
  onDragEnd?: (e: Konva.KonvaEventObject<DragEvent>) => void
  onDragMove?: (e: Konva.KonvaEventObject<DragEvent>) => void
  onDragStart?: (e: Konva.KonvaEventObject<DragEvent>) => void
  onTap?: (e: Konva.KonvaEventObject<Event>) => void
  onTransformEnd?: (e: Konva.KonvaEventObject<Event>) => void
  ref?: (node: Konva.Group | null) => void
  /** Callback to register the Konva Shape ref for direct manipulation during transforms */
  registerRef?: (connectorId: ShapeId, ref: Konva.Shape | null) => void
  /** Function to resolve shape by ID for binding resolution */
  resolveShape?: (shapeId: string) => CanvasShape | undefined
  shape: ConnectorShape
}

export function ConnectorShapeRenderer({
  shape,
  onClick,
  onDragStart,
  onDragEnd,
  onDragMove,
  onTap,
  onTransformEnd,
  ref,
  resolveShape,
  registerRef,
  editor,
}: ConnectorShapeRendererProps) {
  const props = shape.props

  // Ref to track the Konva Shape node for registration
  const shapeNodeRef = useRef<Konva.Shape | null>(null)

  // Version counter to force re-render when bound shapes change
  const [boundShapesVersion, setBoundShapesVersion] = useState(0)

  // Get all bound shape IDs
  const boundShapeIds = useMemo(() => {
    const ids = new Set<string>()
    for (const binding of props.fromBindings) {
      ids.add(binding.shapeId)
    }
    for (const binding of props.toBindings) {
      ids.add(binding.shapeId)
    }
    return ids
  }, [props.fromBindings, props.toBindings])

  // Subscribe to changes in bound shapes to trigger re-render
  useEffect(() => {
    if (!editor) return

    const unsubscribe = editor.listen((diff) => {
      // Check if any of our bound shapes were updated
      for (const [id] of Object.entries(diff.updated)) {
        if (boundShapeIds.has(id)) {
          setBoundShapesVersion((v) => v + 1)
          return
        }
      }
    })

    return unsubscribe
  }, [editor, boundShapeIds])

  // Resolve bound shapes
  // Real-time updates during transforms are handled imperatively via redrawConnectorsForShapes()
  // boundShapesVersion triggers recalculation when bound shapes are updated in state
  // biome-ignore lint/correctness/useExhaustiveDependencies: boundShapesVersion is intentionally used as a trigger
  const fromShapes = useMemo(() => {
    if (!resolveShape) return []
    return props.fromBindings
      .map((binding) => resolveShape(binding.shapeId))
      .filter((s): s is CanvasShape => s !== undefined)
  }, [props.fromBindings, resolveShape, boundShapesVersion])

  // biome-ignore lint/correctness/useExhaustiveDependencies: boundShapesVersion is intentionally used as a trigger
  const toShapes = useMemo(() => {
    if (!resolveShape) return []
    return props.toBindings
      .map((binding) => resolveShape(binding.shapeId))
      .filter((s): s is CanvasShape => s !== undefined)
  }, [props.toBindings, resolveShape, boundShapesVersion])

  // Calculate connector path
  const connectorPath: ConnectorPath | null = useMemo(() => {
    if (fromShapes.length === 0 || toShapes.length === 0) {
      return null
    }

    const fromAnchors = props.fromBindings.map((b) => b.anchor)
    const toAnchors = props.toBindings.map((b) => b.anchor)

    return calculateConnectorPath(fromShapes, toShapes, fromAnchors, toAnchors)
  }, [fromShapes, toShapes, props.fromBindings, props.toBindings])

  // Get bounds for positioning
  const bounds = useMemo(() => {
    if (!connectorPath) return { x: 0, y: 0, width: 100, height: 100 }
    return getConnectorBounds(connectorPath)
  }, [connectorPath])

  // Scene function to draw the connector
  // Supports both React-managed path and imperatively cached path for real-time updates
  const sceneFunc = useCallback(
    (ctx: Konva.Context, konvaShape: Konva.Shape) => {
      // Check for imperatively cached path data first (used during transforms)
      const cachedPath = (konvaShape as any).__connectorPath as
        | ConnectorPath
        | undefined
      const cachedProps = (konvaShape as any).__connectorProps as
        | typeof props
        | undefined

      // Use cached data if available, otherwise use React-computed path
      const pathToRender = cachedPath || connectorPath
      const propsToUse = cachedProps || props

      if (!pathToRender) return

      const nativeCtx = (
        ctx as unknown as { _context: CanvasRenderingContext2D }
      )._context

      // Use shared drawing utility
      drawConnectorPath(nativeCtx, pathToRender, propsToUse)

      // Clear cached data after render (will be set again during next transform)
      // This ensures React-computed path is used when transform is complete
      if (cachedPath) {
        ;(konvaShape as any).__connectorPath = undefined
        ;(konvaShape as any).__connectorProps = undefined
      }
    },
    [connectorPath, props]
  )

  // Hit function for selection - creates a wider hit area around the bezier curves
  const hitFunc = useCallback(
    (ctx: Konva.Context, konvaShape: Konva.Shape) => {
      if (!connectorPath) return

      const nativeCtx = (
        ctx as unknown as { _context: CanvasRenderingContext2D }
      )._context
      const hitWidth = Math.max(props.strokeWidth || 2, 10) + 10

      nativeCtx.lineWidth = hitWidth
      nativeCtx.lineCap = "round"
      nativeCtx.lineJoin = "round"

      ctx.beginPath()

      // Draw hit area for all segments
      for (const segment of connectorPath.fromSegments) {
        drawBezierSegment(nativeCtx, segment)
      }
      for (const segment of connectorPath.toSegments) {
        drawBezierSegment(nativeCtx, segment)
      }

      ctx.fillStrokeShape(konvaShape)
    },
    [connectorPath, props.strokeWidth]
  )

  // Handle ref to set up getSelfRect for proper bounds calculation
  // and register the Shape node for direct manipulation during transforms
  const handleRef = useCallback(
    (node: Konva.Group | null) => {
      if (node) {
        // Find the Shape child and set getSelfRect
        const shapeNode = node.findOne(".connector-path") as
          | Konva.Shape
          | undefined
        if (shapeNode) {
          shapeNode.getSelfRect = () => {
            const padding =
              (props.arrowSize ?? (props.strokeWidth || 2) * 3) + 5
            return {
              x: bounds.x - padding,
              y: bounds.y - padding,
              width: bounds.width + padding * 2,
              height: bounds.height + padding * 2,
            }
          }

          // Store ref for registration
          shapeNodeRef.current = shapeNode

          // Register with the connector bindings system for direct manipulation
          registerRef?.(shape.id as ShapeId, shapeNode)
        }
      } else if (shapeNodeRef.current) {
        // Unregister when unmounting
        registerRef?.(shape.id as ShapeId, null)
        shapeNodeRef.current = null
      }
      if (typeof ref === "function") ref(node)
    },
    [ref, bounds, props.arrowSize, props.strokeWidth, shape.id, registerRef]
  )

  // Clean up registration on unmount
  useEffect(() => {
    return () => {
      if (shapeNodeRef.current) {
        registerRef?.(shape.id as ShapeId, null)
      }
    }
  }, [shape.id, registerRef])

  // Don't render if we can't resolve the shapes
  if (!connectorPath || fromShapes.length === 0 || toShapes.length === 0) {
    return null
  }

  return (
    <Group
      draggable={false}
      id={shape.id}
      onClick={onClick}
      onDragEnd={onDragEnd}
      onDragMove={onDragMove}
      onDragStart={onDragStart}
      onTap={onTap}
      onTransformEnd={onTransformEnd}
      ref={handleRef}
      transformsEnabled="none"
    >
      <Shape
        draggable={false}
        hitFunc={hitFunc}
        id={shape.id}
        listening={true}
        name="connector-path"
        opacity={props.opacity ?? 1}
        sceneFunc={sceneFunc}
        stroke={props.stroke || "#666666"}
        strokeWidth={props.strokeWidth || 2}
        transformsEnabled="none"
      />
    </Group>
  )
}
