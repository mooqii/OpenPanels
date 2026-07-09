import type { Editor } from "../../editor"
import type { Tool } from "../../store"
import { readPersistedCanvasTool } from "../../tool-persistence"

/**
 * Maps tool IDs to editor.setTool calls
 * Returns a function that when called, sets the appropriate tool
 */
export function getToolAction(
  toolId: string,
  editor: Editor,
  currentTool: Tool
): (() => void) | null {
  switch (toolId) {
    case "select":
      return () => {
        if (currentTool.name === "select") {
          // Already selected, do nothing
          return
        }
        editor.setTool({ name: "select" })
      }

    case "hand":
      return () => {
        if (currentTool.name === "hand") {
          editor.setTool({ name: "select" })
        } else {
          editor.setTool({ name: "hand" })
        }
      }

    case "rectangle":
      return () => {
        if (currentTool.name === "draw" && currentTool.shape === "rectangle") {
          editor.setTool({ name: "select" })
        } else {
          editor.setTool({ name: "draw", shape: "rectangle" })
        }
      }

    case "ellipse":
      return () => {
        if (currentTool.name === "draw" && currentTool.shape === "ellipse") {
          editor.setTool({ name: "select" })
        } else {
          editor.setTool({ name: "draw", shape: "ellipse" })
        }
      }

    case "line":
      return () => {
        if (currentTool.name === "draw" && currentTool.shape === "line") {
          editor.setTool({ name: "select" })
        } else {
          editor.setTool({ name: "draw", shape: "line" })
        }
      }

    case "pencil":
      return () => {
        if (currentTool.name === "pencil") {
          editor.setTool({ name: "select" })
        } else {
          editor.setTool(readPersistedCanvasTool("pencil"))
        }
      }

    case "brush":
      return () => {
        if (currentTool.name === "brush") {
          editor.setTool({ name: "select" })
        } else {
          editor.setTool(readPersistedCanvasTool("brush"))
        }
      }

    case "marker":
      return () => {
        if (currentTool.name === "marker") {
          editor.setTool({ name: "select" })
        } else {
          editor.setTool(readPersistedCanvasTool("marker"))
        }
      }

    case "pen":
      return () => {
        if (currentTool.name === "pen") {
          editor.setTool({ name: "select" })
        } else {
          editor.setTool({ name: "pen" })
        }
      }

    case "text":
      return () => {
        if (currentTool.name === "text") {
          editor.setTool({ name: "select" })
        } else {
          editor.setTool(readPersistedCanvasTool("text"))
        }
      }

    case "connector":
      return () => {
        if (currentTool.name === "connector") {
          editor.setTool({ name: "select" })
        } else {
          editor.setTool({ name: "connector" })
        }
      }

    case "image":
      // Image tool is special - it doesn't set a tool, it opens a file picker
      // This will be handled separately in the component
      return null

    default:
      console.warn(`Unknown tool ID: ${toolId}`)
      return null
  }
}

/**
 * Checks if a tool is currently active based on its ID
 */
export function isToolActive(toolId: string, currentTool: Tool): boolean {
  switch (toolId) {
    case "select":
      return currentTool.name === "select"

    case "hand":
      return currentTool.name === "hand"

    case "rectangle":
      return currentTool.name === "draw" && currentTool.shape === "rectangle"

    case "ellipse":
      return currentTool.name === "draw" && currentTool.shape === "ellipse"

    case "line":
      return currentTool.name === "draw" && currentTool.shape === "line"

    case "pencil":
      return currentTool.name === "pencil"

    case "brush":
      return currentTool.name === "brush"

    case "marker":
      return currentTool.name === "marker"

    case "pen":
      return currentTool.name === "pen"

    case "text":
      return currentTool.name === "text"

    case "connector":
      return currentTool.name === "connector"

    case "image":
      // Image tool doesn't have an active state
      return false

    default:
      return false
  }
}

/**
 * Checks if any tool in a group is active
 */
export function isGroupActive(toolIds: string[], currentTool: Tool): boolean {
  return toolIds.some((id) => isToolActive(id, currentTool))
}

/**
 * Gets the active tool ID from a group, or null if none are active
 */
export function getActiveToolInGroup(
  toolIds: string[],
  currentTool: Tool
): string | null {
  for (const id of toolIds) {
    if (isToolActive(id, currentTool)) {
      return id
    }
  }
  return null
}
