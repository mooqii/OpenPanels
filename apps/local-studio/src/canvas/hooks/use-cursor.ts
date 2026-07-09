import { useEffect } from "react"
import type { ToolName } from "../store"

interface UseCursorOptions {
  containerRef: React.RefObject<HTMLDivElement | null>
  drawCursor?: string | null
  isTextEditing?: boolean
  toolName: ToolName
}

/**
 * Centralized cursor management hook.
 * Observes state from various hooks and determines which cursor to display
 * based on priority.
 *
 * Note: Transform cursor (rotation, resize) is handled directly by Transformer
 * via stage.content.style.cursor.
 *
 * Priority order (highest to lowest):
 * 1. isPanning - Active pan/drag operation ("grabbing")
 * 2. isSpacePressed - Ready to pan ("grab")
 * 3. drawCursor - Drawing tool active
 * 4. default - Fallback
 */
export function useCursor({
  containerRef,
  toolName,
  drawCursor,
  isTextEditing = false,
}: UseCursorOptions): void {
  useEffect(() => {
    if (!containerRef.current) return

    let cursor = "default"

    // Priority-based cursor selection (highest to lowest)
    if (isTextEditing) {
      cursor = "text"
    } else if (toolName === "hand") {
      cursor = "grabbing"
    } else if (drawCursor) {
      cursor = drawCursor
    }

    containerRef.current.style.cursor = cursor
  }, [containerRef, drawCursor, isTextEditing, toolName])
}
