/**
 * Hook for creating connectors between shapes using an interactive tool.
 *
 * Simple drag-to-connect flow:
 * 1. Select the connector tool
 * 2. Mousedown on source shape, drag to target shape, release to create connector
 *
 * Advanced multi-source/target flow:
 * 1. Select the connector tool
 * 2. Shift+click to add multiple sources
 * 3. Click on targets (without Shift)
 * 4. Double-click or press Enter to finish the connector
 * 5. Press Escape to cancel
 */

import type { KonvaEventObject } from "konva/lib/Node"
import { useCallback, useEffect, useState } from "react"
import type { Editor } from "../editor"
import type { ShapeId } from "../types/ids"
import type {
  ConnectorAnchor,
  ConnectorBinding,
  ConnectorShape,
  Shape,
} from "../types/shapes"

interface ConnectorToolState {
  /** The source shape where drag started (for drag mode) */
  dragSourceShapeId: ShapeId | null
  /** Source shape bindings being collected */
  fromBindings: ConnectorBinding[]
  /** Shape currently being hovered (for preview indicator) */
  hoveredShapeId: ShapeId | null
  /** Whether we're actively creating a connector */
  isConnecting: boolean
  /** Whether we're in drag mode (mousedown -> drag -> mouseup) */
  isDragging: boolean
  /** Current phase: 'from' (collecting sources) or 'to' (collecting targets) */
  phase: "from" | "to"
  /** Target shape bindings being collected */
  toBindings: ConnectorBinding[]
}

const initialState: ConnectorToolState = {
  isConnecting: false,
  isDragging: false,
  dragSourceShapeId: null,
  fromBindings: [],
  toBindings: [],
  phase: "from",
  hoveredShapeId: null,
}

interface UseConnectorToolOptions {
  /** Default stroke color for new connectors */
  defaultStroke?: string
  /** Default stroke width for new connectors */
  defaultStrokeWidth?: number
  editor: Editor
}

interface UseConnectorToolResult {
  /** Cancel the current connector creation */
  cancel: () => void
  /** Finish and create the connector */
  finishConnector: () => ConnectorShape | null
  /** Handle double-click to finish connection */
  handleDoubleClick: (e: KonvaEventObject<PointerEvent>) => void
  /** Handle mouse down on the stage */
  handleMouseDown: (e: KonvaEventObject<PointerEvent>) => void
  /** Handle mouse move for preview rendering */
  handleMouseMove: (e: KonvaEventObject<PointerEvent>) => void
  /** Handle mouse up to complete drag connection */
  handleMouseUp: (e: KonvaEventObject<PointerEvent>) => void
  /** ID of the shape currently being hovered */
  hoveredShapeId: ShapeId | null
  /** Whether the tool is active */
  isActive: boolean
  /** Whether we're in drag mode */
  isDragging: boolean
  /** Current mouse position in canvas coordinates (for preview) */
  mousePosition: { x: number; y: number } | null
  /** Resolve shape by ID (for preview component) */
  resolveShape: (shapeId: string) => Shape | undefined
  /** Current canvas scale (for preview component) */
  scale: number
  /** Current state of the connector tool */
  state: ConnectorToolState
  /** Switch from collecting sources to collecting targets */
  switchToTargets: () => void
}

/**
 * Find the shape at a given stage position
 */
function findShapeAtPosition(
  editor: Editor,
  stageX: number,
  stageY: number
): Shape | undefined {
  const stage = editor.stage
  if (!stage) return undefined

  // Get the shape under the pointer
  const target = stage.getIntersection({ x: stageX, y: stageY })
  if (!target) return undefined

  // Get the shape ID from the target
  const shapeId = target.id() as ShapeId
  if (!shapeId) return undefined

  // Get the shape from the editor
  const shape = editor.getShape(shapeId)

  // Don't connect to connectors
  if (shape?.type === "connector") return undefined

  return shape
}

/**
 * Hook to manage the connector creation tool
 */
export function useConnectorTool({
  editor,
  defaultStroke = "#666666",
  defaultStrokeWidth = 2,
}: UseConnectorToolOptions): UseConnectorToolResult {
  const [state, setState] = useState<ConnectorToolState>(initialState)
  const [mousePosition, setMousePosition] = useState<{
    x: number
    y: number
  } | null>(null)

  const tool = editor.getTool()
  const isActive = tool.name === "connector"

  // Cancel the current connector creation
  const cancel = useCallback(() => {
    setState(initialState)
    setMousePosition(null)
  }, [])

  // Switch from collecting sources to collecting targets
  const switchToTargets = useCallback(() => {
    if (state.fromBindings.length === 0) return
    setState((prev) => ({ ...prev, phase: "to" }))
  }, [state.fromBindings.length])

  // Finish and create the connector
  const finishConnector = useCallback((): ConnectorShape | null => {
    if (state.fromBindings.length === 0 || state.toBindings.length === 0) {
      cancel()
      return null
    }

    // Extract shape IDs from bindings
    const fromShapeIds = state.fromBindings.map((b) => b.shapeId)
    const toShapeIds = state.toBindings.map((b) => b.shapeId)

    // Create the connector shape
    const connector = editor.createConnector({
      fromShapeIds,
      toShapeIds,
      stroke: defaultStroke,
      strokeWidth: defaultStrokeWidth,
      arrowEnd: "arrow",
    }) as ConnectorShape

    // Reset state
    setState(initialState)
    setMousePosition(null)

    // Switch back to select tool
    editor.setTool({ name: "select" })

    return connector
  }, [state, editor, defaultStroke, defaultStrokeWidth, cancel])

  // Handle mouse down on stage
  const handleMouseDown = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      if (!isActive) return

      const stage = editor.stage
      if (!stage) return

      // Get the pointer position
      const pointerPos = stage.getPointerPosition()
      if (!pointerPos) return

      // Find shape at position
      const shape = findShapeAtPosition(editor, pointerPos.x, pointerPos.y)
      if (!shape) return

      // Check if this shape is already in the current bindings
      const currentBindings =
        state.phase === "from" ? state.fromBindings : state.toBindings
      const alreadyAdded = currentBindings.some((b) => b.shapeId === shape.id)
      if (alreadyAdded) return

      // Create binding for this shape
      const binding: ConnectorBinding = {
        shapeId: shape.id,
        anchor: "auto" as ConnectorAnchor,
      }

      if (state.phase === "from") {
        // In 'from' phase, add to source bindings
        if (e.evt.shiftKey && state.fromBindings.length > 0) {
          // Shift+click: add additional source (multi-source mode)
          setState((prev) => ({
            ...prev,
            isConnecting: true,
            fromBindings: [...prev.fromBindings, binding],
          }))
        } else if (state.fromBindings.length > 0) {
          // Already have a source, switch to target phase and add this shape as target
          setState((prev) => ({
            ...prev,
            phase: "to",
            toBindings: [...prev.toBindings, binding],
          }))
        } else {
          // First click: start drag mode with this shape as source
          setState((prev) => ({
            ...prev,
            isConnecting: true,
            isDragging: true,
            dragSourceShapeId: shape.id,
            fromBindings: [binding],
            phase: "to", // Immediately switch to target phase for drag
          }))
        }
      } else {
        // In 'to' phase, add to target bindings
        setState((prev) => ({
          ...prev,
          toBindings: [...prev.toBindings, binding],
        }))
      }

      e.cancelBubble = true
    },
    [isActive, editor, state]
  )

  // Handle mouse move - track position for preview
  const handleMouseMove = useCallback(
    (_e: KonvaEventObject<PointerEvent>) => {
      if (!isActive) return

      const stage = editor.stage
      if (!stage) return

      const pointerPos = stage.getPointerPosition()
      if (!pointerPos) return

      const scale = stage.scaleX()
      const stagePos = stage.position()
      setMousePosition({
        x: (pointerPos.x - stagePos.x) / scale,
        y: (pointerPos.y - stagePos.y) / scale,
      })

      // Detect shape under pointer for hover preview
      const hoveredShape = findShapeAtPosition(
        editor,
        pointerPos.x,
        pointerPos.y
      )
      const newHoveredId = hoveredShape?.id ?? null

      // Only update state if hovered shape changed
      setState((prev) => {
        if (prev.hoveredShapeId === newHoveredId) return prev
        return { ...prev, hoveredShapeId: newHoveredId }
      })
    },
    [isActive, editor]
  )

  // Handle mouse up - complete drag connection
  const handleMouseUp = useCallback(
    (_e: KonvaEventObject<PointerEvent>) => {
      // Only handle if we're in drag mode
      if (!(isActive && state.isDragging)) return

      const stage = editor.stage
      if (!stage) return

      const pointerPos = stage.getPointerPosition()
      if (!pointerPos) return

      // Find shape at release position
      const targetShape = findShapeAtPosition(
        editor,
        pointerPos.x,
        pointerPos.y
      )

      // If released on a different valid shape, create connector immediately
      if (targetShape && targetShape.id !== state.dragSourceShapeId) {
        // Extract shape IDs from bindings
        const fromShapeIds = state.fromBindings.map((b) => b.shapeId)
        const toShapeIds = [targetShape.id]

        // Create the connector shape
        editor.createConnector({
          fromShapeIds,
          toShapeIds,
          stroke: defaultStroke,
          strokeWidth: defaultStrokeWidth,
          arrowEnd: "arrow",
        }) as ConnectorShape
      }

      // Reset state but stay in connector tool for next connection
      setState(initialState)
      setMousePosition(null)
    },
    [isActive, state, editor, defaultStroke, defaultStrokeWidth]
  )

  // Handle double-click to finish
  const handleDoubleClick = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      if (!(isActive && state.isConnecting)) return

      const stage = editor.stage
      if (!stage) return

      const pointerPos = stage.getPointerPosition()
      if (!pointerPos) return

      // Find shape at position
      const shape = findShapeAtPosition(editor, pointerPos.x, pointerPos.y)

      if (state.phase === "from") {
        // If still in 'from' phase, need to add at least one target
        if (shape) {
          // Add as target and finish
          const binding: ConnectorBinding = {
            shapeId: shape.id,
            anchor: "auto",
          }
          setState((prev) => ({
            ...prev,
            phase: "to",
            toBindings: [...prev.toBindings, binding],
          }))
        }
        // Wait for state update, then finish
        setTimeout(() => finishConnector(), 0)
      } else {
        // In 'to' phase, finish the connector
        if (shape) {
          const alreadyAdded = state.toBindings.some(
            (b) => b.shapeId === shape.id
          )
          if (!alreadyAdded) {
            const binding: ConnectorBinding = {
              shapeId: shape.id,
              anchor: "auto",
            }
            setState((prev) => ({
              ...prev,
              toBindings: [...prev.toBindings, binding],
            }))
          }
        }
        setTimeout(() => finishConnector(), 0)
      }

      e.cancelBubble = true
    },
    [isActive, state, editor, finishConnector]
  )

  // Resolve shape by ID (for preview component)
  const resolveShape = useCallback(
    (shapeId: string) => {
      return editor.getShape(shapeId as ShapeId)
    },
    [editor]
  )

  // Get current scale
  const scale = editor.stage?.scaleX() ?? 1

  // Handle keyboard events internally
  useEffect(() => {
    if (!isActive) return

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        cancel()
        editor.setTool({ name: "select" })
      } else if (e.key === "Enter") {
        if (state.phase === "from" && state.fromBindings.length > 0) {
          // Switch to target phase
          switchToTargets()
        } else if (state.toBindings.length > 0) {
          // Finish the connector
          finishConnector()
        }
      } else if (e.key === "Tab") {
        e.preventDefault()
        if (state.phase === "from" && state.fromBindings.length > 0) {
          switchToTargets()
        }
      }
    }

    window.addEventListener("keydown", handleKeyDown)
    return () => {
      window.removeEventListener("keydown", handleKeyDown)
    }
  }, [isActive, state, cancel, switchToTargets, finishConnector, editor])

  // Reset state when tool changes away from connector
  useEffect(() => {
    if (!isActive && state.isConnecting) {
      setState(initialState)
      setMousePosition(null)
    }
  }, [isActive, state.isConnecting])

  return {
    state,
    mousePosition,
    handleMouseDown,
    handleMouseMove,
    handleMouseUp,
    handleDoubleClick,
    switchToTargets,
    finishConnector,
    cancel,
    isActive,
    isDragging: state.isDragging,
    resolveShape,
    scale,
    hoveredShapeId: state.hoveredShapeId,
  }
}
