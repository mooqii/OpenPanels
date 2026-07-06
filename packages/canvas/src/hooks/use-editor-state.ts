import React, { useMemo } from "react"
import type { Editor } from "../editor"
import type { ShapeId } from "../types/ids"

function useSubscribe(editor: Editor) {
  return React.useCallback(
    (cb: () => void) => {
      return editor.subscribe(cb)
    },
    [editor]
  )
}

export function useSelectedShapes(editor: Editor) {
  return React.useSyncExternalStore(useSubscribe(editor), () =>
    editor.getSelectedShapes()
  )
}

export function useOpenedGroupId(editor: Editor) {
  return editor.useStore((state) => state.openedGroupId)
}

export function useCurrentPageShapes(editor: Editor) {
  return React.useSyncExternalStore(useSubscribe(editor), () =>
    editor.getCurrentPageShapes()
  )
}

export function useTool(editor: Editor) {
  return React.useSyncExternalStore(useSubscribe(editor), () =>
    editor.getTool()
  )
}

/**
 * Hook to get children of a group shape reactively.
 * Returns children sorted by index, and updates when shapes change.
 */
export function useShapeChildren(editor: Editor, parentId: ShapeId) {
  const shapes = useCurrentPageShapes(editor)
  return useMemo(
    () =>
      shapes
        .filter((s) => s.parentId === parentId)
        .sort((a, b) => a.index - b.index),
    [shapes, parentId]
  )
}
