/**
 * Fill utility functions for converting ShapeFill types to Konva properties.
 */

import type { AssetId } from "../types/ids"
import type {
  GradientColorStop,
  ImageFill,
  LinearGradientFill,
  RadialGradientFill,
  ShapeFill,
  SolidFill,
} from "../types/shapes"

// =============================================================================
// Konva Fill Props Types
// =============================================================================

export interface KonvaSolidFillProps {
  fill: string
  fillPriority: "color"
}

export interface KonvaLinearGradientProps {
  fillLinearGradientColorStops: (string | number)[]
  fillLinearGradientEndPoint: { x: number; y: number }
  fillLinearGradientStartPoint: { x: number; y: number }
  fillPriority: "linear-gradient"
}

export interface KonvaRadialGradientProps {
  fillPriority: "radial-gradient"
  fillRadialGradientColorStops: (string | number)[]
  fillRadialGradientEndPoint: { x: number; y: number }
  fillRadialGradientEndRadius: number
  fillRadialGradientStartPoint: { x: number; y: number }
  fillRadialGradientStartRadius: number
}

export interface KonvaPatternFillProps {
  fillPatternImage: HTMLImageElement
  fillPatternOffset?: { x: number; y: number }
  fillPatternScale?: { x: number; y: number }
  fillPriority: "pattern"
}

export type KonvaFillProps =
  | KonvaSolidFillProps
  | KonvaLinearGradientProps
  | KonvaRadialGradientProps
  | KonvaPatternFillProps

// =============================================================================
// Conversion Functions
// =============================================================================

/**
 * Flatten gradient color stops to Konva's format: [offset, color, offset, color, ...]
 */
export function flattenColorStops(
  stops: GradientColorStop[]
): (string | number)[] {
  const result: (string | number)[] = []
  for (const stop of stops) {
    result.push(stop.offset, stop.color)
  }
  return result
}

/**
 * Calculate linear gradient start and end points based on rotation angle.
 * Rotation is in degrees, where 0 is left-to-right, 90 is top-to-bottom.
 */
export function calculateLinearGradientPoints(
  width: number,
  height: number,
  rotation: number
): { start: { x: number; y: number }; end: { x: number; y: number } } {
  // Convert rotation to radians
  const radians = (rotation * Math.PI) / 180

  // Calculate the diagonal length to ensure gradient covers the entire shape
  const diagonal = Math.sqrt(width * width + height * height)

  // Center of the shape
  const centerX = width / 2
  const centerY = height / 2

  // Calculate start and end points
  const dx = (Math.cos(radians) * diagonal) / 2
  const dy = (Math.sin(radians) * diagonal) / 2

  return {
    start: { x: centerX - dx, y: centerY - dy },
    end: { x: centerX + dx, y: centerY + dy },
  }
}

/**
 * Convert ShapeFill to Konva fill props for solid fills.
 */
export function getSolidFillProps(fill: SolidFill): KonvaSolidFillProps {
  return {
    fill: fill.color,
    fillPriority: "color",
  }
}

/**
 * Convert ShapeFill to Konva fill props for linear gradients.
 */
export function getLinearGradientFillProps(
  fill: LinearGradientFill,
  width: number,
  height: number
): KonvaLinearGradientProps {
  const { start, end } = calculateLinearGradientPoints(
    width,
    height,
    fill.rotation
  )

  return {
    fillLinearGradientStartPoint: start,
    fillLinearGradientEndPoint: end,
    fillLinearGradientColorStops: flattenColorStops(fill.colorStops),
    fillPriority: "linear-gradient",
  }
}

/**
 * Convert ShapeFill to Konva fill props for radial gradients.
 */
export function getRadialGradientFillProps(
  fill: RadialGradientFill,
  width: number,
  height: number
): KonvaRadialGradientProps {
  const centerX = width / 2
  const centerY = height / 2
  const radius = Math.max(width, height) / 2

  return {
    fillRadialGradientStartPoint: { x: centerX, y: centerY },
    fillRadialGradientStartRadius: 0,
    fillRadialGradientEndPoint: { x: centerX, y: centerY },
    fillRadialGradientEndRadius: radius,
    fillRadialGradientColorStops: flattenColorStops(fill.colorStops),
    fillPriority: "radial-gradient",
  }
}

/**
 * Get Konva fill props from a ShapeFill.
 * Note: For image fills, you need to load the image separately and use getImageFillProps.
 */
export function getKonvaFillProps(
  fill: ShapeFill,
  width: number,
  height: number
): Omit<KonvaFillProps, "fillPatternImage"> | null {
  switch (fill.type) {
    case "solid":
      return getSolidFillProps(fill)
    case "linear-gradient":
      return getLinearGradientFillProps(fill, width, height)
    case "radial-gradient":
      return getRadialGradientFillProps(fill, width, height)
    case "image":
      // Image fills need special handling with loaded image
      return null
    default: {
      return null
    }
  }
}

/**
 * Get Konva pattern fill props from an ImageFill with a loaded image.
 */
export function getImageFillProps(
  fill: ImageFill,
  image: HTMLImageElement
): KonvaPatternFillProps {
  return {
    fillPatternImage: image,
    fillPatternScale: fill.scale,
    fillPatternOffset: fill.offset,
    fillPriority: "pattern",
  }
}

// =============================================================================
// CSS Gradient Generation (for UI previews)
// =============================================================================

/**
 * Generate a CSS linear-gradient string from a LinearGradientFill.
 */
export function toCssLinearGradient(fill: LinearGradientFill): string {
  const stops = fill.colorStops
    .map((stop) => `${stop.color} ${stop.offset * 100}%`)
    .join(", ")
  return `linear-gradient(${fill.rotation}deg, ${stops})`
}

/**
 * Generate a CSS radial-gradient string from a RadialGradientFill.
 */
export function toCssRadialGradient(fill: RadialGradientFill): string {
  const stops = fill.colorStops
    .map((stop) => `${stop.color} ${stop.offset * 100}%`)
    .join(", ")
  return `radial-gradient(circle, ${stops})`
}

/**
 * Generate a CSS background string from any ShapeFill.
 * For image fills, returns an empty string (handled separately).
 */
export function toCssBackground(fill: ShapeFill): string {
  switch (fill.type) {
    case "solid":
      return fill.color
    case "linear-gradient":
      return toCssLinearGradient(fill)
    case "radial-gradient":
      return toCssRadialGradient(fill)
    case "image":
      return ""
    default: {
      return ""
    }
  }
}

// =============================================================================
// Default Fills
// =============================================================================

/**
 * Default solid fill (white)
 */
export const DEFAULT_SOLID_FILL: SolidFill = {
  type: "solid",
  color: "#ffffff",
}

/**
 * Default linear gradient fill
 */
export const DEFAULT_LINEAR_GRADIENT: LinearGradientFill = {
  type: "linear-gradient",
  colorStops: [
    { offset: 0, color: "#22c55e" },
    { offset: 1, color: "#3b82f6" },
  ],
  rotation: 90,
}

/**
 * Default radial gradient fill
 */
export const DEFAULT_RADIAL_GRADIENT: RadialGradientFill = {
  type: "radial-gradient",
  colorStops: [
    { offset: 0, color: "#ffffff" },
    { offset: 1, color: "#3b82f6" },
  ],
}

/**
 * Create a new solid fill
 */
export function createSolidFill(color: string): SolidFill {
  return { type: "solid", color }
}

/**
 * Create a new linear gradient fill
 */
export function createLinearGradient(
  colorStops: GradientColorStop[],
  rotation = 90
): LinearGradientFill {
  return { type: "linear-gradient", colorStops, rotation }
}

/**
 * Create a new radial gradient fill
 */
export function createRadialGradient(
  colorStops: GradientColorStop[]
): RadialGradientFill {
  return { type: "radial-gradient", colorStops }
}

/**
 * Create a new image fill
 */
export function createImageFill(
  assetId: string,
  options?: Pick<ImageFill, "scale" | "offset">
): ImageFill {
  return {
    type: "image",
    assetId: assetId as AssetId,
    scale: options?.scale,
    offset: options?.offset,
  }
}
