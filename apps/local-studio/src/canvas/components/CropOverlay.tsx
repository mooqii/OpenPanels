import type Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { Circle, Group, Line, Rect, Shape } from "react-konva"
import type { CropRect } from "../hooks/use-crop"
import type { ImageShape } from "../types/shapes"

/** Display rect in screen coordinates */
interface DisplayRect {
  height: number
  width: number
  x: number
  y: number
}

const HANDLE_SIZE = 10
const BORDER_WIDTH = 1
const DEFAULT_CROP_COLORS = {
  border: "CanvasText",
  grid: "CanvasText",
  handleFill: "Canvas",
  handleStroke: "CanvasText",
  overlay: "transparent",
}

interface CropOverlayProps {
  /** Current aspect ratio (width/height) when locked */
  aspectRatio?: number | null
  /** Whether aspect ratio is locked */
  aspectRatioLock?: boolean
  /** Current crop rectangle in image source coordinates */
  cropRect: CropRect
  /** Image natural height */
  imageHeight: number
  /** Image natural width */
  imageWidth: number
  /** Callback when crop rect changes */
  onCropRectChange: (rect: CropRect) => void
  /** Current stage scale for consistent handle sizes */
  scale?: number
  /** The image shape being cropped */
  shape: ImageShape
}

function readCropOverlayColors() {
  return {
    overlay: readCssColor("--canvas-crop-overlay", "transparent"),
    grid: readCssColor("--canvas-crop-grid", "CanvasText"),
    handleFill: readCssColor("--canvas-crop-handle", "Canvas"),
    handleStroke: readCssColor("--canvas-crop-handle-stroke", "CanvasText"),
    border: readCssColor("--canvas-crop-border", "CanvasText"),
  }
}

function readCssColor(name: string, fallback: string) {
  if (typeof document === "undefined") return fallback
  const probe = document.createElement("span")
  probe.style.color = `var(${name}, ${fallback})`
  probe.style.position = "absolute"
  probe.style.visibility = "hidden"
  document.body.appendChild(probe)
  const color = getComputedStyle(probe).color
  probe.remove()
  return color || fallback
}

type HandlePosition =
  | "top-left"
  | "top-right"
  | "bottom-left"
  | "bottom-right"
  | "top"
  | "right"
  | "bottom"
  | "left"

export function CropOverlay({
  shape,
  cropRect,
  onCropRectChange,
  imageWidth,
  imageHeight,
  scale = 1,
  aspectRatioLock = false,
  aspectRatio = null,
}: CropOverlayProps) {
  const isDraggingRef = useRef(false)
  const dragStartRef = useRef<{
    rect: CropRect
    mouseX: number
    mouseY: number
    handle?: HandlePosition
  } | null>(null)

  // Refs for imperative Konva node updates
  const groupRef = useRef<Konva.Group>(null)
  const displayCropRectRef = useRef<DisplayRect>({
    x: 0,
    y: 0,
    width: 0,
    height: 0,
  })
  const borderRectRef = useRef<Konva.Rect>(null)
  const lineRefs = useRef<(Konva.Line | null)[]>([null, null, null, null])
  const handleRefs = useRef<(Konva.Circle | null)[]>([null, null, null, null])

  // Shape transform properties
  const shapeX = shape.props.x ?? 0
  const shapeY = shape.props.y ?? 0
  const shapeWidth = shape.props.width ?? imageWidth
  const shapeHeight = shape.props.height ?? imageHeight
  const shapeRotation = shape.props.rotation ?? 0

  // Get existing crop from shape (if any)
  const existingCrop = shape.props.crop as CropRect | undefined

  // Calculate display dimensions for the FULL image
  // When editing an existing crop, we expand to show the full image
  const displayDimensions = useMemo(() => {
    if (!existingCrop) {
      // No existing crop - shape shows full image at its current size
      return {
        x: shapeX,
        y: shapeY,
        width: shapeWidth,
        height: shapeHeight,
        scaleX: shapeWidth / imageWidth,
        scaleY: shapeHeight / imageHeight,
      }
    }

    // Existing crop - calculate full image dimensions based on current crop display
    // The shape currently shows existingCrop.width x existingCrop.height of source
    // at shapeWidth x shapeHeight display size
    const displayScaleX = shapeWidth / existingCrop.width
    const displayScaleY = shapeHeight / existingCrop.height

    // Full image at this scale
    const fullWidth = imageWidth * displayScaleX
    const fullHeight = imageHeight * displayScaleY

    // Position: offset so the cropped region aligns with current shape position
    const offsetX = existingCrop.x * displayScaleX
    const offsetY = existingCrop.y * displayScaleY

    return {
      x: shapeX - offsetX,
      y: shapeY - offsetY,
      width: fullWidth,
      height: fullHeight,
      scaleX: displayScaleX,
      scaleY: displayScaleY,
    }
  }, [
    existingCrop,
    shapeX,
    shapeY,
    shapeWidth,
    shapeHeight,
    imageWidth,
    imageHeight,
  ])

  // Scale from image source coords to display coords
  const scaleX = displayDimensions.scaleX
  const scaleY = displayDimensions.scaleY

  // Convert crop rect from source to display coords (memoized for hook deps)
  const displayCropRect = useMemo(
    () => ({
      x: cropRect.x * scaleX,
      y: cropRect.y * scaleY,
      width: cropRect.width * scaleX,
      height: cropRect.height * scaleY,
    }),
    [cropRect.x, cropRect.y, cropRect.width, cropRect.height, scaleX, scaleY]
  )

  // Sync displayCropRectRef when not dragging (so toolbar/prop changes apply)
  if (!isDraggingRef.current) {
    displayCropRectRef.current = displayCropRect
  }

  // Handle size adjusted for zoom
  const handleRadius = HANDLE_SIZE / 2 / scale
  const borderWidth = BORDER_WIDTH / scale
  const gridWidth = 1 / scale
  const [cropColors, setCropColors] = useState(DEFAULT_CROP_COLORS)

  useEffect(() => {
    const updateColors = () => setCropColors(readCropOverlayColors())
    updateColors()
    const observer = new MutationObserver(updateColors)
    observer.observe(document.documentElement, {
      attributeFilter: ["class", "data-theme"],
      attributes: true,
    })
    return () => observer.disconnect()
  }, [])

  // Minimum crop size
  const minSize = 10

  /**
   * Clamp crop rect to image bounds, preserving the correct edges based on
   * which handle is being dragged. This prevents the opposite edge from moving
   * when a resize hits the image boundary.
   */
  const clampCropRect = useCallback(
    (rect: CropRect, handle?: HandlePosition): CropRect => {
      let { x, y, width, height } = rect

      // Determine which edges are anchored based on handle
      const anchorLeft =
        handle === "right" ||
        handle === "top-right" ||
        handle === "bottom-right"
      const anchorRight =
        handle === "left" || handle === "top-left" || handle === "bottom-left"
      const anchorTop =
        handle === "bottom" ||
        handle === "bottom-left" ||
        handle === "bottom-right"
      const anchorBottom =
        handle === "top" || handle === "top-left" || handle === "top-right"

      // For rect drag (no handle), use the default behavior
      const isRectDrag = !handle

      // Clamp horizontal
      if (anchorLeft) {
        // Left edge (x) is fixed, clamp width
        width = Math.max(minSize, Math.min(width, imageWidth - x))
      } else if (anchorRight) {
        // Right edge is fixed, clamp x and adjust width to preserve right edge
        const rightEdge = x + width
        x = Math.max(0, x)
        if (x === 0 && width > rightEdge) {
          width = rightEdge
        }
        width = Math.max(minSize, width)
        // Ensure right edge doesn't exceed image
        if (x + width > imageWidth) {
          width = imageWidth - x
        }
      } else if (isRectDrag) {
        // Rect drag: clamp position without changing size
        x = Math.max(0, Math.min(x, imageWidth - width))
      }

      // Clamp vertical
      if (anchorTop) {
        // Top edge (y) is fixed, clamp height
        height = Math.max(minSize, Math.min(height, imageHeight - y))
      } else if (anchorBottom) {
        // Bottom edge is fixed, clamp y and adjust height to preserve bottom edge
        const bottomEdge = y + height
        y = Math.max(0, y)
        if (y === 0 && height > bottomEdge) {
          height = bottomEdge
        }
        height = Math.max(minSize, height)
        // Ensure bottom edge doesn't exceed image
        if (y + height > imageHeight) {
          height = imageHeight - y
        }
      } else if (isRectDrag) {
        // Rect drag: clamp position without changing size
        y = Math.max(0, Math.min(y, imageHeight - height))
      }

      // Final safety clamps
      width = Math.max(minSize, Math.min(width, imageWidth - x))
      height = Math.max(minSize, Math.min(height, imageHeight - y))

      return { x, y, width, height }
    },
    [imageWidth, imageHeight]
  )

  /**
   * Imperatively update all Konva nodes to match the given display rect,
   * then trigger a layer redraw. This eliminates React re-render delay
   * so the overlay, border, grid, and handles all move together.
   */
  const applyDisplayRect = useCallback((rect: DisplayRect) => {
    // Update the ref (used by Shape's sceneFunc)
    displayCropRectRef.current = rect

    // Update border rect
    const borderRect = borderRectRef.current
    if (borderRect) {
      borderRect.position({ x: rect.x, y: rect.y })
      borderRect.width(rect.width)
      borderRect.height(rect.height)
    }

    // Update grid lines (rule of thirds)
    const thirdWidth = rect.width / 3
    const thirdHeight = rect.height / 3
    const lines = lineRefs.current

    // Vertical line 1
    if (lines[0]) {
      lines[0].points([
        rect.x + thirdWidth,
        rect.y,
        rect.x + thirdWidth,
        rect.y + rect.height,
      ])
    }
    // Vertical line 2
    if (lines[1]) {
      lines[1].points([
        rect.x + thirdWidth * 2,
        rect.y,
        rect.x + thirdWidth * 2,
        rect.y + rect.height,
      ])
    }
    // Horizontal line 1
    if (lines[2]) {
      lines[2].points([
        rect.x,
        rect.y + thirdHeight,
        rect.x + rect.width,
        rect.y + thirdHeight,
      ])
    }
    // Horizontal line 2
    if (lines[3]) {
      lines[3].points([
        rect.x,
        rect.y + thirdHeight * 2,
        rect.x + rect.width,
        rect.y + thirdHeight * 2,
      ])
    }

    // Update handle positions (4 corners)
    const handles = handleRefs.current
    // top-left
    if (handles[0]) {
      handles[0].position({ x: rect.x, y: rect.y })
    }
    // top-right
    if (handles[1]) {
      handles[1].position({ x: rect.x + rect.width, y: rect.y })
    }
    // bottom-left
    if (handles[2]) {
      handles[2].position({ x: rect.x, y: rect.y + rect.height })
    }
    // bottom-right
    if (handles[3]) {
      handles[3].position({ x: rect.x + rect.width, y: rect.y + rect.height })
    }

    // Trigger layer redraw so the Shape's sceneFunc runs with the updated ref
    const layer = groupRef.current?.getLayer()
    layer?.batchDraw()
  }, [])

  // Handle dragging the crop rect
  const handleRectDragStart = useCallback(
    (e: KonvaEventObject<DragEvent>) => {
      isDraggingRef.current = true
      const stage = e.target.getStage()
      if (!stage) return

      const pos = stage.getPointerPosition()
      if (!pos) return

      dragStartRef.current = {
        rect: { ...cropRect },
        mouseX: pos.x,
        mouseY: pos.y,
      }
      e.cancelBubble = true
    },
    [cropRect]
  )

  const handleRectDragMove = useCallback(
    (e: KonvaEventObject<DragEvent>) => {
      if (!(isDraggingRef.current && dragStartRef.current)) return

      const stage = e.target.getStage()
      if (!stage) return

      const pos = stage.getPointerPosition()
      if (!pos) return

      // Calculate delta in display coords, then convert to source coords
      const deltaX = (pos.x - dragStartRef.current.mouseX) / scaleX / scale
      const deltaY = (pos.y - dragStartRef.current.mouseY) / scaleY / scale

      const newRect = clampCropRect({
        x: dragStartRef.current.rect.x + deltaX,
        y: dragStartRef.current.rect.y + deltaY,
        width: dragStartRef.current.rect.width,
        height: dragStartRef.current.rect.height,
      })

      // Convert to display coords and imperatively update all Konva nodes
      const newDisplayRect: DisplayRect = {
        x: newRect.x * scaleX,
        y: newRect.y * scaleY,
        width: newRect.width * scaleX,
        height: newRect.height * scaleY,
      }
      applyDisplayRect(newDisplayRect)

      // Update React state for persistence and toolbar sync
      onCropRectChange(newRect)

      e.cancelBubble = true
    },
    [scaleX, scaleY, scale, clampCropRect, onCropRectChange, applyDisplayRect]
  )

  const handleRectDragEnd = useCallback((e: KonvaEventObject<DragEvent>) => {
    isDraggingRef.current = false
    dragStartRef.current = null
    e.cancelBubble = true
  }, [])

  // Handle dragging corner/edge handles
  const handleHandleDragStart = useCallback(
    (handle: HandlePosition, e: KonvaEventObject<DragEvent>) => {
      isDraggingRef.current = true
      const stage = e.target.getStage()
      if (!stage) return

      const pos = stage.getPointerPosition()
      if (!pos) return

      dragStartRef.current = {
        rect: { ...cropRect },
        mouseX: pos.x,
        mouseY: pos.y,
        handle,
      }
      e.cancelBubble = true
    },
    [cropRect]
  )

  const handleHandleDragMove = useCallback(
    (handle: HandlePosition, e: KonvaEventObject<DragEvent>) => {
      if (!(isDraggingRef.current && dragStartRef.current)) return

      const stage = e.target.getStage()
      if (!stage) return

      const pos = stage.getPointerPosition()
      if (!pos) return

      // Calculate delta in source coords
      const deltaX = (pos.x - dragStartRef.current.mouseX) / scaleX / scale
      const deltaY = (pos.y - dragStartRef.current.mouseY) / scaleY / scale

      const startRect = dragStartRef.current.rect
      let newRect = { ...startRect }

      // Adjust rect based on which handle is being dragged
      switch (handle) {
        case "top-left":
          newRect.x = startRect.x + deltaX
          newRect.y = startRect.y + deltaY
          newRect.width = startRect.width - deltaX
          newRect.height = startRect.height - deltaY
          break
        case "top-right":
          newRect.y = startRect.y + deltaY
          newRect.width = startRect.width + deltaX
          newRect.height = startRect.height - deltaY
          break
        case "bottom-left":
          newRect.x = startRect.x + deltaX
          newRect.width = startRect.width - deltaX
          newRect.height = startRect.height + deltaY
          break
        case "bottom-right":
          newRect.width = startRect.width + deltaX
          newRect.height = startRect.height + deltaY
          break
        case "top":
          newRect.y = startRect.y + deltaY
          newRect.height = startRect.height - deltaY
          break
        case "bottom":
          newRect.height = startRect.height + deltaY
          break
        case "left":
          newRect.x = startRect.x + deltaX
          newRect.width = startRect.width - deltaX
          break
        case "right":
          newRect.width = startRect.width + deltaX
          break
        default:
          throw new Error(`Invalid handle position: ${handle satisfies never}`)
      }

      // Apply aspect ratio if locked
      if (aspectRatioLock && aspectRatio) {
        if (
          handle === "top-left" ||
          handle === "top-right" ||
          handle === "bottom-left" ||
          handle === "bottom-right"
        ) {
          // For corners, use the larger delta to determine size
          if (Math.abs(deltaX) > Math.abs(deltaY)) {
            newRect.height = newRect.width / aspectRatio
          } else {
            newRect.width = newRect.height * aspectRatio
          }
        } else if (handle === "left" || handle === "right") {
          newRect.height = newRect.width / aspectRatio
        } else {
          newRect.width = newRect.height * aspectRatio
        }
      }

      newRect = clampCropRect(newRect, handle)

      // Convert to display coords and imperatively update all Konva nodes
      const newDisplayRect: DisplayRect = {
        x: newRect.x * scaleX,
        y: newRect.y * scaleY,
        width: newRect.width * scaleX,
        height: newRect.height * scaleY,
      }
      applyDisplayRect(newDisplayRect)

      // Update React state for persistence and toolbar sync
      onCropRectChange(newRect)

      e.cancelBubble = true
    },
    [
      scaleX,
      scaleY,
      scale,
      aspectRatioLock,
      aspectRatio,
      clampCropRect,
      onCropRectChange,
      applyDisplayRect,
    ]
  )

  const handleHandleDragEnd = useCallback((e: KonvaEventObject<DragEvent>) => {
    isDraggingRef.current = false
    dragStartRef.current = null
    e.cancelBubble = true
  }, [])

  // Grid line positions (rule of thirds)
  const thirdWidth = displayCropRect.width / 3
  const thirdHeight = displayCropRect.height / 3

  // Use display dimensions for overlay positioning
  const overlayX = displayDimensions.x
  const overlayY = displayDimensions.y
  const overlayWidth = displayDimensions.width
  const overlayHeight = displayDimensions.height

  return (
    <Group ref={groupRef} rotation={shapeRotation} x={overlayX} y={overlayY}>
      {/* Dark overlay outside crop area - reads from ref for imperative updates */}
      <Shape
        fill={cropColors.overlay}
        listening={false}
        sceneFunc={(context, shapeNode) => {
          // Read from ref so batchDraw() picks up latest values
          const rect = displayCropRectRef.current
          context.beginPath()
          // Outer rect (full image at display size)
          context.rect(0, 0, overlayWidth, overlayHeight)
          // Inner rect (crop area) - counter-clockwise to create hole
          context.moveTo(rect.x, rect.y)
          context.lineTo(rect.x, rect.y + rect.height)
          context.lineTo(rect.x + rect.width, rect.y + rect.height)
          context.lineTo(rect.x + rect.width, rect.y)
          context.closePath()
          context.fillStrokeShape(shapeNode)
        }}
      />

      {/* Crop area border */}
      <Rect
        draggable
        fill="transparent"
        height={displayCropRect.height}
        onDragEnd={handleRectDragEnd}
        onDragMove={handleRectDragMove}
        onDragStart={handleRectDragStart}
        ref={borderRectRef}
        stroke={cropColors.border}
        strokeWidth={borderWidth}
        width={displayCropRect.width}
        x={displayCropRect.x}
        y={displayCropRect.y}
      />

      {/* Grid lines (rule of thirds) - 4 lines with refs for imperative updates */}
      {/* Vertical line 1 */}
      <Line
        listening={false}
        points={[
          displayCropRect.x + thirdWidth,
          displayCropRect.y,
          displayCropRect.x + thirdWidth,
          displayCropRect.y + displayCropRect.height,
        ]}
        ref={(node) => {
          lineRefs.current[0] = node
        }}
        stroke={cropColors.grid}
        strokeWidth={gridWidth}
      />
      {/* Vertical line 2 */}
      <Line
        listening={false}
        points={[
          displayCropRect.x + thirdWidth * 2,
          displayCropRect.y,
          displayCropRect.x + thirdWidth * 2,
          displayCropRect.y + displayCropRect.height,
        ]}
        ref={(node) => {
          lineRefs.current[1] = node
        }}
        stroke={cropColors.grid}
        strokeWidth={gridWidth}
      />
      {/* Horizontal line 1 */}
      <Line
        listening={false}
        points={[
          displayCropRect.x,
          displayCropRect.y + thirdHeight,
          displayCropRect.x + displayCropRect.width,
          displayCropRect.y + thirdHeight,
        ]}
        ref={(node) => {
          lineRefs.current[2] = node
        }}
        stroke={cropColors.grid}
        strokeWidth={gridWidth}
      />
      {/* Horizontal line 2 */}
      <Line
        listening={false}
        points={[
          displayCropRect.x,
          displayCropRect.y + thirdHeight * 2,
          displayCropRect.x + displayCropRect.width,
          displayCropRect.y + thirdHeight * 2,
        ]}
        ref={(node) => {
          lineRefs.current[3] = node
        }}
        stroke={cropColors.grid}
        strokeWidth={gridWidth}
      />

      {/* Corner handles - 4 handles with refs for imperative updates */}
      {/* Top-left */}
      <Circle
        draggable
        fill={cropColors.handleFill}
        onDragEnd={(e) => handleHandleDragEnd(e)}
        onDragMove={(e) => handleHandleDragMove("top-left", e)}
        onDragStart={(e) => handleHandleDragStart("top-left", e)}
        radius={handleRadius}
        ref={(node) => {
          handleRefs.current[0] = node
        }}
        stroke={cropColors.handleStroke}
        strokeWidth={borderWidth}
        x={displayCropRect.x}
        y={displayCropRect.y}
      />
      {/* Top-right */}
      <Circle
        draggable
        fill={cropColors.handleFill}
        onDragEnd={(e) => handleHandleDragEnd(e)}
        onDragMove={(e) => handleHandleDragMove("top-right", e)}
        onDragStart={(e) => handleHandleDragStart("top-right", e)}
        radius={handleRadius}
        ref={(node) => {
          handleRefs.current[1] = node
        }}
        stroke={cropColors.handleStroke}
        strokeWidth={borderWidth}
        x={displayCropRect.x + displayCropRect.width}
        y={displayCropRect.y}
      />
      {/* Bottom-left */}
      <Circle
        draggable
        fill={cropColors.handleFill}
        onDragEnd={(e) => handleHandleDragEnd(e)}
        onDragMove={(e) => handleHandleDragMove("bottom-left", e)}
        onDragStart={(e) => handleHandleDragStart("bottom-left", e)}
        radius={handleRadius}
        ref={(node) => {
          handleRefs.current[2] = node
        }}
        stroke={cropColors.handleStroke}
        strokeWidth={borderWidth}
        x={displayCropRect.x}
        y={displayCropRect.y + displayCropRect.height}
      />
      {/* Bottom-right */}
      <Circle
        draggable
        fill={cropColors.handleFill}
        onDragEnd={(e) => handleHandleDragEnd(e)}
        onDragMove={(e) => handleHandleDragMove("bottom-right", e)}
        onDragStart={(e) => handleHandleDragStart("bottom-right", e)}
        radius={handleRadius}
        ref={(node) => {
          handleRefs.current[3] = node
        }}
        stroke={cropColors.handleStroke}
        strokeWidth={borderWidth}
        x={displayCropRect.x + displayCropRect.width}
        y={displayCropRect.y + displayCropRect.height}
      />
    </Group>
  )
}
