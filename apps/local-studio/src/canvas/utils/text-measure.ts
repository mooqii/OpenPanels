import type Konva from "konva"

/**
 * Text wrap modes supported by Konva
 */
export type WrapMode = "word" | "char" | "none"

/**
 * Horizontal text alignment
 */
export type HorizontalAlign = "left" | "center" | "right" | "justify"

/**
 * Vertical text alignment
 */
export type VerticalAlign = "top" | "middle" | "bottom"

/**
 * Regex for trimming trailing whitespace (hoisted for performance)
 */
const TRAILING_WHITESPACE_REGEX = /\s+$/

/**
 * Trim trailing whitespace from a string (for visual line width calculation)
 * Konva doesn't include trailing spaces when calculating line width for alignment
 */
function trimTrailingWhitespace(str: string): string {
  return str.replace(TRAILING_WHITESPACE_REGEX, "")
}

/**
 * Calculate horizontal offset for a line based on alignment
 */
function getHorizontalOffset(
  lineWidth: number,
  containerWidth: number,
  align: HorizontalAlign
): number {
  switch (align) {
    case "center":
      return (containerWidth - lineWidth) / 2
    case "right":
      return containerWidth - lineWidth
    default:
      return 0
  }
}

/**
 * Calculate vertical offset for text based on alignment
 */
function getVerticalOffset(
  textHeight: number,
  containerHeight: number,
  verticalAlign: VerticalAlign
): number {
  switch (verticalAlign) {
    case "middle":
      return (containerHeight - textHeight) / 2
    case "bottom":
      return containerHeight - textHeight
    default:
      return 0
  }
}

/**
 * Measure text dimensions using an offscreen canvas
 */
export function measureText(
  text: string,
  fontFamily: string,
  fontSize: number,
  fontStyle = "normal"
): { width: number; height: number } {
  const canvas = document.createElement("canvas")
  const context = canvas.getContext("2d")
  if (!context) {
    return { width: 0, height: 0 }
  }

  context.font = `${fontStyle} ${fontSize}px ${fontFamily}`
  const metrics = context.measureText(text)
  return {
    width: metrics.width,
    height: fontSize, // Approximate height
  }
}

/**
 * Find the end index of a line based on wrap mode
 * Returns the index of the last character that fits on the line
 * Handles explicit newline characters as hard line breaks
 */
function findLineEnd(
  text: string,
  lineStartIndex: number,
  maxWidth: number,
  fontFamily: string,
  fontSize: number,
  fontStyle: string,
  wrapMode: WrapMode
): number {
  // First, check for explicit newline in the remaining text
  const newlineIndex = text.indexOf("\n", lineStartIndex)

  if (wrapMode === "none") {
    // No wrapping - return to newline or end of text
    if (newlineIndex !== -1) {
      return newlineIndex
    }
    return text.length
  }

  let lineEndIndex = lineStartIndex

  // Find where the line would overflow, but stop at newline
  while (lineEndIndex < text.length) {
    // Stop at newline character (it's a hard break)
    if (text[lineEndIndex] === "\n") {
      break
    }

    const lineText = text.substring(lineStartIndex, lineEndIndex + 1)
    const lineWidth = measureText(
      lineText,
      fontFamily,
      fontSize,
      fontStyle
    ).width
    if (lineWidth > maxWidth) {
      break
    }
    lineEndIndex++
  }

  // If we stopped at a newline, return that position
  if (lineEndIndex < text.length && text[lineEndIndex] === "\n") {
    return lineEndIndex
  }

  // If we reached end of text or no overflow, return current position
  if (lineEndIndex >= text.length || lineEndIndex === lineStartIndex) {
    // Ensure at least one character per line to avoid infinite loop
    return Math.max(lineEndIndex, lineStartIndex + 1)
  }

  // For "char" mode, break at the character that overflows
  if (wrapMode === "char") {
    return lineEndIndex
  }

  // For "word" mode, find the last word boundary before overflow
  // Look backwards from the overflow point to find a space
  let wordBreakIndex = lineEndIndex
  while (wordBreakIndex > lineStartIndex) {
    const char = text[wordBreakIndex - 1]
    if (char === " " || char === "\t") {
      // Found a word boundary - break here (after the space)
      return wordBreakIndex
    }
    wordBreakIndex--
  }

  // No word boundary found - fall back to char wrap for this long word
  return lineEndIndex
}

/**
 * Get line information for wrapped text
 * Returns array of { start, end } indices for each line
 * Handles both explicit newlines (\n) and width-based wrapping
 */
export function getLineBreaks(
  text: string,
  maxWidth: number,
  fontFamily: string,
  fontSize: number,
  fontStyle: string,
  wrapMode: WrapMode
): Array<{ start: number; end: number }> {
  if (text.length === 0) {
    return [{ start: 0, end: 0 }]
  }

  const lines: Array<{ start: number; end: number }> = []
  let lineStartIndex = 0

  while (lineStartIndex < text.length) {
    const lineEndIndex = findLineEnd(
      text,
      lineStartIndex,
      maxWidth,
      fontFamily,
      fontSize,
      fontStyle,
      wrapMode
    )

    lines.push({ start: lineStartIndex, end: lineEndIndex })

    // If we stopped at a newline, skip it for the next line start
    if (lineEndIndex < text.length && text[lineEndIndex] === "\n") {
      lineStartIndex = lineEndIndex + 1
      // Handle trailing newline - add an empty line
      if (lineStartIndex >= text.length) {
        lines.push({ start: lineStartIndex, end: lineStartIndex })
      }
    } else {
      lineStartIndex = lineEndIndex
    }
  }

  return lines
}

/**
 * Get the X position of a cursor at a given character index
 * Uses Konva Text node's measureSize method if available, otherwise falls back to canvas measurement
 */
export function getCursorXPosition(
  textNode: Konva.Text,
  cursorIndex: number
): number {
  const text = textNode.text()
  if (cursorIndex <= 0) {
    return 0
  }
  if (cursorIndex >= text.length) {
    // Cursor at end - use getTextWidth
    return textNode.getTextWidth()
  }

  // Measure text up to cursor position
  const textBeforeCursor = text.substring(0, cursorIndex)
  const fontFamily = textNode.fontFamily()
  const fontSize = textNode.fontSize()
  const fontStyle = textNode.fontStyle() || "normal"

  // Try using Konva's measureSize if available
  try {
    const size = textNode.measureSize(textBeforeCursor)
    return size.width
  } catch {
    // Fallback to canvas measurement
    return measureText(textBeforeCursor, fontFamily, fontSize, fontStyle).width
  }
}

/**
 * Get cursor position (x, y) for a given character index
 * Handles multi-line text by calculating actual line breaks based on wrap mode
 * Accounts for horizontal and vertical text alignment
 */
export function getCursorPosition(
  textNode: Konva.Text,
  cursorIndex: number
): { x: number; y: number } {
  const text = textNode.text()
  const containerWidth = textNode.width()
  const containerHeight = textNode.height()
  const fontSize = textNode.fontSize()
  const lineHeight = textNode.lineHeight() * fontSize
  const fontFamily = textNode.fontFamily()
  const fontStyle = textNode.fontStyle() || "normal"
  const wrapMode = (textNode.wrap() || "word") as WrapMode
  const align = (textNode.align() || "left") as HorizontalAlign
  const verticalAlign = (textNode.verticalAlign() || "top") as VerticalAlign

  // Get all line breaks based on wrap mode
  const lines = getLineBreaks(
    text,
    containerWidth,
    fontFamily,
    fontSize,
    fontStyle,
    wrapMode
  )

  // Calculate total text height for vertical alignment
  const totalTextHeight = lines.length * lineHeight
  const verticalOffset = getVerticalOffset(
    totalTextHeight,
    containerHeight,
    verticalAlign
  )

  if (cursorIndex <= 0) {
    // For cursor at position 0, calculate x based on first line alignment
    const firstLine = lines[0]
    const firstLineText = text.substring(firstLine.start, firstLine.end)
    // Trim trailing whitespace for alignment calculation (Konva doesn't include it)
    const trimmedFirstLineText = trimTrailingWhitespace(firstLineText)
    const firstLineWidth = measureText(
      trimmedFirstLineText,
      fontFamily,
      fontSize,
      fontStyle
    ).width
    const horizontalOffset = getHorizontalOffset(
      firstLineWidth,
      containerWidth,
      align
    )
    return { x: horizontalOffset, y: verticalOffset }
  }

  // Find which line the cursor is on
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]
    // Cursor is on this line if it's within the line range
    // or if it's at the end of the last line
    if (cursorIndex <= line.end || i === lines.length - 1) {
      // Calculate full line width for alignment (trim trailing whitespace)
      const fullLineText = text.substring(line.start, line.end)
      const trimmedLineText = trimTrailingWhitespace(fullLineText)
      const fullLineWidth = measureText(
        trimmedLineText,
        fontFamily,
        fontSize,
        fontStyle
      ).width
      const horizontalOffset = getHorizontalOffset(
        fullLineWidth,
        containerWidth,
        align
      )

      // Calculate cursor x position within the line
      const lineText = text.substring(
        line.start,
        Math.min(cursorIndex, line.end)
      )
      const cursorXInLine = measureText(
        lineText,
        fontFamily,
        fontSize,
        fontStyle
      ).width

      return {
        x: horizontalOffset + cursorXInLine,
        y: verticalOffset + i * lineHeight,
      }
    }
  }

  // Fallback: cursor at end of last line
  const lastLine = lines.at(lines.length - 1)!
  const lastLineText = text.substring(lastLine.start, lastLine.end)
  const trimmedLastLineText = trimTrailingWhitespace(lastLineText)
  const lastLineWidth = measureText(
    trimmedLastLineText,
    fontFamily,
    fontSize,
    fontStyle
  ).width
  const horizontalOffset = getHorizontalOffset(
    lastLineWidth,
    containerWidth,
    align
  )

  return {
    x:
      horizontalOffset +
      measureText(lastLineText, fontFamily, fontSize, fontStyle).width,
    y: verticalOffset + (lines.length - 1) * lineHeight,
  }
}

/**
 * Get bounding rectangles for a text selection range
 * Returns array of rects (one per line if multi-line)
 * Respects text wrap mode and alignment for accurate positioning
 */
export function getSelectionRects(
  textNode: Konva.Text,
  startIndex: number,
  endIndex: number
): Array<{ x: number; y: number; width: number; height: number }> {
  const text = textNode.text()
  const fontSize = textNode.fontSize()
  const lineHeight = textNode.lineHeight() * fontSize
  const containerWidth = textNode.width()
  const containerHeight = textNode.height()
  const fontFamily = textNode.fontFamily()
  const fontStyle = textNode.fontStyle() || "normal"
  const wrapMode = (textNode.wrap() || "word") as WrapMode
  const align = (textNode.align() || "left") as HorizontalAlign
  const verticalAlign = (textNode.verticalAlign() || "top") as VerticalAlign

  // Normalize indices
  const start = Math.min(startIndex, endIndex)
  const end = Math.max(startIndex, endIndex)

  if (start === end) {
    return []
  }

  // Get all line breaks based on wrap mode
  const lines = getLineBreaks(
    text,
    containerWidth,
    fontFamily,
    fontSize,
    fontStyle,
    wrapMode
  )

  // Calculate vertical offset for alignment
  const totalTextHeight = lines.length * lineHeight
  const verticalOffset = getVerticalOffset(
    totalTextHeight,
    containerHeight,
    verticalAlign
  )

  const rects: Array<{ x: number; y: number; width: number; height: number }> =
    []

  // Find which lines the selection spans
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]

    // Check if this line overlaps with the selection
    if (line.end <= start) {
      // Line is before selection
      continue
    }
    if (line.start >= end) {
      // Line is after selection - we're done
      break
    }

    // Calculate full line width for horizontal alignment (trim trailing whitespace)
    const fullLineText = text.substring(line.start, line.end)
    const trimmedLineText = trimTrailingWhitespace(fullLineText)
    const fullLineWidth = measureText(
      trimmedLineText,
      fontFamily,
      fontSize,
      fontStyle
    ).width
    const horizontalOffset = getHorizontalOffset(
      fullLineWidth,
      containerWidth,
      align
    )

    // This line overlaps with the selection
    const selStart = Math.max(line.start, start)
    const selEnd = Math.min(line.end, end)

    // Calculate x position within the line
    const textBeforeSelStart = text.substring(line.start, selStart)
    const selectedLineText = text.substring(selStart, selEnd)

    const xInLine = measureText(
      textBeforeSelStart,
      fontFamily,
      fontSize,
      fontStyle
    ).width
    const rectWidth = measureText(
      selectedLineText,
      fontFamily,
      fontSize,
      fontStyle
    ).width

    rects.push({
      x: horizontalOffset + xInLine,
      y: verticalOffset + i * lineHeight,
      width: rectWidth,
      height: lineHeight,
    })
  }

  return rects
}

/**
 * Get character index at a given position (x, y) relative to text node
 * Uses binary search for efficiency, handles multi-line wrapped text
 * Respects text wrap mode and alignment for accurate positioning
 */
export function getCharIndexAtPosition(
  textNode: Konva.Text,
  x: number,
  y: number
): number {
  const text = textNode.text()
  if (text.length === 0) {
    return 0
  }

  const fontSize = textNode.fontSize()
  const lineHeight = textNode.lineHeight() * fontSize
  const containerWidth = textNode.width()
  const containerHeight = textNode.height()
  const fontFamily = textNode.fontFamily()
  const fontStyle = textNode.fontStyle() || "normal"
  const wrapMode = (textNode.wrap() || "word") as WrapMode
  const align = (textNode.align() || "left") as HorizontalAlign
  const verticalAlign = (textNode.verticalAlign() || "top") as VerticalAlign

  // Get all line breaks based on wrap mode
  const lines = getLineBreaks(
    text,
    containerWidth,
    fontFamily,
    fontSize,
    fontStyle,
    wrapMode
  )

  // Calculate vertical offset for alignment
  const totalTextHeight = lines.length * lineHeight
  const verticalOffset = getVerticalOffset(
    totalTextHeight,
    containerHeight,
    verticalAlign
  )

  // Adjust y coordinate to account for vertical alignment
  const adjustedY = y - verticalOffset

  // Determine which visual line was clicked
  const lineIndex = Math.max(
    0,
    Math.min(Math.floor(adjustedY / lineHeight), lines.length - 1)
  )

  // Get the line info
  const line = lines[lineIndex]
  const lineStartIndex = line.start
  const lineEndIndex = line.end

  // If click is beyond the last line, return end of text
  if (lineIndex >= lines.length) {
    return text.length
  }

  // Calculate horizontal offset for this line based on alignment (trim trailing whitespace)
  const fullLineText = text.substring(line.start, line.end)
  const trimmedLineText = trimTrailingWhitespace(fullLineText)
  const fullLineWidth = measureText(
    trimmedLineText,
    fontFamily,
    fontSize,
    fontStyle
  ).width
  const horizontalOffset = getHorizontalOffset(
    fullLineWidth,
    containerWidth,
    align
  )

  // Adjust x coordinate to account for horizontal alignment
  const adjustedX = x - horizontalOffset

  // Clamp adjusted x to valid range for this line
  const clampedX = Math.max(0, adjustedX)

  // Binary search within this line's character range
  let left = lineStartIndex
  let right = lineEndIndex

  while (left < right) {
    const mid = Math.floor((left + right) / 2)
    // Measure from line start to mid
    const textUpToMid = text.substring(lineStartIndex, mid)
    const midWidth = measureText(
      textUpToMid,
      fontFamily,
      fontSize,
      fontStyle
    ).width

    if (midWidth < clampedX) {
      left = mid + 1
    } else {
      right = mid
    }
  }

  // Check if we're closer to left or right character
  if (left > lineStartIndex && left <= lineEndIndex) {
    const prevWidth = measureText(
      text.substring(lineStartIndex, left - 1),
      fontFamily,
      fontSize,
      fontStyle
    ).width
    const currWidth = measureText(
      text.substring(lineStartIndex, left),
      fontFamily,
      fontSize,
      fontStyle
    ).width

    // If click is closer to previous character, use that index
    if (clampedX - prevWidth < currWidth - clampedX) {
      return left - 1
    }
  }

  return Math.min(left, text.length)
}
