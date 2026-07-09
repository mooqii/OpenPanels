import {
  Brush,
  Circle,
  Hand,
  Highlighter,
  ImagePlus,
  MousePointer2,
  Pencil,
  PenTool,
  Slash,
  Spline,
  Square,
  Type,
} from "lucide-react"
import type { ToolbarConfig, ToolConfigItem } from "./types"

export const selectTool: ToolConfigItem = {
  id: "select",
  label: "Select",
  shortcut: "v",
  icon: <MousePointer2 size={16} strokeWidth={1.5} />,
}

export const handTool: ToolConfigItem = {
  id: "hand",
  label: "Hand",
  shortcut: "h",
  icon: <Hand size={16} strokeWidth={1.5} />,
}

export const rectangleTool: ToolConfigItem = {
  id: "rectangle",
  label: "Rectangle",
  shortcut: "r",
  icon: <Square size={16} strokeWidth={1.5} />,
}

export const ellipseTool: ToolConfigItem = {
  id: "ellipse",
  label: "Ellipse",
  shortcut: "o",
  icon: <Circle size={16} strokeWidth={1.5} />,
}

export const lineTool: ToolConfigItem = {
  id: "line",
  label: "Line",
  shortcut: "l",
  icon: <Slash size={16} strokeWidth={1.5} />,
}

export const pencilTool: ToolConfigItem = {
  id: "pencil",
  label: "Pencil",
  shortcut: "p",
  icon: <Pencil size={16} strokeWidth={1.5} />,
}

export const brushTool: ToolConfigItem = {
  id: "brush",
  label: "Brush",
  shortcut: "b",
  icon: <Brush size={16} strokeWidth={1.5} />,
}

export const markerTool: ToolConfigItem = {
  id: "marker",
  label: "Marker",
  shortcut: "m",
  icon: <Highlighter size={16} strokeWidth={1.5} />,
}

export const penTool: ToolConfigItem = {
  id: "pen",
  label: "Pen",
  shortcut: "P",
  icon: <PenTool size={16} strokeWidth={1.5} />,
}

export const textTool: ToolConfigItem = {
  id: "text",
  label: "Text",
  shortcut: "t",
  icon: <Type size={16} strokeWidth={1.5} />,
}

export const connectorTool: ToolConfigItem = {
  id: "connector",
  label: "Connector",
  shortcut: "c",
  icon: <Spline size={16} strokeWidth={1.5} />,
}

export const imageTool: ToolConfigItem = {
  id: "image",
  label: "Add Image",
  icon: <ImagePlus size={16} strokeWidth={1.5} />,
}

/**
 * Default toolbar configuration matching the current toolbar behavior
 */
export const DEFAULT_TOOLBAR_CONFIG: ToolbarConfig[] = [
  {
    group: "select-hand",
    tools: [selectTool, handTool],
  },
  {
    group: "geo",
    tools: [rectangleTool, ellipseTool, lineTool],
  },
  {
    group: "freehand",
    tools: [pencilTool, brushTool, markerTool /*, penTool*/],
  },
  textTool,
  imageTool,
]
