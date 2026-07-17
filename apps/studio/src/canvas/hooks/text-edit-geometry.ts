import type Konva from "konva"
import { TEXT_DEFAULT_LINE_HEIGHT, TEXT_MIN_BOX_WIDTH } from "../constants"
import type { TextBoxSizeMode, TextShapeProps } from "../types/shapes"

export const CARET_WIDTH = 1.5

export interface TextEditRect {
  height: number
  width: number
  x: number
  y: number
}

export interface TextEditOverlay {
  caretRect: TextEditRect | null
  height: number
  rotation: number
  scaleX: number
  scaleY: number
  selectionRects: TextEditRect[]
  width: number
  x: number
  y: number
}

export interface TextSelectionRange {
  end: number
  start: number
}

export interface TextSelectionState {
  cursorIndex: number
  selectionRange: TextSelectionRange | null
}

export interface TextOverlayTransform {
  rotation: number
  scaleX: number
  scaleY: number
  x: number
  y: number
}

export function areSelectionRangesEqual(
  left: TextSelectionRange | null,
  right: TextSelectionRange | null
): boolean {
  if (left === right) return true
  if (!(left && right)) return false

  return left.start === right.start && left.end === right.end
}

export function createTextMeasureNode(
  props: Partial<TextShapeProps>
): Konva.Text {
  const align = typeof props.align === "string" ? props.align : "left"
  const fontFamily =
    typeof props.fontFamily === "string" && props.fontFamily.length > 0
      ? props.fontFamily
      : "Arial"
  const fontSize =
    typeof props.fontSize === "number" && Number.isFinite(props.fontSize)
      ? props.fontSize
      : 16
  const fontStyle =
    typeof props.fontStyle === "string" && props.fontStyle.length > 0
      ? props.fontStyle
      : "normal"
  const height =
    typeof props.height === "number" && Number.isFinite(props.height)
      ? props.height
      : fontSize * TEXT_DEFAULT_LINE_HEIGHT
  const lineHeight =
    typeof props.lineHeight === "number" && Number.isFinite(props.lineHeight)
      ? props.lineHeight
      : TEXT_DEFAULT_LINE_HEIGHT
  const text = typeof props.text === "string" ? props.text : ""
  const verticalAlign =
    typeof props.verticalAlign === "string" ? props.verticalAlign : "top"
  const width =
    typeof props.width === "number" && Number.isFinite(props.width)
      ? props.width
      : 0
  const wrap = typeof props.wrap === "string" ? props.wrap : "word"

  return {
    align: (() => align) as unknown as Konva.Text["align"],
    fontFamily: (() => fontFamily) as unknown as Konva.Text["fontFamily"],
    fontSize: (() => fontSize) as unknown as Konva.Text["fontSize"],
    fontStyle: (() => fontStyle) as unknown as Konva.Text["fontStyle"],
    height: (() => height) as unknown as Konva.Text["height"],
    lineHeight: (() => lineHeight) as unknown as Konva.Text["lineHeight"],
    text: (() => text) as unknown as Konva.Text["text"],
    verticalAlign: (() =>
      verticalAlign) as unknown as Konva.Text["verticalAlign"],
    width: (() => width) as unknown as Konva.Text["width"],
    wrap: (() => wrap) as unknown as Konva.Text["wrap"],
  } as unknown as Konva.Text
}

export function getOverlayTransformForTextNode(
  textNode: Konva.Text | null,
  fallbackProps: Partial<TextShapeProps>
): TextOverlayTransform {
  if (!textNode) {
    return {
      rotation: (fallbackProps.rotation as number) ?? 0,
      scaleX: (fallbackProps.scaleX as number) ?? 1,
      scaleY: (fallbackProps.scaleY as number) ?? 1,
      x: (fallbackProps.x as number) ?? 0,
      y: (fallbackProps.y as number) ?? 0,
    }
  }

  const stage = textNode.getStage()
  if (!stage) {
    const nodeScale = textNode.getAbsoluteScale?.() ?? {
      x: (fallbackProps.scaleX as number) ?? 1,
      y: (fallbackProps.scaleY as number) ?? 1,
    }

    return {
      rotation: textNode.getAbsoluteRotation?.() ?? 0,
      scaleX: nodeScale.x,
      scaleY: nodeScale.y,
      x: (fallbackProps.x as number) ?? 0,
      y: (fallbackProps.y as number) ?? 0,
    }
  }

  const absoluteTransform = textNode.getAbsoluteTransform()
  const absoluteOrigin = absoluteTransform.point({ x: 0, y: 0 })
  const stageTransform = stage.getAbsoluteTransform().copy()
  stageTransform.invert()
  const stageLocalOrigin = stageTransform.point(absoluteOrigin)
  const absoluteScale = textNode.getAbsoluteScale?.() ?? {
    x: 1,
    y: 1,
  }
  const stageScale = stage.getAbsoluteScale?.() ?? {
    x: stage.scaleX(),
    y: stage.scaleY?.() ?? stage.scaleX(),
  }
  const stageRotation = stage.getAbsoluteRotation?.() ?? 0

  return {
    rotation: (textNode.getAbsoluteRotation?.() ?? 0) - stageRotation,
    scaleX:
      stageScale.x !== 0 ? absoluteScale.x / stageScale.x : absoluteScale.x,
    scaleY:
      stageScale.y !== 0 ? absoluteScale.y / stageScale.y : absoluteScale.y,
    x: stageLocalOrigin.x,
    y: stageLocalOrigin.y,
  }
}

export function getSelectionFromInput(
  input: HTMLTextAreaElement
): TextSelectionState {
  const selectionStart = input.selectionStart ?? 0
  const selectionEnd = input.selectionEnd ?? selectionStart

  return {
    cursorIndex: selectionEnd,
    selectionRange:
      selectionStart === selectionEnd
        ? null
        : {
            start: selectionStart,
            end: selectionEnd,
          },
  }
}

export function getInitialEditingWidthMode(
  props: Partial<TextShapeProps>
): TextBoxSizeMode {
  if (
    props.textBoxWidthMode === "auto" ||
    props.textBoxWidthMode === "manual"
  ) {
    return props.textBoxWidthMode
  }

  const fontSize =
    typeof props.fontSize === "number" && Number.isFinite(props.fontSize)
      ? props.fontSize
      : 16
  const minimumAutoWidth = Math.max(
    TEXT_MIN_BOX_WIDTH,
    Math.round(fontSize * 0.75)
  )
  const width =
    typeof props.width === "number" && Number.isFinite(props.width)
      ? props.width
      : 0

  return width <= minimumAutoWidth ? "auto" : "manual"
}
