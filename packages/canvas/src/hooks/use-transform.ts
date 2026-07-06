import type Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import React, { useCallback, useMemo, useState } from "react"
import type { Editor } from "../editor"
import type { Transformer } from "../shapes/Transformer"
import {
  getTextBoxWidthMode,
  getTextPropsWithUpdatedTransformLayout,
} from "../text-layout"
import type { ShapeId } from "../types/ids"
import type { ShapeProps } from "../types/shapes"
import { useSelectedShapes } from "./use-editor-state"

/**
 * Hook for managing the custom Transformer.
 * All transform logic (rotation, edge resize, corner resize) is now handled
 * by the Transformer class itself.
 */
export function useTransform(editor: Editor) {
  const transformerRef = React.useRef<Transformer>(null)
  const selectedShapes = useSelectedShapes(editor)

  // Transform state - reactive via callbacks
  const [isTransforming, setIsTransforming] = useState(false)

  // Calculate transformer nodes from selected shapes
  const transformerNodes = useMemo(() => {
    return selectedShapes
      .map((shape) => editor.getShapeNode(shape.id))
      .filter((node): node is Konva.Node => node !== undefined)
  }, [editor, selectedShapes])

  const syncTextNodeLayout = useCallback(
    (node: Konva.Node, activeAnchor?: string | null) => {
      const shape = editor.getShape(node.id() as ShapeId)
      if (!(shape && shape.type === "text")) {
        return null
      }

      const nextWidthMode =
        activeAnchor === "middle-left" || activeAnchor === "middle-right"
          ? "manual"
          : getTextBoxWidthMode(shape.props, "manual")
      const nextProps = getTextPropsWithUpdatedTransformLayout(
        shape.props,
        {
          height: node.height(),
          rotation: node.rotation(),
          scaleX: node.scaleX(),
          scaleY: node.scaleY(),
          width: node.width(),
          x: node.x(),
          y: node.y(),
        },
        nextWidthMode
      )

      if (typeof nextProps.width === "number") {
        node.width(nextProps.width)
      }
      if (typeof nextProps.height === "number") {
        node.height(nextProps.height)
      }

      node.getLayer()?.batchDraw()

      return nextProps
    },
    [editor]
  )

  // Commit transform changes to the editor state
  const handleTransformEnd = useCallback(() => {
    setIsTransforming(false)

    const transformer = transformerRef.current
    if (!transformer) return

    const activeAnchor = transformer.getActiveAnchor()
    const nodes = transformer.getNodes() as Konva.Node[]
    for (const node of nodes) {
      const shape = editor.getShape(node.id() as ShapeId)
      if (!shape) continue

      const updatedProps = {
        x: node.x(),
        y: node.y(),
        scaleX: node.scaleX(),
        scaleY: node.scaleY(),
        rotation: node.rotation(),
      } as Partial<ShapeProps>

      const nodeType = node.getClassName()
      if (nodeType === "Line" || nodeType === "Path") {
        // update scaleX/scaleY for line
      } else if (shape.type === "group") {
        // For groups, only update transform properties (x, y, scale, rotation)
        // Groups don't have fixed width/height - they're determined by children
        // Konva.Group handles child transforms automatically
      } else if (shape.type === "text") {
        const nextProps = syncTextNodeLayout(node, activeAnchor)
        if (!nextProps) continue

        editor.updateShape(shape.id, {
          props: nextProps as Partial<ShapeProps>,
        })
        continue
      } else if (shape.type !== "connector") {
        // update width/height for all other types (excluding connectors which have no dimensions)
        // Cast needed since connector doesn't have width/height but we've excluded it above
        ;(updatedProps as { width?: number; height?: number }).width =
          node.width()
        ;(updatedProps as { width?: number; height?: number }).height =
          node.height()
      }

      editor.updateShape(shape.id, {
        props: updatedProps,
      })
    }

    transformer.update()
  }, [editor, syncTextNodeLayout])

  // Handle transform start - cancel bubble to prevent other handlers
  const handleTransformStart = useCallback(() => {
    setIsTransforming(true)
  }, [])

  // Handle shape drag end (separate from transform)
  const handleShapeDragEnd = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      const node = e.target

      const shape = editor.getShape(node.id() as ShapeId)
      if (!shape) return

      editor.updateShape(shape.id, {
        props: {
          x: node.x(),
          y: node.y(),
        },
      })
    },
    [editor]
  )

  return {
    transformerRef,
    transformerNodes,
    handleTransformStart,
    handleTransformEnd,
    handleShapeDragEnd,
    isTransforming,
    syncTextNodeLayout,
  }
}
