import type { KonvaEventObject } from "konva/lib/Node"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import type { Editor } from "../editor"
import { PageId, type ShapeId } from "../types/ids"
import type { Shape } from "../types/shapes"
import { canvasToStage } from "../utils/coordinates"
import { moveShapesToBack, moveShapesToFront } from "../utils/shape-actions"
import { useSelectedShapes } from "./use-editor-state"

type ContextMenuType = "shape" | "blank"

interface ContextMenuState {
  canvasPoint: { x: number; y: number } | null
  isOpen: boolean
  screenPoint: { x: number; y: number } | null
  type: ContextMenuType
}

interface UseContextMenuOptions {
  clipboard: Shape[] | null
  containerRef: React.RefObject<HTMLElement | null>
  editor: Editor
  handleCopy: () => void
  handlePaste: (options?: { targetPoint?: { x: number; y: number } }) => void
}

export function getMenuPosition(
  screenPoint: { x: number; y: number } | null,
  rect: DOMRect | null
): { x: number; y: number } | null {
  if (screenPoint === null || rect === null) return null
  return {
    x: screenPoint.x - rect.left,
    y: screenPoint.y - rect.top,
  }
}

export function useContextMenu({
  editor,
  containerRef,
  clipboard,
  handleCopy,
  handlePaste,
}: UseContextMenuOptions) {
  const selectedShapes = useSelectedShapes(editor)
  const selectedShapesRef = useRef<Shape[]>(selectedShapes)
  const [contextMenu, setContextMenu] = useState<ContextMenuState>({
    isOpen: false,
    type: "blank",
    canvasPoint: null,
    screenPoint: null,
  })

  const menuPosition = useMemo(() => {
    const rect = containerRef.current?.getBoundingClientRect() ?? null
    return getMenuPosition(contextMenu.screenPoint, rect)
  }, [containerRef, contextMenu.screenPoint])

  useEffect(() => {
    selectedShapesRef.current = selectedShapes
  }, [selectedShapes])

  const closeContextMenu = useCallback(() => {
    setContextMenu((prev) => ({ ...prev, isOpen: false }))
  }, [])

  // Helper to find the top-level group (shape whose parent is a page)
  const getTopLevelShape = useCallback(
    (shapeId: ShapeId): ShapeId => {
      const shape = editor.getShape(shapeId)
      if (!shape) return shapeId

      // If parent is a page, this is already top-level
      if (PageId.isValid(shape.parentId)) {
        return shapeId
      }

      // Otherwise, traverse up to find the top-level ancestor
      let currentId: ShapeId = shapeId
      let current = shape

      while (current && !PageId.isValid(current.parentId)) {
        const parent = editor.getShape(current.parentId as ShapeId)
        if (!parent) break
        currentId = parent.id
        current = parent
      }

      return currentId
    },
    [editor]
  )

  const onStageContextMenu = useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      e.evt.preventDefault()
      e.cancelBubble = true

      const stage = e.target.getStage()
      const pointer = stage?.getRelativePointerPosition()
      if (!pointer) return

      const targetId = typeof e.target?.id === "function" ? e.target.id() : null
      const targetShape = targetId
        ? editor.getShape(targetId as ShapeId)
        : undefined

      if (targetShape) {
        // Select the top-level group if shape is inside a group
        const topLevelId = getTopLevelShape(targetShape.id)
        editor.setSelectedShapes([topLevelId])
      }

      setContextMenu({
        isOpen: true,
        type: targetShape ? "shape" : "blank",
        canvasPoint: pointer,
        screenPoint: { x: e.evt.clientX, y: e.evt.clientY },
      })
    },
    [editor, getTopLevelShape]
  )

  const onOpenChange = useCallback(
    (isOpen: boolean) => {
      if (!isOpen) {
        closeContextMenu()
      }
    },
    [closeContextMenu]
  )

  const handleBringToFront = useCallback(() => {
    const updates = moveShapesToFront(
      selectedShapesRef.current,
      editor.getCurrentPageShapes()
    )
    if (updates.length === 0) return

    editor.run(() => {
      for (const shape of updates) {
        editor.updateShape(shape.id, { index: shape.index })
      }
    })
  }, [editor])

  const handleSendToBack = useCallback(() => {
    const updates = moveShapesToBack(
      selectedShapesRef.current,
      editor.getCurrentPageShapes()
    )
    if (updates.length === 0) return
    editor.run(() => {
      for (const shape of updates) {
        editor.updateShape(shape.id, { index: shape.index })
      }
    })
  }, [editor])

  const handleDeleteSelected = useCallback(() => {
    const currentSelected = selectedShapesRef.current
    if (currentSelected.length === 0) return
    editor.run(() => {
      for (const shape of currentSelected) {
        editor.deleteShape(shape)
      }
    })
  }, [editor])

  const handleZoomIn = useCallback(() => {
    const center = contextMenu.canvasPoint
      ? canvasToStage(
          editor.stage,
          contextMenu.canvasPoint.x,
          contextMenu.canvasPoint.y
        )
      : undefined
    editor.zoomIn(center ?? undefined, { animation: { duration: 200 } })
  }, [contextMenu.canvasPoint, editor])

  const handleZoomOut = useCallback(() => {
    const center = contextMenu.canvasPoint
      ? canvasToStage(
          editor.stage,
          contextMenu.canvasPoint.x,
          contextMenu.canvasPoint.y
        )
      : undefined
    editor.zoomOut(center ?? undefined, { animation: { duration: 200 } })
  }, [contextMenu.canvasPoint, editor])

  const onAction = useCallback(
    (key: string | number) => {
      switch (key) {
        case "copy":
          handleCopy()
          break
        case "paste":
          handlePaste({ targetPoint: contextMenu.canvasPoint ?? undefined })
          break
        case "bring-front":
          handleBringToFront()
          break
        case "send-back":
          handleSendToBack()
          break
        case "delete":
          handleDeleteSelected()
          break
        case "zoom-in":
          handleZoomIn()
          break
        case "zoom-out":
          handleZoomOut()
          break
        default:
          break
      }
      closeContextMenu()
    },
    [
      closeContextMenu,
      handleBringToFront,
      handleCopy,
      handleDeleteSelected,
      handlePaste,
      handleSendToBack,
      handleZoomIn,
      handleZoomOut,
      contextMenu.canvasPoint,
    ]
  )

  return {
    contextMenu,
    menuPosition,
    onStageContextMenu,
    onOpenChange,
    onAction,
    canCopy: selectedShapes.length > 0,
    canPaste: Boolean(clipboard?.length),
  }
}
