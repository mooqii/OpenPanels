import type Konva from "konva"
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react"
import { TEXT_DEFAULT_LINE_HEIGHT, TEXT_MIN_BOX_WIDTH } from "../constants"
import type { Editor } from "../editor"
import {
  getTextBoxHeightMode,
  getTextBoxWidthMode,
  getTextPropsWithUpdatedLayout,
  getTextPropsWithUpdatedTransformLayout,
} from "../text-layout"
import { toTextShapeFontSize, toTextShapeFontStyle } from "../text-tool"
import { readPersistedCanvasTool } from "../tool-persistence"
import type { ShapeId } from "../types/ids"
import type {
  Shape,
  TextBoxSizeMode,
  TextShape,
  TextShapeProps,
} from "../types/shapes"
import {
  getCharIndexAtPosition,
  getCursorPosition,
  getSelectionRects,
} from "../utils/text-measure"

const CARET_WIDTH = 1.5

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

interface TextSelectionRange {
  end: number
  start: number
}

interface TextSelectionState {
  cursorIndex: number
  selectionRange: TextSelectionRange | null
}

interface TextOverlayTransform {
  rotation: number
  scaleX: number
  scaleY: number
  x: number
  y: number
}

function areSelectionRangesEqual(
  left: TextSelectionRange | null,
  right: TextSelectionRange | null
): boolean {
  if (left === right) return true
  if (!(left && right)) return false

  return left.start === right.start && left.end === right.end
}

function createTextMeasureNode(props: Partial<TextShapeProps>): Konva.Text {
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

function getOverlayTransformForTextNode(
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

function getSelectionFromInput(input: HTMLTextAreaElement): TextSelectionState {
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

function getInitialEditingWidthMode(
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

export function useTextEdit(editor: Editor) {
  const inputRef = useRef<HTMLTextAreaElement>(null)
  const focusFrameRef = useRef<number | null>(null)
  const textNodeRef = useRef<Konva.Text | null>(null)
  const draftTextPropsRef = useRef<TextShapeProps | null>(null)
  const pendingFocusRef = useRef(false)
  const cursorPositionRef = useRef(0)
  const selectionRangeRef = useRef<TextSelectionRange | null>(null)
  const selectionAnchorRef = useRef<number | null>(null)
  const blurTimerRef = useRef<number | NodeJS.Timeout | null>(null)
  const isInteractingWithTextRef = useRef(false)
  const isComposingRef = useRef(false)
  const lastPersistedPropsRef = useRef<TextShapeProps | null>(null)
  const widthModeRef = useRef<TextBoxSizeMode>("auto")
  const heightModeRef = useRef<TextBoxSizeMode>("auto")

  const [editingShapeId, setEditingShapeId] = useState<ShapeId | null>(null)
  const [draftTextPropsState, setDraftTextPropsState] =
    useState<TextShapeProps | null>(null)
  const [selectionState, setSelectionState] = useState<TextSelectionState>({
    cursorIndex: 0,
    selectionRange: null,
  })
  const [layoutVersion, bumpLayoutVersion] = useReducer((value) => value + 1, 0)
  const [isTextNodeReady, setIsTextNodeReady] = useState(false)
  const [_textNodeVersion, bumpTextNodeVersion] = useReducer(
    (value) => value + 1,
    0
  )

  const editingShape =
    editingShapeId === null
      ? null
      : ((editor.getShape(editingShapeId) as TextShape | undefined) ?? null)

  const draftTextProps = draftTextPropsState

  const setDraftTextProps = useCallback((nextProps: TextShapeProps | null) => {
    draftTextPropsRef.current = nextProps
    setDraftTextPropsState(nextProps)
    if (nextProps) {
      bumpLayoutVersion()
    }
  }, [])

  const setSelectionStateFromValues = useCallback(
    (cursorIndex: number, selectionRange: TextSelectionRange | null) => {
      cursorPositionRef.current = cursorIndex
      selectionRangeRef.current = selectionRange

      setSelectionState((current) => {
        if (
          current.cursorIndex === cursorIndex &&
          areSelectionRangesEqual(current.selectionRange, selectionRange)
        ) {
          return current
        }

        return {
          cursorIndex,
          selectionRange,
        }
      })
    },
    []
  )

  const cancelBlurTimer = useCallback(() => {
    if (blurTimerRef.current !== null) {
      clearTimeout(blurTimerRef.current)
      blurTimerRef.current = null
    }
  }, [])

  const cancelPendingFocusFrame = useCallback(() => {
    if (focusFrameRef.current !== null) {
      cancelAnimationFrame(focusFrameRef.current)
      focusFrameRef.current = null
    }
  }, [])

  const setAttachedTextNode = useCallback((node: Konva.Text | null) => {
    if (textNodeRef.current === node) {
      return
    }

    textNodeRef.current = node
    setIsTextNodeReady(node !== null)
    bumpTextNodeVersion()
  }, [])

  const getTextNode = useCallback((): Konva.Text | null => {
    return textNodeRef.current
  }, [])

  const bindEditingTextNode = useCallback(
    (node: Konva.Text | null) => {
      if (node && editingShapeId && node.id() !== editingShapeId) {
        return
      }

      setAttachedTextNode(node)
    },
    [editingShapeId, setAttachedTextNode]
  )

  const getBaseDraftProps = useCallback((): TextShapeProps | null => {
    if (draftTextPropsRef.current) {
      return draftTextPropsRef.current
    }

    if (editingShape) {
      return editingShape.props
    }

    return null
  }, [editingShape])

  const buildDraftTextProps = useCallback(
    (
      baseProps: Partial<TextShapeProps>,
      overrides: Partial<TextShapeProps> = {}
    ): TextShapeProps => {
      const nextWidthMode = getTextBoxWidthMode(
        {
          ...baseProps,
          ...overrides,
          textBoxWidthMode: overrides.textBoxWidthMode ?? widthModeRef.current,
        },
        widthModeRef.current
      )
      const nextHeightMode = getTextBoxHeightMode(
        {
          ...baseProps,
          ...overrides,
          textBoxHeightMode:
            overrides.textBoxHeightMode ?? heightModeRef.current,
        },
        heightModeRef.current
      )

      widthModeRef.current = nextWidthMode
      heightModeRef.current = nextHeightMode

      return getTextPropsWithUpdatedLayout(
        baseProps,
        {
          ...overrides,
          textBoxHeightMode: nextHeightMode,
          textBoxWidthMode: nextWidthMode,
          verticalAlign: "top",
          wrap: "word",
        },
        {
          fallbackHeightMode: nextHeightMode,
          fallbackWidthMode: nextWidthMode,
          measureLineWidth: (line, fontFamily, fontSize, fontStyle) => {
            const textNode = textNodeRef.current
            if (
              !textNode ||
              textNode.fontFamily() !== fontFamily ||
              textNode.fontSize() !== fontSize ||
              textNode.fontStyle() !== fontStyle
            ) {
              return undefined
            }

            try {
              return textNode.measureSize(line).width
            } catch {
              return undefined
            }
          },
        }
      ) as TextShapeProps
    },
    []
  )

  const syncSelectionFromInput = useCallback(() => {
    const input = inputRef.current
    if (!input) return

    const nextSelection = getSelectionFromInput(input)
    setSelectionStateFromValues(
      nextSelection.cursorIndex,
      nextSelection.selectionRange
    )
  }, [setSelectionStateFromValues])

  const updateInputPosition = useCallback(() => {
    const input = inputRef.current
    const textNode = getTextNode()
    const currentDraftProps = draftTextPropsRef.current

    if (!input) return
    if (!textNode) {
      input.style.visibility = "hidden"
      return
    }

    const stage = textNode.getStage()
    if (!stage) return

    const transform = textNode.getAbsoluteTransform()
    const topLeft = transform.point({ x: 0, y: 0 })
    const stageBox = stage.container().getBoundingClientRect()
    const absoluteScale = textNode.getAbsoluteScale?.() ?? {
      x: stage.scaleX(),
      y: stage.scaleX(),
    }
    const rotation = textNode.getAbsoluteRotation()
    const fontSize = textNode.fontSize() * absoluteScale.y
    const fontStyle = textNode.fontStyle()
    const lineHeight = textNode.lineHeight() * fontSize
    const resolvedWidth =
      typeof currentDraftProps?.width === "number"
        ? currentDraftProps.width
        : textNode.width()
    const resolvedHeight =
      typeof currentDraftProps?.height === "number"
        ? currentDraftProps.height
        : textNode.height()
    const fill =
      typeof currentDraftProps?.fill === "string"
        ? currentDraftProps.fill
        : "CanvasText"
    const opacity =
      typeof currentDraftProps?.opacity === "number"
        ? currentDraftProps.opacity
        : 1

    input.style.left = `${stageBox.left + topLeft.x}px`
    input.style.top = `${stageBox.top + topLeft.y}px`
    input.style.width = `${Math.max(1, resolvedWidth * absoluteScale.x)}px`
    input.style.height = `${Math.max(
      lineHeight,
      resolvedHeight * absoluteScale.y
    )}px`
    input.style.color = fill
    input.style.fontFamily = textNode.fontFamily()
    input.style.fontSize = `${fontSize}px`
    input.style.fontStyle = fontStyle.includes("italic") ? "italic" : "normal"
    input.style.fontWeight =
      fontStyle === "bold" || fontStyle === "700" ? "700" : "400"
    input.style.lineHeight = `${lineHeight}px`
    input.style.opacity = String(opacity)
    input.style.textAlign = textNode.align()
    input.style.caretColor = "auto"
    input.style.transform = `rotate(${rotation}deg)`
    input.style.transformOrigin = "left top"
    input.style.visibility = "visible"
  }, [getTextNode])

  const syncDraftFromText = useCallback(
    (text: string) => {
      const baseProps = getBaseDraftProps()
      if (!baseProps) return

      const nextProps = buildDraftTextProps(baseProps, {
        text,
      })

      setDraftTextProps(nextProps)
      syncSelectionFromInput()
    },
    [
      buildDraftTextProps,
      getBaseDraftProps,
      setDraftTextProps,
      syncSelectionFromInput,
    ]
  )

  const setInputSelection = useCallback(
    (start: number, end: number) => {
      const input = inputRef.current
      if (!input) return

      input.focus()
      input.setSelectionRange(start, end)

      const nextSelection = getSelectionFromInput(input)
      setSelectionStateFromValues(
        nextSelection.cursorIndex,
        nextSelection.selectionRange
      )
    },
    [setSelectionStateFromValues]
  )

  const setupTextNodeEvents = useCallback(() => {
    const textNode = getTextNode()
    if (!textNode) return

    const getMeasureNode = () =>
      createTextMeasureNode(
        getBaseDraftProps() ?? textNodeRef.current?.attrs ?? {}
      )

    const handleClick = () => {
      cancelBlurTimer()

      if (selectionRangeRef.current) {
        inputRef.current?.focus()
        return
      }

      const localPos = getLocalPointerPosition(textNode)
      if (!localPos) return

      const charIndex = getCharIndexAtPosition(
        getMeasureNode(),
        localPos.x,
        localPos.y
      )

      selectionAnchorRef.current = null
      setInputSelection(charIndex, charIndex)
    }

    const handleMouseDown = () => {
      isInteractingWithTextRef.current = true
      cancelBlurTimer()

      const localPos = getLocalPointerPosition(textNode)
      if (!localPos) return

      const charIndex = getCharIndexAtPosition(
        getMeasureNode(),
        localPos.x,
        localPos.y
      )

      selectionAnchorRef.current = charIndex
      setInputSelection(charIndex, charIndex)
    }

    const handleMouseMove = () => {
      if (selectionAnchorRef.current === null) return

      const localPos = getLocalPointerPosition(textNode)
      if (!localPos) return

      const charIndex = getCharIndexAtPosition(
        getMeasureNode(),
        localPos.x,
        localPos.y
      )
      const anchor = selectionAnchorRef.current

      setInputSelection(anchor, charIndex)
    }

    const handleMouseUp = () => {
      selectionAnchorRef.current = null
      setTimeout(() => {
        isInteractingWithTextRef.current = false
      }, 0)
    }

    textNode.on("click", handleClick)
    textNode.on("pointerdown", handleMouseDown)
    textNode.on("pointermove", handleMouseMove)
    textNode.on("pointerup", handleMouseUp)

    return () => {
      textNode.off("click", handleClick)
      textNode.off("pointerdown", handleMouseDown)
      textNode.off("pointermove", handleMouseMove)
      textNode.off("pointerup", handleMouseUp)
    }
  }, [cancelBlurTimer, getBaseDraftProps, getTextNode, setInputSelection])

  useEffect(() => {
    if (!editingShapeId) {
      return
    }

    return setupTextNodeEvents()
  }, [editingShapeId, setupTextNodeEvents])

  useLayoutEffect(() => {
    if (!(editingShapeId && isTextNodeReady)) {
      return
    }

    updateInputPosition()

    const input = inputRef.current
    if (!input) {
      return
    }

    if (!pendingFocusRef.current) {
      return
    }

    const selectionRange = selectionRangeRef.current
    const cursorIndex = cursorPositionRef.current
    const selectionStart = selectionRange?.start ?? cursorIndex
    const selectionEnd = selectionRange?.end ?? cursorIndex

    cancelPendingFocusFrame()
    focusFrameRef.current = requestAnimationFrame(() => {
      const latestInput = inputRef.current
      if (!(latestInput && pendingFocusRef.current)) {
        return
      }

      latestInput.focus()
      latestInput.setSelectionRange(selectionStart, selectionEnd)
      pendingFocusRef.current = false
      syncSelectionFromInput()
      updateInputPosition()
      focusFrameRef.current = null
    })

    return () => {
      cancelPendingFocusFrame()
    }
  }, [
    cancelPendingFocusFrame,
    editingShapeId,
    isTextNodeReady,
    syncSelectionFromInput,
    updateInputPosition,
  ])

  useLayoutEffect(() => {
    if (!(editingShapeId && isTextNodeReady)) {
      return
    }

    if (!Number.isFinite(layoutVersion)) {
      return
    }

    updateInputPosition()
  }, [editingShapeId, isTextNodeReady, layoutVersion, updateInputPosition])

  useEffect(() => {
    if (!editingShape) {
      lastPersistedPropsRef.current = null
      return
    }

    if (!draftTextPropsRef.current) {
      lastPersistedPropsRef.current = editingShape.props
      return
    }

    if (lastPersistedPropsRef.current === null) {
      lastPersistedPropsRef.current = editingShape.props
      return
    }

    if (lastPersistedPropsRef.current === editingShape.props) {
      return
    }

    lastPersistedPropsRef.current = editingShape.props

    const currentDraft = draftTextPropsRef.current
    const nextProps = buildDraftTextProps(
      {
        ...currentDraft,
        align: editingShape.props.align,
        fill: editingShape.props.fill,
        fontFamily: editingShape.props.fontFamily,
        fontSize: editingShape.props.fontSize,
        fontStyle: editingShape.props.fontStyle,
        lineHeight: editingShape.props.lineHeight,
        opacity: editingShape.props.opacity,
        stroke: editingShape.props.stroke,
        strokeWidth: editingShape.props.strokeWidth,
      },
      {
        text: (currentDraft.text as string | undefined) ?? "",
      }
    )

    setDraftTextProps(nextProps)
  }, [buildDraftTextProps, editingShape, setDraftTextProps])

  const startTextEditing = useCallback(
    (shapeId: ShapeId) => {
      const shape = editor.getShape(shapeId) as TextShape | undefined
      if (!shape || shape.type !== "text") return

      cancelBlurTimer()
      pendingFocusRef.current = true
      setAttachedTextNode(null)
      widthModeRef.current = getTextBoxWidthMode(
        shape.props,
        getInitialEditingWidthMode(shape.props)
      )
      heightModeRef.current = "auto"
      lastPersistedPropsRef.current = shape.props

      const initialText = (shape.props.text as string | undefined) ?? ""
      const nextProps = buildDraftTextProps(shape.props, {
        text: initialText,
      })

      setEditingShapeId(shapeId)
      setDraftTextProps(nextProps)
      setSelectionStateFromValues(initialText.length, null)

      const input = inputRef.current
      if (input) {
        input.value = initialText
      }
    },
    [
      buildDraftTextProps,
      cancelBlurTimer,
      editor,
      setDraftTextProps,
      setAttachedTextNode,
      setSelectionStateFromValues,
    ]
  )

  const stopTextEditing = useCallback(() => {
    cancelBlurTimer()
    cancelPendingFocusFrame()

    const input = inputRef.current
    const finalDraftProps = draftTextPropsRef.current
    const currentShape =
      editingShapeId === null
        ? null
        : ((editor.getShape(editingShapeId) as TextShape | undefined) ?? null)
    const finalText = input?.value ?? (finalDraftProps?.text as string) ?? ""

    if (editingShapeId && currentShape) {
      if (finalText.length === 0) {
        editor.deleteShape(editingShapeId)
      } else {
        const nextProps = buildDraftTextProps(
          {
            ...currentShape.props,
            ...finalDraftProps,
          },
          {
            text: finalText,
          }
        )

        editor.updateShape(
          editingShapeId,
          (old) =>
            ({
              ...old,
              props: {
                ...old.props,
                ...nextProps,
              },
            }) as Shape
        )
      }
    }

    if (input) {
      input.value = ""
      input.style.visibility = "hidden"
    }

    setEditingShapeId(null)
    setDraftTextProps(null)
    setSelectionStateFromValues(0, null)
    pendingFocusRef.current = false
    setAttachedTextNode(null)
    selectionAnchorRef.current = null
    isComposingRef.current = false
    lastPersistedPropsRef.current = null
  }, [
    buildDraftTextProps,
    cancelBlurTimer,
    cancelPendingFocusFrame,
    editingShapeId,
    editor,
    setDraftTextProps,
    setAttachedTextNode,
    setSelectionStateFromValues,
  ])

  const handleStageMouseDown = useCallback(
    (e: Konva.KonvaEventObject<PointerEvent>) => {
      const stage = e.target.getStage()
      const pos = stage?.getRelativePointerPosition()
      if (!pos) return

      const scale = editor.stage?.scaleX() ?? 1
      const currentTool = editor.getTool()
      const textTool =
        currentTool.name === "text"
          ? currentTool
          : readPersistedCanvasTool("text")
      const baseFontSize = textTool.fontSize
      const initialProps = getTextPropsWithUpdatedLayout(
        {
          align: textTool.align,
          fill: textTool.color,
          fontFamily: textTool.fontFamily,
          fontSize: toTextShapeFontSize(baseFontSize, scale),
          fontStyle: toTextShapeFontStyle(textTool.fontWeight),
          height: 0,
          lineHeight: TEXT_DEFAULT_LINE_HEIGHT,
          text: "",
          textBoxHeightMode: "auto",
          textBoxWidthMode: "auto",
          verticalAlign: "top",
          width: 0,
          wrap: "word",
          x: pos.x,
          y: pos.y,
        },
        {},
        {
          fallbackHeightMode: "auto",
          fallbackWidthMode: "auto",
        }
      ) as TextShapeProps

      const newShape = editor.createShape({
        type: "text",
        props: initialProps,
      })

      editor.setSelectedShapes([newShape.id as ShapeId])

      startTextEditing(newShape.id as ShapeId)

      editor.setTool({ name: "select" })
    },
    [editor, startTextEditing]
  )

  const syncFromEventTarget = useCallback(
    (target: HTMLTextAreaElement) => {
      const nextSelection = getSelectionFromInput(target)
      setSelectionStateFromValues(
        nextSelection.cursorIndex,
        nextSelection.selectionRange
      )
      syncDraftFromText(target.value)
    },
    [setSelectionStateFromValues, syncDraftFromText]
  )

  const handleInput = useCallback(
    (e: React.FormEvent<HTMLTextAreaElement>) => {
      syncFromEventTarget(e.currentTarget)
    },
    [syncFromEventTarget]
  )

  const handleCompositionStart = useCallback(
    (e: React.CompositionEvent<HTMLTextAreaElement>) => {
      isComposingRef.current = true
      syncFromEventTarget(e.currentTarget)
    },
    [syncFromEventTarget]
  )

  const handleCompositionUpdate = useCallback(
    (e: React.CompositionEvent<HTMLTextAreaElement>) => {
      syncFromEventTarget(e.currentTarget)
    },
    [syncFromEventTarget]
  )

  const handleCompositionEnd = useCallback(
    (e: React.CompositionEvent<HTMLTextAreaElement>) => {
      isComposingRef.current = false
      syncFromEventTarget(e.currentTarget)
    },
    [syncFromEventTarget]
  )

  const handleSelect = useCallback(() => {
    syncSelectionFromInput()
  }, [syncSelectionFromInput])

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Escape") {
        e.preventDefault()
        stopTextEditing()
      }
    },
    [stopTextEditing]
  )

  const handleKeyUp = useCallback(() => {
    syncSelectionFromInput()
  }, [syncSelectionFromInput])

  const handleBlur = useCallback(() => {
    if (isInteractingWithTextRef.current) {
      return
    }

    cancelBlurTimer()

    blurTimerRef.current = setTimeout(() => {
      blurTimerRef.current = null
      stopTextEditing()
    }, 200)
  }, [cancelBlurTimer, stopTextEditing])

  const setEditingBoxModes = useCallback(
    (modes: { heightMode?: TextBoxSizeMode; widthMode?: TextBoxSizeMode }) => {
      if (modes.widthMode) {
        widthModeRef.current = modes.widthMode
      }
      if (modes.heightMode) {
        heightModeRef.current = modes.heightMode
      }
    },
    []
  )

  const syncDraftFromNode = useCallback(
    (node: Konva.Node | null) => {
      const currentDraft = draftTextPropsRef.current
      if (!(node && currentDraft)) return

      const nextWidthMode = getTextBoxWidthMode(
        currentDraft,
        widthModeRef.current
      )
      const nextProps = getTextPropsWithUpdatedTransformLayout(
        currentDraft,
        {
          height: node.height(),
          rotation: node.rotation(),
          scaleX: node.scaleX(),
          scaleY: node.scaleY(),
          width: node.width(),
          x: node.x(),
          y: node.y(),
        },
        nextWidthMode
      ) as TextShapeProps

      if (typeof nextProps.width === "number") {
        node.width(nextProps.width)
      }
      if (typeof nextProps.height === "number") {
        node.height(nextProps.height)
      }

      node.getLayer()?.batchDraw()

      setDraftTextProps(nextProps)
    },
    [setDraftTextProps]
  )

  const overlay = useMemo(() => {
    if (!draftTextProps) {
      return null
    }

    const textNode = getTextNode()
    const measureNode = createTextMeasureNode(draftTextProps)
    const selectionRects = selectionState.selectionRange
      ? getSelectionRects(
          measureNode,
          selectionState.selectionRange.start,
          selectionState.selectionRange.end
        )
      : []
    const caretRect = selectionState.selectionRange
      ? null
      : (() => {
          const cursorPosition = getCursorPosition(
            measureNode,
            selectionState.cursorIndex
          )
          const caretHeight = measureNode.lineHeight() * measureNode.fontSize()

          return {
            height: caretHeight,
            width: CARET_WIDTH,
            x: Math.max(0, cursorPosition.x - CARET_WIDTH / 2),
            y: cursorPosition.y,
          }
        })()

    const overlayTransform = getOverlayTransformForTextNode(
      textNode,
      draftTextProps
    )

    return {
      caretRect,
      height:
        typeof draftTextProps.height === "number" ? draftTextProps.height : 0,
      rotation: overlayTransform.rotation,
      scaleX: overlayTransform.scaleX,
      scaleY: overlayTransform.scaleY,
      selectionRects,
      width:
        typeof draftTextProps.width === "number" ? draftTextProps.width : 0,
      x: overlayTransform.x,
      y: overlayTransform.y,
    } satisfies TextEditOverlay
  }, [draftTextProps, getTextNode, selectionState])

  return {
    bindEditingTextNode,
    cursorPosition: cursorPositionRef,
    draftTextProps,
    editingShape,
    editingShapeId,
    handleBlur,
    handleCompositionEnd,
    handleCompositionStart,
    handleCompositionUpdate,
    handleInput,
    handleKeyDown,
    handleKeyUp,
    handleSelect,
    handleStageMouseDown,
    inputRef,
    isEditing: editingShapeId !== null,
    isComposing: isComposingRef.current,
    isTextNodeReady,
    layoutVersion,
    overlay,
    selectionRange: selectionRangeRef,
    setEditingBoxModes,
    startTextEditing,
    stopTextEditing,
    syncDraftFromNode,
    syncTextNodeFromInput: syncDraftFromText,
    updateInputPosition,
  }
}

const getLocalPointerPosition = (
  textNode: Konva.Text
): { x: number; y: number } | null => {
  const stage = textNode.getStage()
  if (!stage) return null

  const pointerPos = stage.getPointerPosition()
  if (!pointerPos) return null

  const transform = textNode.getAbsoluteTransform().copy()
  transform.invert()
  return transform.point(pointerPos)
}
