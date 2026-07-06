import Konva from "konva"
import type { KonvaEventObject } from "konva/lib/Node"
import React from "react"
import { CANVAS_MAX_ZOOM, CANVAS_MIN_ZOOM } from "../constants"
import type { Editor } from "../editor"
import type { Tool } from "../store"

type InputType = "mouse" | "pen" | "touch"

const isTouchDevice = () => {
  if (typeof window === "undefined") return false
  return window.matchMedia("(hover: none) and (pointer: coarse)").matches
}

/** Enable touch events during stage drag so pinch can be detected while panning. */
if (typeof Konva !== "undefined" && isTouchDevice()) {
  Konva.hitOnDragEnabled = true
}

interface WheelInput {
  inputType: InputType | null
  pointer: { x: number; y: number }
  rawEvent: WheelEvent
}

// Modified from https://github.com/ElyaConrad/zoompinch/blob/main/core/src/helpers.ts
function detectTrackpad(event: WheelEvent): boolean {
  const { deltaY, deltaMode, ctrlKey } = event
  const isPixelMode = deltaMode === WheelEvent.DOM_DELTA_PIXEL

  if (ctrlKey) {
    const absDeltaY = Math.abs(deltaY)
    const hasFractionalDelta = deltaY % 1 !== 0
    return (absDeltaY < 50 && isPixelMode) || hasFractionalDelta
  }

  if ((event as any).wheelDeltaY) {
    if ((event as any).wheelDeltaY === event.deltaY * -3) {
      return true
    }
  } else if (event.deltaMode === WheelEvent.DOM_DELTA_PIXEL) {
    return true
  }
  return false
}

/**
 * Core wheel handling logic shared between Stage wheel events and overlay wheel events.
 * Handles both zoom (ctrl/meta + scroll) and pan (regular scroll).
 */
function applyWheelToStage(stage: Konva.Stage, input: WheelInput) {
  const { rawEvent: event, pointer, inputType } = input
  const { deltaX, deltaY, deltaMode, ctrlKey, metaKey } = event
  const isZoomGesture = ctrlKey || metaKey

  // Normalize delta values based on deltaMode
  const LINE_HEIGHT = 20
  const PAGE_HEIGHT = 100

  let dx = deltaX
  let dy = deltaY

  if (deltaMode === WheelEvent.DOM_DELTA_LINE) {
    dx = deltaX * LINE_HEIGHT
    dy = deltaY * LINE_HEIGHT
  } else if (deltaMode === WheelEvent.DOM_DELTA_PAGE) {
    dx = deltaX * PAGE_HEIGHT
    dy = deltaY * PAGE_HEIGHT
  }

  if (isZoomGesture) {
    const oldScale = stage.scaleX()
    // Use deltaY to calculate smooth zoom factor
    // Smaller intensity = slower zoom
    const zoomIntensity =
      inputType === "touch" || detectTrackpad(event) ? 0.01 : 0.0008
    const zoomFactor = Math.exp(-dy * zoomIntensity)
    const newScale = oldScale * zoomFactor
    const clampedScale = Math.max(
      CANVAS_MIN_ZOOM,
      Math.min(CANVAS_MAX_ZOOM, newScale)
    )

    const mousePointTo = {
      x: (pointer.x - stage.x()) / oldScale,
      y: (pointer.y - stage.y()) / oldScale,
    }

    const newPos = {
      x: pointer.x - mousePointTo.x * clampedScale,
      y: pointer.y - mousePointTo.y * clampedScale,
    }

    stage.scale({ x: clampedScale, y: clampedScale })
    stage.position(newPos)
  } else {
    // Always pan on scroll (unless zooming)
    const newPos = {
      x: stage.x() - dx,
      y: stage.y() - dy,
    }
    stage.position(newPos)
  }
}

function getDistance(
  p1: { x: number; y: number },
  p2: { x: number; y: number }
) {
  return Math.sqrt((p2.x - p1.x) ** 2 + (p2.y - p1.y) ** 2)
}

function getCenter(p1: { x: number; y: number }, p2: { x: number; y: number }) {
  return {
    x: (p1.x + p2.x) / 2,
    y: (p1.y + p2.y) / 2,
  }
}

export function useCamera(
  editor: Editor,
  tool: Tool,
  containerRef?: React.RefObject<HTMLDivElement | null>
) {
  const lastToolRef = React.useRef<Tool>(null)
  const lastInputTypeRef = React.useRef<InputType>(null)
  const isPanning = tool.name === "hand"

  // For manual implementation of panning with middle mouse button
  const lastPanPointerRef = React.useRef<{ x: number; y: number } | null>(null)

  const lastCenterRef = React.useRef<{ x: number; y: number } | null>(null)
  const lastDistRef = React.useRef(0)
  const dragStoppedRef = React.useRef(false)

  const handleWheel = React.useCallback(
    (e: KonvaEventObject<WheelEvent>) => {
      e.evt.preventDefault()

      const stage = editor.stage
      if (!stage) return

      const pointer = stage.getPointerPosition()
      if (!pointer) return

      applyWheelToStage(stage, {
        rawEvent: e.evt,
        pointer,
        inputType: lastInputTypeRef.current,
      })
    },
    [editor]
  )

  // Handle wheel events from overlay elements (like ContextToolbar)
  // This forwards wheel events to the camera system so canvas panning
  // continues smoothly when the cursor passes over overlays
  const handleOverlayWheel = React.useCallback(
    (e: React.WheelEvent) => {
      const stage = editor.stage
      if (!stage) return

      const container = containerRef?.current
      if (!container) return

      // Calculate pointer position relative to the stage
      const rect = container.getBoundingClientRect()
      const pointer = {
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      }

      applyWheelToStage(stage, {
        rawEvent: e.nativeEvent,
        pointer,
        inputType: lastInputTypeRef.current,
      })
    },
    [editor, containerRef]
  )

  const handleMouseDown = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      lastInputTypeRef.current = e.evt.pointerType as InputType

      if (isPanning) {
        e.cancelBubble = true
      }
      if (e.evt.button === 1) {
        e.evt.preventDefault()
        e.cancelBubble = true
        if (!isPanning) {
          editor.setTool({ name: "hand" })
          lastToolRef.current = tool
          lastPanPointerRef.current = editor.stage?.getPointerPosition() ?? null
        }
      }
    },
    [isPanning, tool, editor]
  )

  const handleMouseMove = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      if (isPanning) {
        e.cancelBubble = true
      }

      if (!lastPanPointerRef.current) return

      const stage = editor.stage
      if (!stage) return
      const pos = stage.getPointerPosition()
      if (!pos) return

      stage.position({
        x: stage.x() + pos.x - lastPanPointerRef.current.x,
        y: stage.y() + pos.y - lastPanPointerRef.current.y,
      })
      lastPanPointerRef.current = pos
    },
    [isPanning, editor]
  )

  const handleMouseUp = React.useCallback(
    (e: KonvaEventObject<PointerEvent>) => {
      if (isPanning) {
        e.cancelBubble = true
      }

      if (e.evt.button === 1) {
        e.evt.preventDefault()
        lastPanPointerRef.current = null // Stop manual pan
        if (isPanning) {
          editor.stage?.stopDrag()
          const tool = lastToolRef.current || { name: "select" }
          editor.setTool(tool)
          lastToolRef.current = tool
        }
      }
    },
    [isPanning, editor]
  )

  const handleTouchMove = React.useCallback(
    (e: KonvaEventObject<TouchEvent>) => {
      const touch1 = e.evt.touches[0]
      const touch2 = e.evt.touches[1]
      const stage = e.target.getStage()
      if (!stage) return

      if (
        touch1 &&
        !touch2 &&
        stage.isDragging() === false &&
        dragStoppedRef.current
      ) {
        stage.startDrag()
        dragStoppedRef.current = false
      }

      if (touch1 && touch2) {
        e.evt.preventDefault()
        if (stage.isDragging()) {
          stage.stopDrag()
          dragStoppedRef.current = true
        }

        const rect = stage.container().getBoundingClientRect()
        const p1 = {
          x: touch1.clientX - rect.left,
          y: touch1.clientY - rect.top,
        }
        const p2 = {
          x: touch2.clientX - rect.left,
          y: touch2.clientY - rect.top,
        }

        const newCenter = getCenter(p1, p2)
        const dist = getDistance(p1, p2)

        if (lastCenterRef.current === null) {
          lastCenterRef.current = newCenter
          lastDistRef.current = dist
          return
        }
        if (lastDistRef.current === 0) {
          lastDistRef.current = dist
          return
        }

        const stageScale = stage.scaleX()
        const stagePos = stage.position()
        const pointTo = {
          x: (newCenter.x - stagePos.x) / stageScale,
          y: (newCenter.y - stagePos.y) / stageScale,
        }
        const scale = Math.max(
          CANVAS_MIN_ZOOM,
          Math.min(CANVAS_MAX_ZOOM, stageScale * (dist / lastDistRef.current))
        )
        const dx = newCenter.x - lastCenterRef.current.x
        const dy = newCenter.y - lastCenterRef.current.y
        const newPos = {
          x: newCenter.x - pointTo.x * scale + dx,
          y: newCenter.y - pointTo.y * scale + dy,
        }

        stage.scale({ x: scale, y: scale })
        stage.position(newPos)
        lastDistRef.current = dist
        lastCenterRef.current = newCenter
      }
    },
    []
  )

  const handleTouchEnd = React.useCallback(() => {
    lastDistRef.current = 0
    lastCenterRef.current = null
  }, [])

  const handleDragEnd = React.useCallback(() => {
    dragStoppedRef.current = false
  }, [])

  return {
    handleWheel,
    handleOverlayWheel,
    handleMouseDown,
    handleMouseMove,
    handleMouseUp,
    handleTouchMove,
    handleTouchEnd,
    handleDragEnd,
    // Cursor state for useCursor
    isPanning: tool.name === "hand",
  }
}
