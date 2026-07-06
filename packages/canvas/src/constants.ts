export const ALIGNMENT_GUIDE_COLOR = "#f50057"

export const TRANSFORMER_STROKE_COLOR = "rgb(0, 161, 255)"
export const TRANSFORMER_BORDER_STROKE_WIDTH = 1
export const TRANSFORMER_FILL_COLOR = "#fff"
export const TRANSFORMER_ANCHOR_SIZE = 8

export const SELECTION_RECT_COLOR = "rgba(0, 0, 255, 0.1)"

export const CANVAS_MIN_ZOOM = 0.02
export const CANVAS_MAX_ZOOM = 5

// Hover effect defaults
export const HOVER_OUTLINE_COLOR = "#006FEE" // Match transformer color
export const HOVER_OUTLINE_WIDTH = 2

export const INITIAL_GEO_FILL = "rgba(217, 217, 217)"
export const INITIAL_GEO_STROKE = "rgba(0, 0, 0)"

export const CANVAS_PLACEHOLDER_COLOR_VARIABLE = "--canvas-placeholder"
export const INITIAL_PLACEHOLDER_FILL = "oklch(92.05% 0.0067 106.53)"

export function resolveCanvasPlaceholderFill() {
  if (typeof window === "undefined") {
    return INITIAL_PLACEHOLDER_FILL
  }

  const fill = window
    .getComputedStyle(document.documentElement)
    .getPropertyValue(CANVAS_PLACEHOLDER_COLOR_VARIABLE)
    .trim()

  return fill || INITIAL_PLACEHOLDER_FILL
}

// Pencil drawing defaults
export const PENCIL_STROKE_COLOR = "#df4b26"
export const PENCIL_STROKE_WIDTH_THIN = 2
export const PENCIL_STROKE_WIDTH = 4
export const PENCIL_STROKE_WIDTH_THICK = 6

// Pen tool defaults
export const PEN_STROKE_COLOR = "#1a1a1a"
export const PEN_STROKE_WIDTH = 1
export const PEN_ANCHOR_SIZE = 8
export const PEN_ANCHOR_FILL = "#fff"
export const PEN_ANCHOR_STROKE = "#006FEE"
export const PEN_HANDLE_SIZE = 6
export const PEN_HANDLE_LINE_COLOR = "#006FEE"
export const PEN_CLOSE_THRESHOLD = 12 // Distance to auto-close path

// Brush tool defaults
export const BRUSH_STROKE_COLOR = "#3B82F6"
export const BRUSH_STROKE_WIDTH = 32
export const BRUSH_MIN_WIDTH = 1
export const BRUSH_MAX_WIDTH = 24
export const BRUSH_SMOOTHING_FACTOR = 0.3
export const BRUSH_MAX_VELOCITY = 800 // pixels per second for minimum pressure

// Marker tool defaults
export const MARKER_STROKE_COLOR = "#FACC15" // Yellow highlighter color
export const MARKER_STROKE_WIDTH = 32
export const MARKER_DEFAULT_OPACITY = 0.5

// Text tool defaults
export const TEXT_DEFAULT_COLOR = "#ef4444"
export const TEXT_DEFAULT_FONT_FAMILY = "Arial"
export const TEXT_DEFAULT_FONT_SIZE = 32
export const TEXT_DEFAULT_FONT_WEIGHT = "normal"
export const TEXT_DEFAULT_ALIGN = "left"
export const TEXT_DEFAULT_LINE_HEIGHT = 1.2
export const TEXT_MIN_BOX_WIDTH = 16
export const TEXT_FONT_SIZE_MIN = 12
export const TEXT_FONT_SIZE_MAX = 128
export const TEXT_FONT_SIZE_OPTIONS = [
  12, 13, 14, 15, 16, 20, 24, 32, 36, 40, 48, 64, 96, 128,
] as const
