import React from "react"
import { getToolAction } from "../components/tools/tool-mapper"
import { getShortcutMap } from "../components/tools/types"
import { useToolbarConfig } from "../EditorContext"
import type { Editor } from "../editor"
import type { Tool } from "../store"
import type { Shape } from "../types/shapes"
import { handleImagePaste } from "../utils/clipboard"
import { hasNativeTextSelection, isTextInput } from "../utils/event"
import { decodeShapeData, SHAPE_DATA_MARKER } from "./use-clipboard"

interface UseKeyboardOptions {
  allowImagePaste?: boolean
  editor: Editor
  /** Whether pen tool is currently drawing */
  isPenDrawing?: boolean
  /** Callback for copy action */
  onCopy?: () => void
  /** Callback for paste action */
  onPaste?: (options?: { shapes?: Shape[] }) => void
  /** Callback to close and finish pen path */
  onPenClose?: () => void
  /** Callback to finish pen path (open path) */
  onPenFinish?: () => void
  /** Callback to cancel pen path */
  // onPenCancel?: () => void
  /** Callback to remove last point from pen path */
  onPenRemoveLastPoint?: () => void
  selectedShapes: Shape[]
}

export function useKeyboard({
  allowImagePaste = true,
  editor,
  selectedShapes,
  onPenFinish,
  onPenClose,
  // onPenCancel,
  onPenRemoveLastPoint,
  isPenDrawing,
  onCopy,
  onPaste,
}: UseKeyboardOptions) {
  const [isShiftPressed, setIsShiftPressed] = React.useState(false)
  const lastToolRef = React.useRef<Tool>(null)
  const toolbarConfig = useToolbarConfig()
  const shortcutMap = React.useMemo(
    () => getShortcutMap(toolbarConfig),
    [toolbarConfig]
  )

  // Handle keyboard shortcuts
  React.useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (isTextInput(e)) {
        return
      }

      if (e.code === "Space") {
        const tool = editor.getTool()
        if (tool.name !== "hand") {
          editor.setTool({ name: "hand" })
          lastToolRef.current = tool
        }
        return
      }

      if (e.shiftKey) {
        setIsShiftPressed(true)
      }

      // Delete selected shapes
      if (
        (e.key === "Delete" || e.key === "Backspace") &&
        selectedShapes.length > 0
      ) {
        e.preventDefault()
        editor.run(() => {
          for (const shape of selectedShapes) {
            editor.deleteShape(shape)
          }
        })
      }

      // Copy: Cmd/Ctrl + C
      if (
        (e.ctrlKey || e.metaKey) &&
        e.key === "c" &&
        selectedShapes.length > 0
      ) {
        if (hasNativeTextSelection()) {
          return
        }

        e.preventDefault()
        onCopy?.()
      }

      // Note: Paste is handled by the native 'paste' event listener to support
      // both image paste from system clipboard and in-app shape paste

      // Undo: Cmd/Ctrl + Z
      if ((e.ctrlKey || e.metaKey) && e.key === "z" && !e.shiftKey) {
        e.preventDefault()
        editor.undo()
      }

      // Redo: Cmd/Ctrl + Shift + Z
      if ((e.ctrlKey || e.metaKey) && e.key === "z" && e.shiftKey) {
        e.preventDefault()
        editor.redo()
      }

      // Group: Cmd/Ctrl + G
      if ((e.ctrlKey || e.metaKey) && e.key === "g" && !e.shiftKey) {
        e.preventDefault()
        editor.groupSelectedShapes()
      }

      // Ungroup: Cmd/Ctrl + Shift + G
      if ((e.ctrlKey || e.metaKey) && e.key === "g" && e.shiftKey) {
        e.preventDefault()
        editor.ungroupSelectedShapes()
      }

      // Merge connectors: Cmd/Ctrl + M
      if ((e.ctrlKey || e.metaKey) && e.key === "m") {
        e.preventDefault()
        const merged = editor.mergeSelectedConnectors()
        if (!merged) {
          // Merge not possible - connectors must share a common source or target
          console.log(
            "Cannot merge: connectors must share a common source or target"
          )
        }
      }

      // Pen tool shortcuts
      if (isPenDrawing) {
        // Escape: Cancel current path
        if (e.key === "Escape") {
          e.preventDefault()
          onPenFinish?.()
          editor.setTool({ name: "select" })
          lastToolRef.current = null
          return
        }

        // Enter: Finish path (closed)
        if (e.key === "Enter") {
          e.preventDefault()
          onPenClose?.()
        }

        // Backspace/Delete while drawing: Remove last point
        if (e.key === "Backspace" || e.key === "Delete") {
          e.preventDefault()
          onPenRemoveLastPoint?.()
        }
      }

      // Escape when pen tool is active but not drawing: Switch to select
      const tool = editor.getTool()
      if (tool.name === "pen" && !isPenDrawing && e.key === "Escape") {
        e.preventDefault()
        editor.setTool({ name: "select" })
        lastToolRef.current = null
        return
      }

      // Escape switches any non-text editing tool back to the default select tool.
      if (e.key === "Escape") {
        const currentTool = editor.getTool()
        if (currentTool.name !== "select") {
          e.preventDefault()
          editor.setTool({ name: "select" })
          lastToolRef.current = null
        }
        return
      }

      // Tool shortcuts (single key, no modifiers)
      // Use toolbar config to determine which shortcuts are enabled
      const hasModifier = e.ctrlKey || e.metaKey || e.altKey
      if (!hasModifier) {
        const keyPressed = e.key
        const keyLower = keyPressed.toLowerCase()

        // Check exact case first (for case-sensitive shortcuts like "P" vs "p")
        let toolId = shortcutMap.get(keyPressed)

        // If no exact match and key pressed is lowercase, try lowercase version
        // This allows case-insensitive matching for lowercase shortcuts
        if (!toolId && keyPressed === keyLower) {
          toolId = shortcutMap.get(keyLower)
        }

        if (toolId) {
          const currentTool = editor.getTool()
          const toolAction = getToolAction(toolId, editor, currentTool)
          if (toolAction) {
            e.preventDefault()
            toolAction()
          }
        }
      }
    }

    const handleKeyUp = (e: KeyboardEvent) => {
      if (isTextInput(e)) {
        return
      }

      if (e.code === "Space") {
        const tool = lastToolRef.current || { name: "select" }
        editor.setTool(tool)
        lastToolRef.current = null
      }
      if (!e.shiftKey && isShiftPressed) {
        setIsShiftPressed(false)
      }
    }

    // Handle paste event
    const handlePaste = async (e: ClipboardEvent) => {
      // Don't handle paste in text inputs
      if (isTextInput(e)) {
        return
      }

      const clipboardData = e.clipboardData
      if (!clipboardData) return

      // Check for our encoded shape data in text/plain first (highest priority)
      const textData = clipboardData.getData("text/plain")
      if (textData?.startsWith(SHAPE_DATA_MARKER)) {
        e.preventDefault()
        // Decode shape data and use it directly
        const decodedShapes = decodeShapeData(textData)
        if (decodedShapes && decodedShapes.length > 0) {
          // Pass decoded shapes to handlePaste
          onPaste?.({ shapes: decodedShapes })
        }
        return
      }

      // Try to paste image
      if (allowImagePaste) {
        const assetStore = editor.getAssetStore()
        const imageHandled = await handleImagePaste(
          editor,
          clipboardData,
          assetStore
        )
        if (imageHandled) {
          e.preventDefault()
          return
        }
      }

      // If neither handled it, try in-app shape paste as fallback
      onPaste?.()
    }

    window.addEventListener("keydown", handleKeyDown)
    window.addEventListener("keyup", handleKeyUp)
    window.addEventListener("paste", handlePaste)

    return () => {
      window.removeEventListener("keydown", handleKeyDown)
      window.removeEventListener("keyup", handleKeyUp)
      window.removeEventListener("paste", handlePaste)
    }
  }, [
    selectedShapes,
    editor,
    allowImagePaste,
    isShiftPressed,
    isPenDrawing,
    onPenClose,
    onPenFinish,
    onPenRemoveLastPoint,
    shortcutMap,
    onCopy,
    onPaste,
  ])

  return {
    isShiftPressed,
  }
}
