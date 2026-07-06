// Tool types that can be shown in the toolbar
export type ToolType =
  | "fill"
  | "stroke"
  | "shadow"
  | "corner-radius"
  | "dimensions"
  | "download"
  | "info"
  | "divider"
  | "text-color"
  | "text-font"
  | "text-style"
  | "text-size"
  | "text-align"
  | "group"
  | "ungroup"
  | "rasterize"
  | "crop"

export type CommandToolConfig = {
  type: "command"
  command: string
  icon: React.ReactNode
  label?: string
  tooltip?: string | (() => string)
}

// Tool configuration
export type ToolConfig =
  | {
      type: ToolType
    }
  | CommandToolConfig

// Tools configuration by shape type
export const GEO_TOOLS: ToolConfig[] = [
  { type: "fill" },
  { type: "stroke" },
  { type: "shadow" },
  { type: "corner-radius" },
  { type: "divider" },
  { type: "dimensions" },
  { type: "divider" },
  { type: "download" },
]

export const GEO_ELLIPSE_TOOLS: ToolConfig[] = [
  { type: "fill" },
  { type: "stroke" },
  { type: "shadow" },
  { type: "divider" },
  { type: "dimensions" },
  { type: "divider" },
  { type: "download" },
]

export const GEO_LINE_TOOLS: ToolConfig[] = [
  { type: "stroke" },
  { type: "divider" },
  { type: "dimensions" },
  { type: "divider" },
  { type: "download" },
]

export const IMAGE_TOOLS: ToolConfig[] = [
  { type: "crop" },
  { type: "download" },
  { type: "info" },
]

export const TEXT_TOOLS: ToolConfig[] = [
  { type: "text-color" },
  { type: "text-font" },
  { type: "text-style" },
  { type: "text-size" },
  { type: "text-align" },
  { type: "divider" },
  { type: "download" },
]

export const PATH_TOOLS: ToolConfig[] = [
  { type: "fill" },
  { type: "stroke" },
  { type: "divider" },
  { type: "download" },
]

export const MULTI_TOOLS: ToolConfig[] = [
  { type: "rasterize" },
  { type: "group" },
  { type: "ungroup" },
  { type: "divider" },
  { type: "download" },
]
