import {
  TEXT_DEFAULT_ALIGN,
  TEXT_DEFAULT_COLOR,
  TEXT_DEFAULT_FONT_FAMILY,
  TEXT_DEFAULT_FONT_SIZE,
  TEXT_DEFAULT_FONT_WEIGHT,
  TEXT_FONT_SIZE_MAX,
  TEXT_FONT_SIZE_MIN,
} from "./constants"
import type { TextTool, TextToolAlign, TextToolFontWeight } from "./store"
import type { TextShape } from "./types/shapes"

export function clampTextFontSize(value: number): number {
  return Math.min(TEXT_FONT_SIZE_MAX, Math.max(TEXT_FONT_SIZE_MIN, value))
}

export function normalizeTextToolAlign(value: unknown): TextToolAlign {
  return value === "center" || value === "right" ? value : TEXT_DEFAULT_ALIGN
}

export function normalizeTextToolFontWeight(
  value: unknown
): TextToolFontWeight {
  if (typeof value !== "string") {
    return TEXT_DEFAULT_FONT_WEIGHT
  }

  return value.includes("700") || value.includes("bold") ? "700" : "normal"
}

export function getDefaultTextTool(): TextTool {
  return {
    name: "text",
    align: TEXT_DEFAULT_ALIGN,
    color: TEXT_DEFAULT_COLOR,
    fontFamily: TEXT_DEFAULT_FONT_FAMILY,
    fontSize: TEXT_DEFAULT_FONT_SIZE,
    fontWeight: TEXT_DEFAULT_FONT_WEIGHT,
  }
}

export function getTextToolFromShape(
  shape: TextShape,
  zoom: number,
  overrides: Partial<Omit<TextTool, "name">> = {}
): TextTool {
  const defaultTool = getDefaultTextTool()
  const safeZoom = zoom > 0 ? zoom : 1
  const fill = shape.props.fill
  const fontFamily = shape.props.fontFamily
  const fontSize = shape.props.fontSize

  return {
    name: "text",
    align: normalizeTextToolAlign(shape.props.align),
    color: typeof fill === "string" && fill.trim().length > 0 ? fill : "black",
    fontFamily:
      typeof fontFamily === "string" && fontFamily.trim().length > 0
        ? fontFamily
        : defaultTool.fontFamily,
    fontSize:
      typeof fontSize === "number" && Number.isFinite(fontSize) && fontSize > 0
        ? clampTextFontSize(Math.round(fontSize * safeZoom))
        : defaultTool.fontSize,
    fontWeight: normalizeTextToolFontWeight(shape.props.fontStyle),
    ...overrides,
  }
}

export function toTextShapeFontSize(value: number, zoom: number): number {
  const safeZoom = zoom > 0 ? zoom : 1
  return clampTextFontSize(Math.round(value)) / safeZoom
}

export function toTextShapeFontStyle(
  value: TextToolFontWeight | undefined
): string {
  return value === "700" ? "700" : "normal"
}
