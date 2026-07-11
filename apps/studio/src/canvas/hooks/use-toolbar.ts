import type { IRect } from "konva/lib/types"
import { useCallback, useEffect, useRef, useState } from "react"
import type { Editor } from "../editor"
import type { Transformer } from "../shapes/Transformer"
import type { ShapeId } from "../types/ids"
import type { GeoShapeProps, Shape } from "../types/shapes"
import {
  GEO_ELLIPSE_TOOLS,
  GEO_LINE_TOOLS,
  GEO_TOOLS,
  IMAGE_TOOLS,
  MULTI_TOOLS,
  PATH_TOOLS,
  TEXT_TOOLS,
  type ToolConfig,
} from "../types/tools"
import { useSelectedShapes } from "./use-editor-state"

type TransformerRef = React.RefObject<Transformer | null>

function hasTool(tools: ToolConfig[], toolType: ToolConfig["type"]) {
  return tools.some((tool) => tool.type === toolType)
}

export function normalizeImageTools(tools: ToolConfig[]): ToolConfig[] {
  const cropTool: ToolConfig = { type: "crop" }
  const infoTool: ToolConfig = { type: "info" }
  const nextTools = hasTool(tools, "crop") ? [...tools] : [cropTool, ...tools]

  if (hasTool(nextTools, "info")) {
    return nextTools
  }

  const downloadIndex = nextTools.findIndex((tool) => tool.type === "download")
  if (downloadIndex === -1) {
    return [...nextTools, infoTool]
  }

  return [
    ...nextTools.slice(0, downloadIndex + 1),
    infoTool,
    ...nextTools.slice(downloadIndex + 1),
  ]
}

function getToolsForShapes(editor: Editor, shapes: Shape[]): ToolConfig[] {
  if (shapes.length === 0) return []

  const shape = shapes.length === 1 ? shapes[0] : null
  const shapeType = shape?.type || "transformer"

  // Check for custom tools from editor
  const customTools = editor.getToolsForShape(shapes)
  if (customTools) {
    return shapeType === "image"
      ? normalizeImageTools(customTools)
      : customTools
  }

  switch (shapeType) {
    case "geo": {
      const geo = (shape?.props as GeoShapeProps).geo
      if (geo === "ellipse") {
        return GEO_ELLIPSE_TOOLS
      }
      if (geo === "line") {
        return GEO_LINE_TOOLS
      }
      return GEO_TOOLS
    }
    case "path":
      return PATH_TOOLS
    case "image":
      return normalizeImageTools(IMAGE_TOOLS)
    case "text":
      return TEXT_TOOLS
    case "group":
    case "transformer":
      return MULTI_TOOLS
    default:
      return []
  }
}

export function useToolbar({
  editor,
  transformerRef,
  isTransforming,
  cropShapeId,
}: {
  editor: Editor
  transformerRef: TransformerRef
  isTransforming: boolean
  /** When set, hide toolbar for the shape being cropped */
  cropShapeId?: ShapeId | null
}) {
  const selectedShapes = useSelectedShapes(editor)
  const toolbarRef = useRef<HTMLDivElement | null>(null)

  // Track drag state for hiding toolbar during drag
  const [isDragging, setIsDragging] = useState(false)

  // Get selected shape if single selection
  const selectedShape = selectedShapes.length === 1 ? selectedShapes[0] : null

  // Check if we're in crop mode for the selected shape
  const isInCropMode = cropShapeId != null && selectedShape?.id === cropShapeId

  // Get tools for current selection (empty if in crop mode)
  const tools = isInCropMode ? [] : getToolsForShapes(editor, selectedShapes)

  // Calculate toolbar position above the selected shape
  const calculateToolbarPosition = useCallback((): IRect | null => {
    const transformer = transformerRef.current
    if (!transformer) return null

    // Get transformer's bounding box (already accounts for selection bounds)
    const bounds = transformer.getClientRect()

    return bounds
  }, [transformerRef])

  const update = useCallback(() => {
    const toolbarElement = toolbarRef.current
    if (!toolbarElement) return

    // Hide toolbar if transforming, dragging, or no single selection
    if (isTransforming || isDragging || tools.length === 0) {
      toolbarElement.style.opacity = "0"
      return
    }

    const pos = calculateToolbarPosition()

    if (pos) {
      toolbarElement.style.left = `${pos.x}px`
      toolbarElement.style.top = `${pos.y}px`
      toolbarElement.style.width = `${pos.width}px`
      toolbarElement.style.height = `${pos.height}px`
      toolbarElement.style.opacity = "1"
    } else {
      toolbarElement.style.opacity = "0"
    }
  }, [tools.length, isTransforming, isDragging, calculateToolbarPosition])

  useEffect(() => {
    if (tools.length === 0) return

    update()

    if (!editor.stage) return

    editor.stage.on("dragmove", update)
    editor.stage.on("wheel", update)
    editor.stage.on("scaleXChange", update)
    editor.stage.on("scaleYChange", update)

    return () => {
      editor.stage?.off("dragmove", update)
      editor.stage?.off("wheel", update)
      editor.stage?.off("scaleXChange", update)
      editor.stage?.off("scaleYChange", update)
    }
  }, [tools.length, update, editor.stage])

  // biome-ignore lint/correctness/useExhaustiveDependencies: off
  useEffect(() => {
    update()
  }, [selectedShapes, update])

  // Handle drag start - hide toolbar
  const handleDragStart = useCallback(() => {
    setIsDragging(true)
  }, [])

  // Handle drag end - show toolbar at new position
  const handleDragEnd = useCallback(() => {
    setIsDragging(false)
  }, [])

  return {
    toolbarRef,
    tools,
    selectedShape,
    handleDragStart,
    handleDragEnd,
    update,
  }
}
