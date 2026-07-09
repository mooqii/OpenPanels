import { useCallback, useRef, useState } from "react"
import type { Editor } from "../editor"
import type { PageId, ShapeId } from "../types/ids"
import type { Bounds, Shape } from "../types/shapes"
import {
  getAbsoluteRotatedAnchorPoints,
  getShapesBounds,
} from "../utils/coordinates"
import {
  cloneShapesForClipboard,
  cloneShapesForPaste,
} from "../utils/shape-actions"

const DEFAULT_ROOT_FONT_SIZE_PX = 16
const DEFAULT_PASTE_GAP_REM = 4

/** Marker prefix for shape data in clipboard */
export const SHAPE_DATA_MARKER = "creart:shapes:"

/** Decode shape data from clipboard text */
export function decodeShapeData(text: string): Shape[] | null {
  if (!text.startsWith(SHAPE_DATA_MARKER)) return null
  try {
    const encoded = text.slice(SHAPE_DATA_MARKER.length)
    const json = decodeURIComponent(atob(encoded))
    return JSON.parse(json) as Shape[]
  } catch {
    return null
  }
}

interface UseClipboardResult {
  clipboard: Shape[] | null
  handleCopy: () => void
  handlePaste: (options?: {
    targetPoint?: { x: number; y: number }
    shapes?: Shape[]
  }) => void
}

export function useClipboard(editor: Editor): UseClipboardResult {
  const [clipboard, setClipboard] = useState<Shape[] | null>(null)
  const clipboardRef = useRef<Shape[] | null>(null)

  // Keep ref in sync with state for access in callbacks
  const setClipboardWithRef = useCallback((shapes: Shape[] | null) => {
    setClipboard(shapes)
    clipboardRef.current = shapes
  }, [])

  const handleCopy = useCallback(async () => {
    const selectedShapes = editor.getSelectedShapes()
    if (selectedShapes.length === 0) return

    const shapesById = new Map<ShapeId, Shape>()
    for (const shape of selectedShapes) {
      shapesById.set(shape.id, shape)
      if (shape.type === "group") {
        for (const descendant of editor.getShapeDescendants(shape.id)) {
          shapesById.set(descendant.id, descendant)
        }
      }
    }

    const ordered = editor
      .getCurrentPageShapes()
      .filter((shape) => shapesById.has(shape.id))

    const shapesToCopy = ordered.length
      ? ordered
      : Array.from(shapesById.values())
    const clonedShapes = cloneShapesForClipboard(shapesToCopy)

    // Store in in-app clipboard
    setClipboardWithRef(clonedShapes)

    // Also write to system clipboard with encoded shape data
    try {
      const shapeData = JSON.stringify(clonedShapes)
      const encodedData = btoa(encodeURIComponent(shapeData))
      const markerText = `${SHAPE_DATA_MARKER}${encodedData}`
      await navigator.clipboard.writeText(markerText)
    } catch {
      // Ignore clipboard write errors (e.g., permission denied)
    }
  }, [editor, setClipboardWithRef])

  const handlePaste = useCallback(
    (options?: {
      targetPoint?: { x: number; y: number }
      shapes?: Shape[]
    }) => {
      const currentClipboard = options?.shapes ?? clipboardRef.current
      if (!currentClipboard || currentClipboard.length === 0) return

      const currentPageId = editor.getCurrentPageId()
      const currentShapes = editor.getCurrentPageShapes()

      const maxIndex = currentShapes.length
        ? Math.max(...currentShapes.map((shape) => shape.index))
        : 0
      const baseIndex = maxIndex + 1
      const bounds = getClipboardBounds(currentClipboard)

      const targetPoint =
        options?.targetPoint ?? getDefaultPasteTargetPoint(bounds)
      const offset = {
        x: targetPoint.x - bounds.x,
        y: targetPoint.y - bounds.y,
      }

      const parentId = currentPageId ?? currentClipboard[0].parentId

      const idFactory = () => `shape:${crypto.randomUUID()}` as ShapeId
      const clones = cloneShapesForPaste(currentClipboard, {
        idFactory,
        offset,
        baseIndex,
        parentId: parentId as PageId | ShapeId,
      })

      editor.run(() => {
        for (const shape of clones) {
          editor.createShape(shape)
        }
      })
      const clonedIds = new Set(clones.map((shape) => shape.id))
      const topLevelIds = clones
        .filter((shape) => !clonedIds.has(shape.parentId as ShapeId))
        .map((shape) => shape.id)
      editor.setSelectedShapes(topLevelIds)
    },
    [editor]
  )

  return {
    clipboard,
    handleCopy,
    handlePaste,
  }
}

function getDefaultPasteGapPx(): number {
  if (typeof window === "undefined" || typeof document === "undefined") {
    return DEFAULT_PASTE_GAP_REM * DEFAULT_ROOT_FONT_SIZE_PX
  }

  const rootFontSize = Number.parseFloat(
    window.getComputedStyle(document.documentElement).fontSize
  )

  return Number.isFinite(rootFontSize) && rootFontSize > 0
    ? DEFAULT_PASTE_GAP_REM * rootFontSize
    : DEFAULT_PASTE_GAP_REM * DEFAULT_ROOT_FONT_SIZE_PX
}

function getDefaultPasteTargetPoint(bounds: Bounds): { x: number; y: number } {
  return {
    x: bounds.x + bounds.width + getDefaultPasteGapPx(),
    y: bounds.y,
  }
}

function getClipboardBounds(shapes: Shape[]): Bounds {
  if (shapes.length === 0) {
    return { x: 0, y: 0, width: 0, height: 0 }
  }

  const byId = new Map<ShapeId, Shape>()
  for (const shape of shapes) {
    byId.set(shape.id, shape)
  }

  const points = shapes
    .filter((shape) => shape.type !== "group")
    .flatMap((shape) =>
      getAbsoluteRotatedAnchorPoints(shape, (id) => byId.get(id))
    )

  if (points.length === 0) {
    return getShapesBounds(shapes)
  }

  let minX = Number.POSITIVE_INFINITY
  let minY = Number.POSITIVE_INFINITY
  let maxX = Number.NEGATIVE_INFINITY
  let maxY = Number.NEGATIVE_INFINITY

  for (const point of points) {
    minX = Math.min(minX, point.x)
    minY = Math.min(minY, point.y)
    maxX = Math.max(maxX, point.x)
    maxY = Math.max(maxY, point.y)
  }

  return {
    x: minX,
    y: minY,
    width: maxX - minX,
    height: maxY - minY,
  }
}
