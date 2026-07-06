import {
  BRUSH_STROKE_COLOR,
  BRUSH_STROKE_WIDTH,
  MARKER_DEFAULT_OPACITY,
  MARKER_STROKE_COLOR,
  MARKER_STROKE_WIDTH,
  PENCIL_STROKE_COLOR,
  PENCIL_STROKE_WIDTH,
  TEXT_DEFAULT_ALIGN,
  TEXT_DEFAULT_COLOR,
  TEXT_DEFAULT_FONT_FAMILY,
  TEXT_DEFAULT_FONT_SIZE,
  TEXT_DEFAULT_FONT_WEIGHT,
} from "./constants"
import type { Tool } from "./store"

const STORAGE_KEY = "creart-canvas-tool-settings"

type PersistedToolName = "brush" | "marker" | "pencil" | "text"
type PersistedTool<TName extends PersistedToolName> = Extract<
  Tool,
  { name: TName }
>

interface PersistedBrushTool {
  color: string
  size: number
}

interface PersistedMarkerTool extends PersistedBrushTool {
  opacity: number
}

interface PersistedTextTool {
  align: "left" | "center" | "right"
  color: string
  fontFamily: string
  fontSize: number
  fontWeight: "normal" | "700"
}

interface PersistedCanvasToolSettings {
  brush?: PersistedBrushTool
  marker?: PersistedMarkerTool
  pencil?: PersistedBrushTool
  text?: PersistedTextTool
}

const DEFAULT_TOOL_SETTINGS: {
  [TName in PersistedToolName]: PersistedTool<TName>
} = {
  pencil: {
    name: "pencil",
    color: PENCIL_STROKE_COLOR,
    size: PENCIL_STROKE_WIDTH,
  },
  brush: {
    name: "brush",
    color: BRUSH_STROKE_COLOR,
    size: BRUSH_STROKE_WIDTH,
  },
  marker: {
    name: "marker",
    color: MARKER_STROKE_COLOR,
    size: MARKER_STROKE_WIDTH,
    opacity: MARKER_DEFAULT_OPACITY,
  },
  text: {
    name: "text",
    align: TEXT_DEFAULT_ALIGN,
    color: TEXT_DEFAULT_COLOR,
    fontFamily: TEXT_DEFAULT_FONT_FAMILY,
    fontSize: TEXT_DEFAULT_FONT_SIZE,
    fontWeight: TEXT_DEFAULT_FONT_WEIGHT,
  },
}

function isPersistedToolName(value: Tool["name"]): value is PersistedToolName {
  return (
    value === "pencil" ||
    value === "brush" ||
    value === "marker" ||
    value === "text"
  )
}

function isValidColor(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0
}

function isValidSize(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0
}

function isValidOpacity(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isFinite(value) &&
    value >= 0 &&
    value <= 1
  )
}

function isValidTextAlign(
  value: unknown
): value is "left" | "center" | "right" {
  return value === "left" || value === "center" || value === "right"
}

function isValidFontWeight(value: unknown): value is "normal" | "700" {
  return value === "normal" || value === "700"
}

function getDefaultTool<TName extends PersistedToolName>(
  toolName: TName
): PersistedTool<TName> {
  return { ...DEFAULT_TOOL_SETTINGS[toolName] }
}

function readPersistedToolSettings(): PersistedCanvasToolSettings {
  if (typeof window === "undefined") {
    return {}
  }

  try {
    const storedValue = window.localStorage.getItem(STORAGE_KEY)
    if (!storedValue) return {}

    const parsedValue: unknown = JSON.parse(storedValue)
    if (!parsedValue || typeof parsedValue !== "object") {
      return {}
    }

    return parsedValue as PersistedCanvasToolSettings
  } catch {
    return {}
  }
}

function persistToolSettings(nextSettings: PersistedCanvasToolSettings) {
  if (typeof window === "undefined") return

  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(nextSettings))
  } catch {
    // Ignore storage failures and keep the in-memory tool settings.
  }
}

export function readPersistedCanvasTool<TName extends PersistedToolName>(
  toolName: TName
): PersistedTool<TName> {
  const defaults = getDefaultTool(toolName)
  const persistedSettings = readPersistedToolSettings()[toolName]

  if (!persistedSettings || typeof persistedSettings !== "object") {
    return defaults
  }

  if (toolName === "marker") {
    const markerSettings = persistedSettings as PersistedMarkerTool

    if (
      !(
        isValidColor(markerSettings.color) &&
        isValidSize(markerSettings.size) &&
        isValidOpacity(markerSettings.opacity)
      )
    ) {
      return defaults
    }

    return {
      name: "marker",
      color: markerSettings.color,
      size: markerSettings.size,
      opacity: markerSettings.opacity,
    } as PersistedTool<TName>
  }

  if (toolName === "text") {
    if (
      !(
        isValidTextAlign((persistedSettings as PersistedTextTool).align) &&
        isValidColor((persistedSettings as PersistedTextTool).color) &&
        isValidColor((persistedSettings as PersistedTextTool).fontFamily) &&
        isValidSize((persistedSettings as PersistedTextTool).fontSize) &&
        isValidFontWeight((persistedSettings as PersistedTextTool).fontWeight)
      )
    ) {
      return defaults
    }

    return {
      name: "text",
      align: (persistedSettings as PersistedTextTool).align,
      color: (persistedSettings as PersistedTextTool).color,
      fontFamily: (persistedSettings as PersistedTextTool).fontFamily,
      fontSize: (persistedSettings as PersistedTextTool).fontSize,
      fontWeight: (persistedSettings as PersistedTextTool).fontWeight,
    } as PersistedTool<TName>
  }

  if (
    !(
      isValidColor((persistedSettings as PersistedBrushTool).color) &&
      isValidSize((persistedSettings as PersistedBrushTool).size)
    )
  ) {
    return defaults
  }

  return {
    name: toolName,
    color: (persistedSettings as PersistedBrushTool).color,
    size: (persistedSettings as PersistedBrushTool).size,
  } as PersistedTool<TName>
}

export function persistCanvasTool(tool: Tool): void {
  if (!isPersistedToolName(tool.name)) return

  const currentSettings = readPersistedToolSettings()

  if (tool.name === "marker") {
    persistToolSettings({
      ...currentSettings,
      marker: {
        color: tool.color,
        size: tool.size,
        opacity: tool.opacity,
      },
    })
    return
  }

  if (tool.name === "text") {
    persistToolSettings({
      ...currentSettings,
      text: {
        align: tool.align,
        color: tool.color,
        fontFamily: tool.fontFamily,
        fontSize: tool.fontSize,
        fontWeight: tool.fontWeight,
      },
    })
    return
  }

  if (tool.name === "brush" || tool.name === "pencil") {
    persistToolSettings({
      ...currentSettings,
      [tool.name]: {
        color: tool.color,
        size: tool.size,
      },
    })
  }
}
