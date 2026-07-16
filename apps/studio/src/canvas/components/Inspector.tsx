import { Button } from "@heroui/react"
import { useCallback } from "react"
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
        <Button
          isDisabled={selectedShapes.length === 0}
          onPress={handleDeleteSelected}
          size="sm"
          variant="danger"
        >
          Delete ({selectedShapes.length})
        </Button>
        <Button onPress={handleGetSnapshot} size="sm" variant="secondary">
          Log Snapshot
        </Button>
        <Button onPress={handleCreatePlaceholder} size="sm" variant="secondary">
          Add placeholder
        </Button>
      </div>
      <div
        style={{
          marginLeft: "16px",
          fontSize: "14px",
          color: "var(--op-text-secondary, CanvasText)",
        }}
      >
        Shapes: {shapes.length} | Selected: {selectedShapes.length}
      </div>
    </div>
  )
}
