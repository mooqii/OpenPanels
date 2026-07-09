import { TEXT_DEFAULT_LINE_HEIGHT, TEXT_MIN_BOX_WIDTH } from "./constants"
import type { TextBoxSizeMode, TextShapeProps } from "./types/shapes"
import { getLineBreaks, measureText, type WrapMode } from "./utils/text-measure"

export function getTextBoxWidthMode(
  props: Partial<TextShapeProps>,
  fallback: TextBoxSizeMode = "manual"
): TextBoxSizeMode {
  return props.textBoxWidthMode === "auto" ||
    props.textBoxWidthMode === "manual"
    ? props.textBoxWidthMode
    : fallback
}

export function getTextBoxHeightMode(
  props: Partial<TextShapeProps>,
  fallback: TextBoxSizeMode = "auto"
): TextBoxSizeMode {
  return props.textBoxHeightMode === "auto" ||
    props.textBoxHeightMode === "manual"
    ? props.textBoxHeightMode
    : fallback
}

function getMaxLineWidth(
  text: string,
  fontFamily: string,
  fontSize: number,
  fontStyle: string,
  measureLineWidth?: (
    line: string,
    fontFamily: string,
    fontSize: number,
    fontStyle: string
  ) => number | undefined
): number {
  const lines = text.split("\n")
  let maxWidth = 0

  for (const line of lines) {
    const measuredWidth = measureLineWidth?.(
      line,
      fontFamily,
      fontSize,
      fontStyle
    )
    const lineWidth =
      typeof measuredWidth === "number" && Number.isFinite(measuredWidth)
        ? measuredWidth
        : measureText(line, fontFamily, fontSize, fontStyle).width
    if (lineWidth > maxWidth) {
      maxWidth = lineWidth
    }
  }

  return maxWidth
}

export function calculateTextLayout({
  text,
  fontFamily,
  fontSize,
  fontStyle,
  lineHeight,
  width,
  height,
  widthMode,
  heightMode,
  minWidth = TEXT_MIN_BOX_WIDTH,
  measureLineWidth,
  wrap = "word",
}: {
  fontFamily: string
  fontSize: number
  fontStyle: string
  height: number
  heightMode: TextBoxSizeMode
  lineHeight: number
  measureLineWidth?: (
    line: string,
    fontFamily: string,
    fontSize: number,
    fontStyle: string
  ) => number | undefined
  minWidth?: number
  text: string
  width: number
  widthMode: TextBoxSizeMode
  wrap?: WrapMode
}): {
  contentHeight: number
  height: number
  width: number
} {
  const safeFontSize = fontSize > 0 ? fontSize : 16
  const safeLineHeight = lineHeight > 0 ? lineHeight : TEXT_DEFAULT_LINE_HEIGHT
  const lineHeightPx = safeLineHeight * safeFontSize
  const resolvedMinWidth = Math.max(minWidth, Math.round(safeFontSize * 0.75))
  const preferredAutoWidth = Math.max(
    resolvedMinWidth,
    Math.ceil(
      getMaxLineWidth(
        text,
        fontFamily,
        safeFontSize,
        fontStyle,
        measureLineWidth
      )
    )
  )
  const resolvedWidth =
    widthMode === "auto"
      ? preferredAutoWidth
      : Math.max(resolvedMinWidth, width)
  const lines = getLineBreaks(
    text,
    resolvedWidth,
    fontFamily,
    safeFontSize,
    fontStyle,
    wrap
  )
  const contentHeight = Math.max(lineHeightPx, lines.length * lineHeightPx)

  return {
    width: resolvedWidth,
    contentHeight,
    height:
      heightMode === "manual" ? Math.max(height, contentHeight) : contentHeight,
  }
}

export function getTextPropsWithUpdatedLayout(
  currentProps: Partial<TextShapeProps>,
  overrides: Partial<TextShapeProps>,
  options?: {
    fallbackHeightMode?: TextBoxSizeMode
    fallbackWidthMode?: TextBoxSizeMode
    measureLineWidth?: (
      line: string,
      fontFamily: string,
      fontSize: number,
      fontStyle: string
    ) => number | undefined
  }
): Partial<TextShapeProps> {
  const nextProps = {
    ...currentProps,
    ...overrides,
  }
  const text = (nextProps.text as string | undefined) ?? ""
  const fontFamily = (nextProps.fontFamily as string | undefined) ?? "Arial"
  const fontSize = (nextProps.fontSize as number | undefined) ?? 16
  const fontStyle = (nextProps.fontStyle as string | undefined) ?? "normal"
  const lineHeight =
    (nextProps.lineHeight as number | undefined) ?? TEXT_DEFAULT_LINE_HEIGHT
  const wrap = ((nextProps.wrap as WrapMode | undefined) ?? "word") as WrapMode
  const widthMode = getTextBoxWidthMode(
    nextProps,
    options?.fallbackWidthMode ?? "manual"
  )
  const heightMode = getTextBoxHeightMode(
    nextProps,
    options?.fallbackHeightMode ?? "auto"
  )
  const { width, height } = calculateTextLayout({
    text,
    fontFamily,
    fontSize,
    fontStyle,
    height: (nextProps.height as number | undefined) ?? 0,
    heightMode,
    lineHeight,
    measureLineWidth: options?.measureLineWidth,
    width: (nextProps.width as number | undefined) ?? 0,
    widthMode,
    wrap,
  })

  return {
    ...nextProps,
    wrap,
    width,
    height,
    verticalAlign: "top",
    textBoxWidthMode: widthMode,
    textBoxHeightMode: heightMode,
  }
}

export function getTextPropsWithUpdatedTransformLayout(
  currentProps: Partial<TextShapeProps>,
  overrides: Pick<
    Partial<TextShapeProps>,
    "height" | "rotation" | "scaleX" | "scaleY" | "width" | "x" | "y"
  >,
  widthMode: TextBoxSizeMode
): Partial<TextShapeProps> {
  return getTextPropsWithUpdatedLayout(
    currentProps,
    {
      ...overrides,
      textBoxHeightMode: "auto",
      textBoxWidthMode: widthMode,
    },
    {
      fallbackHeightMode: "auto",
      fallbackWidthMode: widthMode,
    }
  )
}
