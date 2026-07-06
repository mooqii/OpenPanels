import { useEffect } from "react"
import type { Editor } from "../editor"

export function usePreventBrowserZoom(editor: Editor) {
  useEffect(() => {
    const handleWheelCapture = (evt: WheelEvent) => {
      const isZoomGesture = evt.ctrlKey || evt.metaKey
      if (!isZoomGesture) return

      evt.preventDefault()

      const stage = editor.stage
      const stageContainer = stage?.container()
      const target = evt.target as Node | null

      // If the wheel event is already on the stage container, Konva/React will
      // handle it via the Stage's `onWheel`.
      if (stageContainer && target && stageContainer.contains(target)) {
        return
      }

      // Forward zoom gestures into Konva when hovering overlay UI.
      if (stage) {
        stage.setPointersPositions(evt as any)
        stage.fire("wheel", { evt } as any)
      }
    }

    window.addEventListener("wheel", handleWheelCapture, {
      capture: true,
      passive: false,
    })

    return () => {
      window.removeEventListener("wheel", handleWheelCapture, true)
    }
  }, [editor])
}
