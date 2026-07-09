import type Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import type React from "react"
import { useEffect, useImperativeHandle, useRef } from "react"
import { Group } from "react-konva"
import {
  TRANSFORMER_BORDER_STROKE_WIDTH,
  TRANSFORMER_STROKE_COLOR,
} from "../constants"
import {
  type Box,
  Transformer as TransformerClass,
} from "../shapes/Transformer"

export interface TransformerProps {
  /** Anchor corner radius */
  anchorCornerRadius?: number
  /** Anchor fill color */
  anchorFill?: string
  /** Anchor size (width/height) */
  anchorSize?: number
  /** Anchor stroke color */
  anchorStroke?: string
  /** Anchor stroke width */
  anchorStrokeWidth?: number
  /** Whether border is enabled */
  borderEnabled?: boolean
  /** Border stroke color */
  borderStroke?: string
  /** Border stroke width */
  borderStrokeWidth?: number
  /** Function to constrain the bounding box during transform */
  boundBoxFunc?: (oldBox: Box, newBox: Box) => Box
  /** Enabled anchors */
  enabledAnchors?: string[]
  /** Whether to keep aspect ratio */
  keepRatio?: boolean
  /** Nodes to attach to the transformer */
  nodes?: Konva.Node[]
  /** Callback during transform */
  onTransform?: () => void
  /** Callback when transform ends */
  onTransformEnd?: () => void
  /** Callback when transform starts */
  onTransformStart?: (e: KonvaEventObject<PointerEvent>) => void
  ref: React.RefObject<TransformerClass | null>
  /** Whether resizing is enabled */
  resizeEnabled?: boolean
  /** Rotation snap angles (in degrees) */
  rotationSnaps?: number[]
  /** Rotation snap tolerance in degrees */
  rotationSnapTolerance?: number
}

/**
 * React wrapper for the custom Transformer class (Transformer).
 * This transformer updates width/height directly instead of scaleX/scaleY.
 */
export const Transformer = ({
  nodes,
  resizeEnabled = true,
  borderStroke = TRANSFORMER_STROKE_COLOR,
  borderStrokeWidth = TRANSFORMER_BORDER_STROKE_WIDTH,
  borderEnabled = true,
  anchorFill,
  anchorStroke,
  anchorStrokeWidth,
  anchorSize,
  anchorCornerRadius,
  keepRatio = false,
  rotationSnaps,
  rotationSnapTolerance,
  enabledAnchors,
  onTransformStart,
  onTransform,
  onTransformEnd,
  boundBoxFunc,
  ref,
}: TransformerProps) => {
  const transformerRef = useRef<TransformerClass | null>(null)
  const groupRef = useRef<Konva.Group>(null)

  // Expose the transformer instance via ref
  // biome-ignore lint/correctness/useExhaustiveDependencies: off
  useImperativeHandle(ref, () => transformerRef.current!, [
    transformerRef.current,
  ])

  // Initialize the transformer on mount only
  // biome-ignore lint/correctness/useExhaustiveDependencies: only runs on mount
  useEffect(() => {
    const group = groupRef.current
    if (!group) return

    // Create the transformer with initial values
    const transformer = new TransformerClass({
      resizeEnabled,
      rotateEnabled: true,
      borderStroke,
      borderStrokeWidth,
      borderEnabled,
      anchorFill,
      anchorStroke,
      anchorStrokeWidth,
      anchorSize,
      anchorCornerRadius,
      keepRatio,
      rotationSnaps,
      rotationSnapTolerance,
      enabledAnchors,
      boundBoxFunc,
    })

    // Add event listeners
    if (onTransformStart) {
      transformer.on("transformstart", onTransformStart)
    }
    if (onTransform) {
      transformer.on("transform", onTransform)
    }
    if (onTransformEnd) {
      transformer.on("transformend", onTransformEnd)
    }

    // Add to the group's parent (Layer)
    const layer = group.getLayer()
    if (layer) {
      layer.add(transformer)
    }

    transformerRef.current = transformer

    return () => {
      transformer.destroy()
      transformerRef.current = null
    }
  }, [])

  // Update nodes when they change
  useEffect(() => {
    if (transformerRef.current && nodes) {
      transformerRef.current.setNodes(nodes)
    }
  }, [nodes])

  // Update configuration when props change
  useEffect(() => {
    if (!transformerRef.current) return
    transformerRef.current.resizeEnabled = resizeEnabled
  }, [resizeEnabled])

  useEffect(() => {
    if (!transformerRef.current) return
    transformerRef.current.keepRatio = keepRatio
  }, [keepRatio])

  useEffect(() => {
    if (!transformerRef.current) return
    transformerRef.current.borderStroke = borderStroke
  }, [borderStroke])

  useEffect(() => {
    if (!transformerRef.current) return
    transformerRef.current.borderStrokeWidth = borderStrokeWidth
  }, [borderStrokeWidth])

  useEffect(() => {
    if (!transformerRef.current) return
    transformerRef.current.borderEnabled = borderEnabled
  }, [borderEnabled])

  useEffect(() => {
    if (!transformerRef.current) return
    if (anchorFill !== undefined) transformerRef.current.anchorFill = anchorFill
  }, [anchorFill])

  useEffect(() => {
    if (!transformerRef.current) return
    if (anchorStroke !== undefined)
      transformerRef.current.anchorStroke = anchorStroke
  }, [anchorStroke])

  useEffect(() => {
    if (!transformerRef.current) return
    if (anchorStrokeWidth !== undefined)
      transformerRef.current.anchorStrokeWidth = anchorStrokeWidth
  }, [anchorStrokeWidth])

  useEffect(() => {
    if (!transformerRef.current) return
    if (anchorSize !== undefined) transformerRef.current.anchorSize = anchorSize
  }, [anchorSize])

  useEffect(() => {
    if (!transformerRef.current) return
    if (anchorCornerRadius !== undefined)
      transformerRef.current.anchorCornerRadius = anchorCornerRadius
  }, [anchorCornerRadius])

  useEffect(() => {
    if (!transformerRef.current) return
    if (rotationSnaps !== undefined)
      transformerRef.current.rotationSnaps = rotationSnaps
  }, [rotationSnaps])

  useEffect(() => {
    if (!transformerRef.current) return
    if (rotationSnapTolerance !== undefined)
      transformerRef.current.rotationSnapTolerance = rotationSnapTolerance
  }, [rotationSnapTolerance])

  useEffect(() => {
    if (!transformerRef.current) return
    if (enabledAnchors !== undefined)
      transformerRef.current.enabledAnchors = enabledAnchors
  }, [enabledAnchors])

  useEffect(() => {
    if (!transformerRef.current) return
    transformerRef.current.boundBoxFunc = boundBoxFunc
  }, [boundBoxFunc])

  // Update callbacks when they change
  useEffect(() => {
    if (!transformerRef.current) return
    // Remove old listener and add new one
    transformerRef.current.off("transformstart")
    if (onTransformStart) {
      transformerRef.current.on("transformstart", onTransformStart)
    }
  }, [onTransformStart])

  useEffect(() => {
    if (!transformerRef.current) return
    transformerRef.current.off("transform")
    if (onTransform) {
      transformerRef.current.on("transform", onTransform)
    }
  }, [onTransform])

  useEffect(() => {
    if (!transformerRef.current) return
    transformerRef.current.off("transformend")
    if (onTransformEnd) {
      transformerRef.current.on("transformend", onTransformEnd)
    }
  }, [onTransformEnd])

  // The Group is just a placeholder to access the layer
  // The actual transformer is added directly to the layer
  return <Group listening={false} ref={groupRef} />
}

Transformer.displayName = "Transformer"
