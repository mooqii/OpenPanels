/**
 * Hook to manage connector bindings and update connectors when bound shapes move.
 * Tracks which connectors are bound to which shapes and triggers updates accordingly.
 *
 * Also provides a ref registry for direct Konva manipulation during transforms,
 * enabling real-time connector updates without React re-renders.
 */

import type Konva from "konva"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import type { Editor } from "../editor"
import type { ShapeId } from "../types/ids"
import type { ConnectorShape, Shape } from "../types/shapes"
import {
  type ConnectorPath,
  calculateConnectorPath,
} from "../utils/connector-path"

/**
 * Build an index of shape IDs to connectors that reference them
 */
function buildBindingIndex(shapes: Shape[]): Map<ShapeId, Set<ShapeId>> {
  const index = new Map<ShapeId, Set<ShapeId>>()

  for (const shape of shapes) {
    if (shape.type !== "connector") continue
    const connector = shape as ConnectorShape

    // Index from-bindings
    for (const binding of connector.props.fromBindings) {
      const existing = index.get(binding.shapeId) ?? new Set()
      existing.add(connector.id)
      index.set(binding.shapeId, existing)
    }

    // Index to-bindings
    for (const binding of connector.props.toBindings) {
      const existing = index.get(binding.shapeId) ?? new Set()
      existing.add(connector.id)
      index.set(binding.shapeId, existing)
    }
  }

  return index
}

/**
 * Get all connector IDs that are bound to any of the given shape IDs
 */
function getConnectorsForShapes(
  shapeIds: ShapeId[],
  bindingIndex: Map<ShapeId, Set<ShapeId>>
): Set<ShapeId> {
  const connectorIds = new Set<ShapeId>()

  for (const shapeId of shapeIds) {
    const connectors = bindingIndex.get(shapeId)
    if (connectors) {
      for (const connectorId of connectors) {
        connectorIds.add(connectorId)
      }
    }
  }

  return connectorIds
}

interface UseConnectorBindingsOptions {
  editor: Editor
}

interface UseConnectorBindingsResult {
  /**
   * Get the Konva Shape ref for a connector
   */
  getConnectorRef: (connectorId: ShapeId) => Konva.Shape | null
  /**
   * Get all connectors that are bound to a specific shape
   */
  getConnectorsForShape: (shapeId: ShapeId) => ConnectorShape[]

  /**
   * Get all connectors that reference any of the given shapes
   */
  getConnectorsForShapes: (shapeIds: ShapeId[]) => ConnectorShape[]

  /**
   * Handle deletion of shapes - removes or updates connectors that reference them
   */
  handleShapesDeleted: (shapeIds: ShapeId[]) => void

  /**
   * Notify that shapes have moved and connectors may need updating.
   * This triggers a re-render of affected connectors.
   */
  notifyShapesMoved: (shapeIds: ShapeId[]) => void

  /**
   * Imperatively redraw connectors affected by the given shape IDs.
   * Reads current positions directly from Konva nodes, bypassing React state.
   * Use this during onTransform/onDragMove for real-time visual updates.
   */
  redrawConnectorsForShapes: (shapeIds: ShapeId[]) => void

  /**
   * Register a connector's Konva Shape ref for direct manipulation.
   * Call with null to unregister when component unmounts.
   */
  registerConnectorRef: (connectorId: ShapeId, ref: Konva.Shape | null) => void
}

/**
 * Hook to manage connector bindings and handle updates when bound shapes change.
 */
export function useConnectorBindings({
  editor,
}: UseConnectorBindingsOptions): UseConnectorBindingsResult {
  // Track the last known positions of shapes for change detection
  const lastPositionsRef = useRef<Map<ShapeId, { x: number; y: number }>>(
    new Map()
  )

  // Registry of connector Konva Shape refs for direct manipulation
  const connectorRefsRef = useRef<Map<ShapeId, Konva.Shape>>(new Map())

  // Version counter to rebuild binding index when connectors are added/removed
  const [indexVersion, setIndexVersion] = useState(0)

  // Subscribe to connector additions/removals to rebuild the index
  useEffect(() => {
    const unsubscribe = editor.listen((diff) => {
      // Check if any connectors were added or removed
      let needsRebuild = false

      for (const record of Object.values(diff.added)) {
        if (
          record.typeName === "shape" &&
          (record as Shape).type === "connector"
        ) {
          needsRebuild = true
          break
        }
      }

      if (!needsRebuild) {
        for (const record of Object.values(diff.removed)) {
          if (
            record.typeName === "shape" &&
            (record as Shape).type === "connector"
          ) {
            needsRebuild = true
            break
          }
        }
      }

      // Also check if connector bindings were updated
      if (!needsRebuild) {
        for (const [, [, to]] of Object.entries(diff.updated)) {
          if (to.typeName === "shape" && (to as Shape).type === "connector") {
            needsRebuild = true
            break
          }
        }
      }

      if (needsRebuild) {
        setIndexVersion((v) => v + 1)
      }
    })

    return unsubscribe
  }, [editor])

  // Build binding index from current shapes
  // biome-ignore lint/correctness/useExhaustiveDependencies: indexVersion triggers rebuild when connectors change
  const bindingIndex = useMemo(() => {
    const shapes = editor.getCurrentPageShapes()
    return buildBindingIndex(shapes)
  }, [editor, indexVersion])

  // Get connectors for a single shape
  const getConnectorsForShapeCallback = useCallback(
    (shapeId: ShapeId): ConnectorShape[] => {
      const connectorIds = bindingIndex.get(shapeId)
      if (!connectorIds) return []

      const connectors: ConnectorShape[] = []
      for (const connectorId of connectorIds) {
        const shape = editor.getShape(connectorId)
        if (shape?.type === "connector") {
          connectors.push(shape as ConnectorShape)
        }
      }
      return connectors
    },
    [bindingIndex, editor]
  )

  // Get connectors for multiple shapes
  const getConnectorsForShapesCallback = useCallback(
    (shapeIds: ShapeId[]): ConnectorShape[] => {
      const connectorIds = getConnectorsForShapes(shapeIds, bindingIndex)
      const connectors: ConnectorShape[] = []

      for (const connectorId of connectorIds) {
        const shape = editor.getShape(connectorId)
        if (shape?.type === "connector") {
          connectors.push(shape as ConnectorShape)
        }
      }

      return connectors
    },
    [bindingIndex, editor]
  )

  // Notify that shapes have moved - connectors will auto-update via React re-render
  // since they read bound shape positions during render
  const notifyShapesMoved = useCallback(
    (shapeIds: ShapeId[]) => {
      // The connector renderer automatically reads the current shape positions,
      // so we just need to ensure a re-render happens.
      // This is handled by the editor's change notification system.

      // Update last known positions
      for (const shapeId of shapeIds) {
        const shape = editor.getShape(shapeId)
        if (shape) {
          const props = shape.props as Record<string, unknown>
          lastPositionsRef.current.set(shapeId, {
            x: (props.x as number) ?? 0,
            y: (props.y as number) ?? 0,
          })
        }
      }
    },
    [editor]
  )

  // Handle deletion of shapes - remove bindings or delete orphaned connectors
  const handleShapesDeleted = useCallback(
    (deletedShapeIds: ShapeId[]) => {
      const deletedSet = new Set(deletedShapeIds)

      // Get all connectors directly from current shapes (not from potentially stale index)
      // This ensures newly created connectors are also checked
      const allShapes = editor.getCurrentPageShapes()
      const connectors = allShapes.filter(
        (s): s is ConnectorShape => s.type === "connector"
      )

      for (const connector of connectors) {
        // Check if this connector references any deleted shapes
        const hasDeletedFromBinding = connector.props.fromBindings.some((b) =>
          deletedSet.has(b.shapeId)
        )
        const hasDeletedToBinding = connector.props.toBindings.some((b) =>
          deletedSet.has(b.shapeId)
        )

        // Skip if this connector doesn't reference any deleted shapes
        if (!(hasDeletedFromBinding || hasDeletedToBinding)) continue

        // Filter out deleted shapes from bindings
        const newFromBindings = connector.props.fromBindings.filter(
          (b) => !deletedSet.has(b.shapeId)
        )
        const newToBindings = connector.props.toBindings.filter(
          (b) => !deletedSet.has(b.shapeId)
        )

        // If connector has no remaining bindings on either end, delete it
        if (newFromBindings.length === 0 || newToBindings.length === 0) {
          editor.deleteShape(connector.id)
        } else {
          // Update connector with remaining bindings
          editor.updateShape<ConnectorShape>(connector.id, {
            props: {
              ...connector.props,
              fromBindings: newFromBindings,
              toBindings: newToBindings,
            },
          })
        }
      }

      // Clean up last positions
      for (const shapeId of deletedShapeIds) {
        lastPositionsRef.current.delete(shapeId)
      }
    },
    [editor]
  )

  // Subscribe to shape changes and update connectors accordingly
  useEffect(() => {
    const unsubscribe = editor.listen((diff) => {
      // Get IDs of updated shapes (excluding connectors)
      const updatedShapeIds: ShapeId[] = []

      for (const [id, [_from, to]] of Object.entries(diff.updated)) {
        if (to.typeName === "shape" && (to as Shape).type !== "connector") {
          updatedShapeIds.push(id as ShapeId)
        }
      }

      if (updatedShapeIds.length > 0) {
        notifyShapesMoved(updatedShapeIds)
      }

      // Handle deleted shapes
      const deletedShapeIds = Object.keys(diff.removed)
        .filter((id) => {
          const record = diff.removed[id]
          return (
            record.typeName === "shape" &&
            (record as Shape).type !== "connector"
          )
        })
        .map((id) => id as ShapeId)

      if (deletedShapeIds.length > 0) {
        handleShapesDeleted(deletedShapeIds)
      }
    })

    return unsubscribe
  }, [editor, notifyShapesMoved, handleShapesDeleted])

  // Register a connector's Konva Shape ref
  const registerConnectorRef = useCallback(
    (connectorId: ShapeId, ref: Konva.Shape | null) => {
      if (ref) {
        connectorRefsRef.current.set(connectorId, ref)
      } else {
        connectorRefsRef.current.delete(connectorId)
      }
    },
    []
  )

  // Get the Konva Shape ref for a connector
  const getConnectorRef = useCallback((connectorId: ShapeId) => {
    return connectorRefsRef.current.get(connectorId) ?? null
  }, [])

  // Imperatively redraw connectors affected by the given shape IDs
  // This reads positions directly from Konva nodes, bypassing React state
  const redrawConnectorsForShapes = useCallback(
    (shapeIds: ShapeId[]) => {
      const affectedConnectorIds = getConnectorsForShapes(
        shapeIds,
        bindingIndex
      )
      if (affectedConnectorIds.size === 0) return

      let needsBatchDraw = false

      for (const connectorId of affectedConnectorIds) {
        const connector = editor.getShape(connectorId) as
          | ConnectorShape
          | undefined
        if (!connector) continue

        const konvaShape = connectorRefsRef.current.get(connectorId)
        if (!konvaShape) continue

        // Read current positions from Konva nodes (not React state)
        const fromShapes = connector.props.fromBindings
          .map((b) => getShapeFromKonvaNode(editor, b.shapeId))
          .filter((s): s is Shape => s !== null)

        const toShapes = connector.props.toBindings
          .map((b) => getShapeFromKonvaNode(editor, b.shapeId))
          .filter((s): s is Shape => s !== null)

        if (fromShapes.length === 0 || toShapes.length === 0) continue

        // Recalculate connector path with current Konva positions
        const fromAnchors = connector.props.fromBindings.map((b) => b.anchor)
        const toAnchors = connector.props.toBindings.map((b) => b.anchor)
        const connectorPath = calculateConnectorPath(
          fromShapes,
          toShapes,
          fromAnchors,
          toAnchors
        )

        // Update the connector's scene function data and trigger redraw
        updateConnectorSceneData(konvaShape, connector, connectorPath)
        needsBatchDraw = true
      }

      // Batch draw all affected connectors at once for performance
      if (needsBatchDraw) {
        const layer = editor.stage?.findOne("Layer")
        if (layer) {
          ;(layer as Konva.Layer).batchDraw()
        }
      }
    },
    [bindingIndex, editor]
  )

  return {
    getConnectorsForShape: getConnectorsForShapeCallback,
    getConnectorsForShapes: getConnectorsForShapesCallback,
    notifyShapesMoved,
    handleShapesDeleted,
    registerConnectorRef,
    getConnectorRef,
    redrawConnectorsForShapes,
  }
}

/**
 * Check if a shape is a connector
 */
export function isConnectorShape(shape: Shape): shape is ConnectorShape {
  return shape.type === "connector"
}

/**
 * Get all shapes that a connector is bound to
 */
export function getConnectorBoundShapes(
  connector: ConnectorShape,
  getShape: (id: ShapeId) => Shape | undefined
): { fromShapes: Shape[]; toShapes: Shape[] } {
  const fromShapes = connector.props.fromBindings
    .map((b) => getShape(b.shapeId))
    .filter((s): s is Shape => s !== undefined)

  const toShapes = connector.props.toBindings
    .map((b) => getShape(b.shapeId))
    .filter((s): s is Shape => s !== undefined)

  return { fromShapes, toShapes }
}

/**
 * Get a shape object with current position read from Konva node.
 * This returns a virtual shape object that reflects the current visual position,
 * which may differ from React state during transforms.
 */
function getShapeFromKonvaNode(editor: Editor, shapeId: ShapeId): Shape | null {
  const shape = editor.getShape(shapeId)
  if (!shape) return null

  const node = editor.getShapeNode(shapeId)
  if (!node) return shape // Fall back to state if no node

  // Create a virtual shape with current Konva node positions
  const props = shape.props as Record<string, unknown>
  return {
    ...shape,
    props: {
      ...props,
      x: node.x(),
      y: node.y(),
      width: node.width() || (props.width as number) || 100,
      height: node.height() || (props.height as number) || 100,
      scaleX: node.scaleX(),
      scaleY: node.scaleY(),
      rotation: node.rotation(),
    },
  } as Shape
}

/**
 * Update a connector's Konva Shape with new path data.
 * This directly manipulates Konva's scene function data to trigger a visual update
 * without going through React's render cycle.
 */
function updateConnectorSceneData(
  konvaShape: Konva.Shape,
  connector: ConnectorShape,
  connectorPath: ConnectorPath
): void {
  const props = connector.props

  // Store the new path data on the Konva shape for the sceneFunc to use
  // We use a custom attribute to pass the updated path
  ;(konvaShape as any).__connectorPath = connectorPath
  ;(konvaShape as any).__connectorProps = props

  // Mark the shape as needing redraw
  // The sceneFunc will be called again with the new data
  konvaShape.getLayer()?.batchDraw()
}
