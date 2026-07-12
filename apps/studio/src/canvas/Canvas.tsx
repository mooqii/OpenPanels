import type Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import {
  type ReactNode,
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { Group, Layer, Rect, Shape, Stage } from "react-konva"
import {
  createPencilCursor,
  getRelativePointerPosition,
  type MarkerCursorPreviewState,
} from "./canvas-cursor"
import { BrushToolbar } from "./components/BrushToolbar"
import { ConnectorPreview } from "./components/ConnectorPreview"
import { ContextMenu } from "./components/ContextMenu"
import { ContextToolbar } from "./components/ContextToolbar"
import { CropOverlay } from "./components/CropOverlay"
import { CropToolbar } from "./components/CropToolbar"
import { DrawPreview } from "./components/DrawPreview"
import { PathEditorOverlay } from "./components/PathEditorOverlay"
import { TextEditInput } from "./components/TextEditInput"
import { Transformer } from "./components/Transformer"
import {
  SELECTION_RECT_COLOR,
  TRANSFORMER_BORDER_STROKE_WIDTH,
  TRANSFORMER_STROKE_COLOR,
} from "./constants"
import { CropProvider } from "./contexts/CropContext"
import { useEditor } from "./EditorContext"
import { useAlignmentGuides } from "./hooks/use-alignment-guides"
import { useAltDragCopy } from "./hooks/use-alt-drag-copy"
import { useBrush } from "./hooks/use-brush"
import { useCamera } from "./hooks/use-camera"
import { useClipboard } from "./hooks/use-clipboard"
import { useConnectorBindings } from "./hooks/use-connector-bindings"
import { useConnectorTool } from "./hooks/use-connector-tool"
import { useContextMenu } from "./hooks/use-context-menu"
import { useCrop } from "./hooks/use-crop"
import { useCursor } from "./hooks/use-cursor"
import { useDrawShape } from "./hooks/use-draw-shape"
import { useDrop } from "./hooks/use-drop"
import {
  useCurrentPageShapes,
  useSelectedShapes,
  useTool,
} from "./hooks/use-editor-state"
import { useHover } from "./hooks/use-hover"
import { useKeyboard } from "./hooks/use-keyboard"
import { useMarker } from "./hooks/use-marker"
import { usePathEdit } from "./hooks/use-path-edit"
import { usePen } from "./hooks/use-pen"
import { usePencil } from "./hooks/use-pencil"
import { useSelection } from "./hooks/use-selection"
import { useTextEdit } from "./hooks/use-text-edit"
import { useToolbar } from "./hooks/use-toolbar"
import { useTransform } from "./hooks/use-transform"
import { ShapeRenderer } from "./renderers/ShapeRenderer"
import {
  type CanvasSelectionSnapshot,
  summarizeSelectionShape,
} from "./selection-snapshot"
import { type AssetId, PageId, ShapeId } from "./types/ids"
import { captureTransformer } from "./utils/capture"

export type {
  CanvasSelectionShape,
  CanvasSelectionSnapshot,
} from "./selection-snapshot"

interface CanvasProps {
  allowImagePaste?: boolean
  children?: ReactNode
  disableCamera?: boolean
  height?: number
  onCanvasPointerDown?: () => void
  onSelectionChange?: (selection: CanvasSelectionSnapshot) => void
  onSelectionMaterializerChange?: (
    materialize: (() => string | null) | null
  ) => void
  onStageReady?: () => void
  selectedShapeIds?: string[]
  width?: number
}

export function Canvas({
  allowImagePaste = true,
  width = 800,
  height = 600,
  children,
  disableCamera = false,
  onCanvasPointerDown,
  onSelectionChange,
  onSelectionMaterializerChange,
  onStageReady,
  selectedShapeIds = [],
}: CanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const lastReadyStageRef = useRef<Konva.Stage | null>(null)
  const editor = useEditor()
  const [markerCursorPreview, setMarkerCursorPreview] =
    useState<MarkerCursorPreviewState | null>(null)
  const [stageReadyVersion, setStageReadyVersion] = useState(0)

  const previewShapeRef = useRef<Konva.Shape>(null)
  const guidesGroupRef = useRef<Konva.Group>(null)
  const hoverGroupRef = useRef<Konva.Group>(null)

  const shapes = useCurrentPageShapes(editor)
  const selectedShapes = useSelectedShapes(editor)
  const tool = useTool(editor)

  useEffect(() => {
    const currentIds = editor.getSelectedShapes().map((shape) => shape.id)
    const nextIds = selectedShapeIds.filter(ShapeId.isValid)
    if (
      currentIds.length === nextIds.length &&
      currentIds.every((id, index) => id === nextIds[index])
    )
      return
    editor.setSelectedShapes(nextIds)
  }, [editor, selectedShapeIds])

  // Filter to top-level shapes only (direct children of the page, not nested in groups)
  const topLevelShapes = useMemo(
    () => shapes.filter((s) => PageId.isValid(s.parentId)),
    [shapes]
  )
  const textEdit = useTextEdit(editor)

  const pen = usePen(editor)
  const pathEdit = usePathEdit(editor)

  const finishPathClosed = useCallback(() => {
    pen.finishPath(true)
  }, [pen.finishPath])

  const finishPathOpen = useCallback(() => {
    pen.finishPath(false)
  }, [pen.finishPath])

  const clipboard = useClipboard(editor)

  const { isShiftPressed } = useKeyboard({
    allowImagePaste,
    editor,
    selectedShapes,
    isPenDrawing: pen.isDrawing,
    onPenClose: finishPathClosed,
    onPenFinish: finishPathOpen,
    onPenRemoveLastPoint: pen.removeLastPoint,
    onCopy: clipboard.handleCopy,
    onPaste: clipboard.handlePaste,
  })

  const drawShape = useDrawShape({
    editor,
    previewShapeRef,
    isShiftPressed,
  })
  const pencil = usePencil({
    editor,
    previewShapeRef,
  })
  const brush = useBrush({
    editor,
    previewShapeRef,
  })
  const marker = useMarker({
    editor,
    previewShapeRef,
  })

  const transform = useTransform(editor, stageReadyVersion)
  const selection = useSelection(editor)
  const camera = useCamera(editor, tool, containerRef)
  const altDragCopy = useAltDragCopy(editor)
  const connector = useConnectorTool({ editor })
  const connectorBindings = useConnectorBindings({ editor })

  const alignmentGuides = useAlignmentGuides({
    editor,
    guidesGroupRef,
  })

  const hover = useHover({
    editor,
    groupRef: hoverGroupRef,
  })

  // Crop mode state
  const crop = useCrop(editor)

  // Disable shape dragging and selection when in crop mode
  const isInCropMode = crop.cropShapeId != null
  const isShapeDraggable =
    !(camera.isPanning || transform.isTransforming || isInCropMode) &&
    tool.name === "select"

  const toolbar = useToolbar({
    editor,
    transformerRef: transform.transformerRef,
    isTransforming: transform.isTransforming,
    cropShapeId: crop.cropShapeId,
  })

  useEffect(() => {
    if (!onSelectionChange) return
    let cancelled = false
    let captureFrame = 0
    const frame = window.requestAnimationFrame(() => {
      if (cancelled) return
      captureFrame = window.requestAnimationFrame(() => {
        if (cancelled) return
        const summarizedShapes = selectedShapes.map((shape) =>
          summarizeSelectionShape(editor, shape)
        )
        onSelectionChange({
          assetRef:
            summarizedShapes.find((shape) => shape.asset?.assetRef)?.asset
              ?.assetRef ?? null,
          selectedShapeIds: selectedShapes.map((shape) => shape.id),
          selectedShapes: summarizedShapes,
        })
      })
    })
    return () => {
      cancelled = true
      window.cancelAnimationFrame(frame)
      if (captureFrame) {
        window.cancelAnimationFrame(captureFrame)
      }
    }
  }, [editor, onSelectionChange, selectedShapes])

  useEffect(() => {
    if (!onSelectionMaterializerChange) return
    onSelectionMaterializerChange(() => {
      if (selectedShapes.length === 0) return null
      return captureTransformer(transform.transformerRef.current)
    })
    return () => onSelectionMaterializerChange(null)
  }, [
    onSelectionMaterializerChange,
    selectedShapes,
    transform.transformerRef.current,
  ])

  // Transformer nodes (filter out crop shape when in crop mode)
  const effectiveTransformerNodes = useMemo(() => {
    if (!crop.cropShapeId) return transform.transformerNodes
    return transform.transformerNodes.filter(
      (node) => node.id() !== crop.cropShapeId
    )
  }, [transform.transformerNodes, crop.cropShapeId])

  const dragdrop = useDrop({ editor, containerRef })
  const isSingleTextSelection =
    selectedShapes.length === 1 && selectedShapes[0]?.type === "text"
  const shouldUseTextTransformerAnchors =
    isSingleTextSelection || textEdit.isEditing

  const shouldKeepImageRatio = useMemo(() => {
    return (
      selectedShapes.length > 0 &&
      selectedShapes.every((shape) => shape.type === "image")
    )
  }, [selectedShapes])
  const drawCursor = useMemo(() => {
    if (tool.name === "text") {
      return "text"
    }

    if (tool.name === "pencil") {
      return createPencilCursor(tool.color)
    }

    if (tool.name === "brush") {
      return "none"
    }

    if (
      tool.name === "connector" ||
      (tool.name === "draw" &&
        (tool.shape === "rectangle" ||
          tool.shape === "ellipse" ||
          tool.shape === "line"))
    ) {
      return "crosshair"
    }

    if (tool.name === "marker") {
      return "none"
    }

    return null
  }, [tool])

  // Centralized cursor management
  useCursor({
    containerRef,
    isTextEditing: textEdit.isEditing,
    toolName: tool.name,
    drawCursor,
  })

  useEffect(() => {
    if (tool.name !== "marker" && tool.name !== "brush") {
      setMarkerCursorPreview(null)
    }
  }, [tool.name])

  useEffect(() => {
    if (textEdit.isEditing && tool.name !== "select") {
      textEdit.stopTextEditing()
    }
  }, [textEdit.isEditing, textEdit.stopTextEditing, tool.name])

  useEffect(() => {
    if (!(textEdit.isEditing && textEdit.isTextNodeReady)) return

    textEdit.updateInputPosition()
    transform.transformerRef.current?.forceUpdate()
  }, [
    textEdit.isEditing,
    textEdit.isTextNodeReady,
    textEdit.updateInputPosition,
    transform.transformerRef,
  ])

  const updateMarkerCursorPosition = useCallback(
    (event: PointerEvent | ReactPointerEvent<HTMLDivElement>) => {
      if (
        (tool.name !== "marker" && tool.name !== "brush") ||
        !containerRef.current
      ) {
        return
      }

      const position = getRelativePointerPosition(containerRef.current, event)
      const scale = editor.getZoom()

      setMarkerCursorPreview({
        color: tool.color,
        ...position,
        diameter: tool.size * scale,
      })
    },
    [editor, tool]
  )

  const clearMarkerCursorPosition = useCallback(() => {
    setMarkerCursorPreview(null)
  }, [])

  const handleShapeDragEnd = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      // Check if Alt+Drag copy was active - if so, skip normal position update
      const wasCopying = altDragCopy.handleDragEnd(e)
      if (!wasCopying) {
        transform.handleShapeDragEnd(e)
      }
      alignmentGuides.handleDragEnd()
      toolbar.handleDragEnd()
      hover.handleDragEnd()
    },
    [
      altDragCopy.handleDragEnd,
      transform.handleShapeDragEnd,
      alignmentGuides.handleDragEnd,
      toolbar.handleDragEnd,
      hover.handleDragEnd,
    ]
  )

  const handleShapeDragMove = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      altDragCopy.handleDragMove(e)
      alignmentGuides.handleDragMove(e)

      // Redraw connectors in real-time during drag
      const draggedId = e.target.id() as ShapeId
      if (draggedId) {
        connectorBindings.redrawConnectorsForShapes([draggedId])
      }
    },
    [
      altDragCopy.handleDragMove,
      alignmentGuides.handleDragMove,
      connectorBindings.redrawConnectorsForShapes,
    ]
  )

  const handleShapeDragStart = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      altDragCopy.handleDragStart(e)
      if (tool.name === "select" && !isInCropMode) {
        selection.handleShapeDragStart(e)
      }
      alignmentGuides.handleDragStart(e)
      toolbar.handleDragStart()
      hover.handleDragStart()
    },
    [
      altDragCopy.handleDragStart,
      tool,
      isInCropMode,
      selection.handleShapeDragStart,
      alignmentGuides.handleDragStart,
      toolbar.handleDragStart,
      hover.handleDragStart,
    ]
  )

  const handleShapeClick = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      // Disable selection when in crop mode
      if (isInCropMode) return
      if (tool.name === "select") {
        selection.handleShapeClick(e)
      }
    },
    [tool, selection.handleShapeClick, isInCropMode]
  )

  // Handle double-click on text shapes and groups
  const handleShapeDoubleClick = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      const shapeId = e.target?.id() as ShapeId | undefined
      if (!shapeId) return

      const shape = editor.getShape(shapeId)
      if (!shape) return

      if (shape.type === "text") {
        textEdit.startTextEditing(shapeId)
        e.cancelBubble = true
        return
      }

      let groupId: ShapeId | null = null
      if (shape.type === "group") {
        groupId = shape.id
      } else {
        const parent = editor.getShapeParent(shapeId)
        if (parent) {
          groupId = parent.id
        }
      }

      if (groupId) {
        // Open group for editing child shapes
        const currentOpenedGroupId = editor.getOpenedGroupId()
        if (currentOpenedGroupId === groupId) {
          // If already open, close it
          editor.closeGroup()
        } else {
          // Open the group
          editor.openGroup(groupId)
          if (shapeId !== groupId) {
            editor.setSelectedShapes([shapeId])
          }
        }
        e.cancelBubble = true
      }
    },
    [editor, textEdit]
  )

  const handleStageMouseDown = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      if (e.evt.button === 2) {
        return
      }
      onCanvasPointerDown?.()
      updateMarkerCursorPosition(e.evt)
      if (!disableCamera) camera.handleMouseDown(e)
      if (e.cancelBubble) return

      if (tool.name === "draw") {
        drawShape.handleMouseDown(e)
        e.cancelBubble = true
      } else if (tool.name === "pencil") {
        pencil.handleMouseDown(e)
        e.cancelBubble = true
      } else if (tool.name === "brush") {
        brush.handleMouseDown(e)
        e.cancelBubble = true
      } else if (tool.name === "marker") {
        marker.handleMouseDown(e)
        e.cancelBubble = true
      } else if (tool.name === "pen") {
        pen.handleMouseDown(e)
        e.cancelBubble = true
      } else if (tool.name === "text") {
        textEdit.handleStageMouseDown(e)
        e.cancelBubble = true
      } else if (tool.name === "connector") {
        connector.handleMouseDown(e)
        e.cancelBubble = true
      } else if (tool.name === "select" && !isInCropMode) {
        selection.handleMouseDown(e)
      }
    },
    [
      camera.handleMouseDown,
      selection.handleMouseDown,
      drawShape.handleMouseDown,
      pencil.handleMouseDown,
      brush.handleMouseDown,
      marker.handleMouseDown,
      pen.handleMouseDown,
      connector.handleMouseDown,
      tool,
      textEdit,
      isInCropMode,
      onCanvasPointerDown,
      disableCamera,
      updateMarkerCursorPosition,
    ]
  )

  const contextMenu = useContextMenu({
    editor,
    containerRef,
    clipboard: clipboard.clipboard,
    handleCopy: clipboard.handleCopy,
    handlePaste: clipboard.handlePaste,
  })

  const handleStageMouseMove = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      if (!disableCamera) camera.handleMouseMove(e)
      updateMarkerCursorPosition(e.evt)
      if (e.cancelBubble) return

      // Route to drawing or selection based on tool
      if (tool.name === "draw") {
        drawShape.handleMouseMove(e)
        e.cancelBubble = true
      } else if (tool.name === "pencil") {
        pencil.handleMouseMove(e)
        e.cancelBubble = true
      } else if (tool.name === "brush") {
        brush.handleMouseMove(e)
        e.cancelBubble = true
      } else if (tool.name === "marker") {
        marker.handleMouseMove(e)
        e.cancelBubble = true
      } else if (tool.name === "pen") {
        pen.handleMouseMove(e)
        e.cancelBubble = true
      } else if (tool.name === "connector") {
        connector.handleMouseMove(e)
        e.cancelBubble = true
      } else if (tool.name === "select" && !isInCropMode) {
        selection.handleMouseMove(e)
      }
    },
    [
      camera.handleMouseMove,
      selection.handleMouseMove,
      drawShape.handleMouseMove,
      pencil.handleMouseMove,
      brush.handleMouseMove,
      marker.handleMouseMove,
      pen.handleMouseMove,
      connector.handleMouseMove,
      tool,
      isInCropMode,
      disableCamera,
      updateMarkerCursorPosition,
    ]
  )

  const handleStageMouseUp = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      if (!disableCamera) camera.handleMouseUp(e)
      if (e.cancelBubble) return

      // Route to drawing or selection based on tool
      if (tool.name === "draw") {
        drawShape.handleMouseUp(e)
      } else if (tool.name === "pencil") {
        pencil.handleMouseUp()
      } else if (tool.name === "brush") {
        brush.handleMouseUp()
      } else if (tool.name === "marker") {
        marker.handleMouseUp()
      } else if (tool.name === "pen") {
        pen.handleMouseUp(e)
      } else if (tool.name === "connector") {
        connector.handleMouseUp(e)
      } else if (tool.name === "select" && !isInCropMode) {
        selection.handleMouseUp(e)
      }
    },
    [
      camera.handleMouseUp,
      selection.handleMouseUp,
      drawShape.handleMouseUp,
      pencil.handleMouseUp,
      brush.handleMouseUp,
      marker.handleMouseUp,
      pen.handleMouseUp,
      connector.handleMouseUp,
      tool,
      isInCropMode,
      disableCamera,
    ]
  )

  const handleWheel = useCallback(
    (e: KonvaEventObject<WheelEvent>) => {
      if (disableCamera) return
      camera.handleWheel(e)
      textEdit.updateInputPosition()
    },
    [camera.handleWheel, textEdit.updateInputPosition, disableCamera]
  )

  // Handle wheel events from overlay elements (like ContextToolbar)
  const handleOverlayWheel = useCallback(
    (e: React.WheelEvent) => {
      camera.handleOverlayWheel(e)
      toolbar.update()
      textEdit.updateInputPosition()
    },
    [camera.handleOverlayWheel, toolbar.update, textEdit.updateInputPosition]
  )

  const handleTransformStart = useCallback(() => {
    transform.handleTransformStart()
    alignmentGuides.handleTransformStart(transform.transformerRef)
  }, [
    transform.handleTransformStart,
    alignmentGuides.handleTransformStart,
    transform.transformerRef,
  ])

  const handleTransform = useCallback(() => {
    const activeAnchor = transform.transformerRef.current?.getActiveAnchor()
    const transformNodes =
      (transform.transformerRef.current?.getNodes() as
        | Konva.Node[]
        | undefined) ?? []

    if (textEdit.isEditing) {
      textEdit.setEditingBoxModes({
        widthMode:
          activeAnchor === "middle-left" || activeAnchor === "middle-right"
            ? "manual"
            : undefined,
      })

      textEdit.syncDraftFromNode(transformNodes[0] ?? null)
    } else {
      for (const node of transformNodes) {
        transform.syncTextNodeLayout(node, activeAnchor)
      }
    }

    textEdit.updateInputPosition()
    alignmentGuides.handleTransformMove(transform.transformerRef)

    // Redraw connectors in real-time during transform
    const transformingIds = transform.transformerNodes.map(
      (node) => node.id() as ShapeId
    )
    if (transformingIds.length > 0) {
      connectorBindings.redrawConnectorsForShapes(transformingIds)
    }
  }, [
    textEdit.isEditing,
    textEdit.setEditingBoxModes,
    textEdit.syncDraftFromNode,
    textEdit.updateInputPosition,
    alignmentGuides.handleTransformMove,
    transform.syncTextNodeLayout,
    transform.transformerRef,
    transform.transformerNodes,
    connectorBindings.redrawConnectorsForShapes,
  ])

  const handleTransformEnd = useCallback(() => {
    transform.handleTransformEnd()
    alignmentGuides.handleTransformEnd()
  }, [transform.handleTransformEnd, alignmentGuides.handleTransformEnd])

  // Create boundBoxFunc for snapping during transform
  const boundBoxFunc = useMemo(
    () => alignmentGuides.createBoundBoxFunc(),
    [alignmentGuides.createBoundBoxFunc]
  )

  // Resolve asset URL for image fills
  const resolveAsset = useCallback(
    (assetId: string): string | undefined => {
      const asset = editor.getAsset(assetId as AssetId)
      if (!asset) return undefined
      const assetStore = editor.getAssetStore()
      if (assetStore) {
        return assetStore.resolve(asset)
      }
      // Fallback to src property if available
      if ("src" in asset.props) {
        return asset.props.src
      }
      return undefined
    },
    [editor]
  )

  // Resolve shape by ID for connector bindings
  const resolveShape = useCallback(
    (shapeId: string) => {
      return editor.getShape(shapeId as ShapeId)
    },
    [editor]
  )

  const bindStageRef = useCallback(
    (stage: Konva.Stage | null) => {
      if (!stage) return

      editor.stage = stage

      if (lastReadyStageRef.current !== stage) {
        lastReadyStageRef.current = stage
        setStageReadyVersion((version) => version + 1)
        onStageReady?.()
      }
    },
    [editor, onStageReady]
  )

  return (
    <div
      onPointerLeave={clearMarkerCursorPosition}
      ref={containerRef}
      style={{
        width,
        height,
        position: "relative",
        outline: dragdrop.isDragging
          ? "2px dashed var(--canvas-drop-outline, var(--accent))"
          : undefined,
        outlineOffset: "-2px",
        backgroundColor: dragdrop.isDragging
          ? "var(--canvas-drop-overlay, color-mix(in oklab, var(--accent) 8%, transparent))"
          : undefined,
      }}
    >
      <Stage
        draggable={!disableCamera && camera.isPanning}
        height={height}
        onContextMenu={contextMenu.onStageContextMenu}
        onDblClick={connector.handleDoubleClick}
        onDragEnd={disableCamera ? undefined : camera.handleDragEnd}
        onPointerDown={handleStageMouseDown}
        onPointerMove={handleStageMouseMove}
        onPointerUp={handleStageMouseUp}
        onTouchEnd={disableCamera ? undefined : camera.handleTouchEnd}
        onTouchMove={disableCamera ? undefined : camera.handleTouchMove}
        onWheel={handleWheel}
        ref={bindStageRef}
        width={width}
      >
        <Layer>
          {topLevelShapes.map((shape) => {
            const asset =
              shape.type === "image"
                ? editor.getAsset((shape.props as any).assetId)
                : undefined

            // Check if this shape is being cropped
            const isShapeCropping = crop.cropShapeId === shape.id

            return (
              <ShapeRenderer
                asset={asset}
                cropNaturalSize={isShapeCropping ? crop.naturalSize : undefined}
                draggable={isShapeDraggable}
                editingShapeId={textEdit.editingShape?.id ?? null}
                editingTextNodeRef={textEdit.bindEditingTextNode}
                editingTextProps={textEdit.draftTextProps}
                editor={editor}
                isCropping={isShapeCropping}
                key={shape.id}
                onClick={handleShapeClick}
                onDoubleClick={handleShapeDoubleClick}
                onDragEnd={handleShapeDragEnd}
                onDragMove={handleShapeDragMove}
                onDragStart={handleShapeDragStart}
                onMouseEnter={
                  tool.name === "select" && !isInCropMode
                    ? hover.handleMouseEnter
                    : undefined
                }
                onMouseLeave={
                  tool.name === "select" && !isInCropMode
                    ? hover.handleMouseLeave
                    : undefined
                }
                registerConnectorRef={connectorBindings.registerConnectorRef}
                resolveAsset={resolveAsset}
                resolveShape={resolveShape}
                shape={shape}
              />
            )
          })}

          {textEdit.overlay && (
            <Group
              data-testid="text-edit-overlay"
              listening={false}
              rotation={textEdit.overlay.rotation}
              scaleX={textEdit.overlay.scaleX}
              scaleY={textEdit.overlay.scaleY}
              x={textEdit.overlay.x}
              y={textEdit.overlay.y}
            >
              <Shape
                data-testid="text-edit-border"
                fillEnabled={false}
                height={textEdit.overlay.height}
                listening={false}
                perfectDrawEnabled={false}
                sceneFunc={(context, shape) => {
                  context.beginPath()
                  context.rect(0, 0, shape.width(), shape.height())
                  context.fillStrokeShape(shape)
                }}
                stroke={TRANSFORMER_STROKE_COLOR}
                strokeScaleEnabled={false}
                strokeWidth={TRANSFORMER_BORDER_STROKE_WIDTH}
                width={textEdit.overlay.width}
                x={0}
                y={0}
              />
            </Group>
          )}

          <Transformer
            borderEnabled={!textEdit.isEditing}
            boundBoxFunc={boundBoxFunc}
            enabledAnchors={
              shouldUseTextTransformerAnchors
                ? ["middle-left", "middle-right"]
                : undefined
            }
            keepRatio={shouldKeepImageRatio}
            nodes={effectiveTransformerNodes}
            onTransform={handleTransform}
            onTransformEnd={handleTransformEnd}
            onTransformStart={handleTransformStart}
            ref={transform.transformerRef}
            rotationSnaps={
              isShiftPressed ? [0, 45, 90, 135, 180, 225, 270, 315] : []
            }
          />

          {selection.isSelecting && (
            <Rect fill={SELECTION_RECT_COLOR} ref={selection.rectRef} />
          )}

          {/* Drawing preview */}
          <DrawPreview
            brushPreviewShape={brush.previewShape}
            isDrawing={
              drawShape.isDrawing ||
              pencil.isDrawing ||
              brush.isDrawing ||
              marker.isDrawing
            }
            markerPreviewShape={marker.previewShape}
            pencilPreviewShape={pencil.previewShape}
            previewShapeRef={previewShapeRef}
            tool={tool}
          />

          {/* Pen tool path editor overlay */}
          {(tool.name === "pen" || pathEdit.editingShape) && (
            <PathEditorOverlay
              editingShape={pathEdit.editingShape}
              isNearFirstPoint={pen.isNearFirstPoint}
              mousePosition={pen.mousePosition}
              onAnchorDragEnd={pathEdit.handleAnchorDragEnd}
              onAnchorDragMove={pathEdit.handleAnchorDragMove}
              onAnchorDragStart={pathEdit.handleAnchorDragStart}
              onHandleDragEnd={pathEdit.handleHandleDragEnd}
              onHandleDragMove={pathEdit.handleHandleDragMove}
              onHandleDragStart={pathEdit.handleHandleDragStart}
              previewPoints={pen.isDrawing ? pen.previewPoints : undefined}
              scale={editor.stage?.scaleX() ?? 1}
              selectedPointIndex={pathEdit.editState.selectedPointIndex}
            />
          )}

          {/* Crop overlay */}
          {crop.croppingShape && crop.cropRect && crop.naturalSize && (
            <CropOverlay
              aspectRatio={crop.getCurrentAspectRatio()}
              aspectRatioLock={crop.aspectRatioLock}
              cropRect={crop.cropRect}
              imageHeight={crop.naturalSize.height}
              imageWidth={crop.naturalSize.width}
              onCropRectChange={crop.setCropRect}
              scale={editor.stage?.scaleX() ?? 1}
              shape={crop.croppingShape}
            />
          )}

          {/* Connector tool preview */}
          {connector.isActive && (
            <ConnectorPreview
              fromBindings={connector.state.fromBindings}
              hoveredShapeId={connector.hoveredShapeId ?? undefined}
              isActive={connector.isActive}
              isDragging={connector.isDragging}
              mousePosition={connector.mousePosition ?? undefined}
              phase={connector.state.phase}
              resolveShape={connector.resolveShape}
              scale={connector.scale}
              toBindings={connector.state.toBindings}
            />
          )}

          {/* Alignment guides - drawn imperatively by useAlignmentGuides hook */}
          <Group listening={false} ref={guidesGroupRef} />

          {/* Hover outline - drawn imperatively by useHover hook */}
          <Group listening={false} ref={hoverGroupRef} />
        </Layer>
      </Stage>

      {(tool.name === "marker" || tool.name === "brush") &&
        markerCursorPreview && (
          <div
            aria-hidden="true"
            data-testid={
              tool.name === "brush"
                ? "brush-cursor-preview"
                : "marker-cursor-preview"
            }
            style={{
              position: "absolute",
              left: markerCursorPreview.x,
              top: markerCursorPreview.y,
              width: markerCursorPreview.diameter,
              height: markerCursorPreview.diameter,
              borderRadius: "9999px",
              backgroundColor: markerCursorPreview.color,
              opacity: tool.name === "marker" ? tool.opacity : 1,
              filter:
                tool.name === "brush"
                  ? "brightness(1.18) saturate(1.08)"
                  : undefined,
              transform: "translate(-50%, -50%)",
              pointerEvents: "none",
            }}
          />
        )}

      <TextEditInput textEdit={textEdit} />

      <CropProvider value={crop}>
        <ContextToolbar
          editor={editor}
          onWheel={handleOverlayWheel}
          ref={toolbar.toolbarRef}
          selectedShape={toolbar.selectedShape}
          tools={toolbar.tools}
          transformerRef={transform.transformerRef}
        />
      </CropProvider>

      {/* Crop toolbar */}
      {crop.cropShapeId && <CropToolbar crop={crop} />}

      {contextMenu.menuPosition && (
        <ContextMenu
          canCopy={contextMenu.canCopy}
          canPaste={contextMenu.canPaste}
          isOpen={contextMenu.contextMenu.isOpen}
          menuPosition={contextMenu.menuPosition}
          menuType={contextMenu.contextMenu.type}
          onAction={contextMenu.onAction}
          onOpenChange={contextMenu.onOpenChange}
        />
      )}

      <BrushToolbar />

      {children}
    </div>
  )
}
