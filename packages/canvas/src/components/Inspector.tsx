import { cn } from "@heroui/react"
import { useCallback } from "react"
import { INITIAL_PLACEHOLDER_FILL } from "../constants"
import { useEditor } from "../EditorContext"
import {
  useCurrentPageShapes,
  useSelectedShapes,
} from "../hooks/use-editor-state"

export function Inspector() {
  const editor = useEditor()

  const handleCreatePlaceholder = useCallback(() => {
    editor.run(() => {
      // Slight random offset per click so multiple placeholders don't stack exactly (for testing)
      const offsetX = Math.round(Math.random() * 60)
      const offsetY = Math.round(Math.random() * 60)
      editor.createShape({
        type: "placeholder",
        props: {
          x: 100 + offsetX,
          y: 100 + offsetY,
          width: 200,
          height: 120,
          fill: INITIAL_PLACEHOLDER_FILL,
          cornerRadius: 0,
          text: "Placeholder",
        },
      })
    })
  }, [editor])

  const handleDeleteSelected = useCallback(() => {
    const selected = editor.getSelectedShapes()
    editor.run(() => {
      for (const shape of selected) {
        editor.deleteShape(shape)
      }
    })
  }, [editor])

  const handleGetSnapshot = useCallback(() => {
    const snapshot = editor.getSnapshot()
    console.log("Snapshot:", snapshot)
    console.log("Store records:", Object.keys(snapshot.store).length)
  }, [editor])

  const shapes = useCurrentPageShapes(editor)
  const selectedShapes = useSelectedShapes(editor)

  return (
    <div className="flex flex-col">
      <div style={{ marginLeft: "auto", display: "flex", gap: "8px" }}>
        <button
          className={cn(
            "rounded px-4 py-2",
            selectedShapes.length > 0
              ? "cursor-pointer bg-red-500 text-white"
              : "cursor-not-allowed bg-gray-400"
          )}
          disabled={selectedShapes.length === 0}
          onClick={handleDeleteSelected}
          type="button"
        >
          Delete ({selectedShapes.length})
        </button>
        <button
          className="rounded bg-gray-400 px-4 py-2"
          onClick={handleGetSnapshot}
          type="button"
        >
          Log Snapshot
        </button>
        <button
          className="rounded bg-gray-400 px-4 py-2"
          onClick={handleCreatePlaceholder}
          type="button"
        >
          Add placeholder
        </button>
      </div>
      <div style={{ marginLeft: "16px", fontSize: "14px", color: "#666" }}>
        Shapes: {shapes.length} | Selected: {selectedShapes.length}
      </div>
    </div>
  )
}
