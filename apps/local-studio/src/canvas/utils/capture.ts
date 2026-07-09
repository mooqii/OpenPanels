import type { Transformer } from "../shapes/Transformer"

/**
 * Capture the current transformer selection as a PNG data URL.
 * Uses the transformer's computed bounds to align with the visual selection.
 */
export function captureTransformer(
  transformer: Transformer | null,
  options?: { basePixelRatio?: number }
): string | null {
  if (!transformer) return null

  const stage = transformer.getStage()
  const nodes = transformer.getNodes()
  if (!stage || nodes.length === 0) return null

  const minX = transformer.getX()
  const minY = transformer.getY()
  const width = transformer.getWidth()
  const height = transformer.getHeight()
  if (!Number.isFinite(width)) return null
  if (!Number.isFinite(height)) return null
  if (width <= 0 || height <= 0) return null

  const scaleX = stage.scaleX()
  const scaleY = stage.scaleY()
  if (!(Number.isFinite(scaleX) && Number.isFinite(scaleY))) return null
  if (scaleX <= 0 || scaleY <= 0) return null

  const defaultPixelRatio =
    typeof window === "undefined" ? 1 : window.devicePixelRatio || 1
  const basePixelRatio = options?.basePixelRatio ?? defaultPixelRatio
  const normalizedScale = scaleX === scaleY ? scaleX : Math.max(scaleX, scaleY)
  // Compensate for stage zoom so output size matches unzoomed canvas units.
  const pixelRatio = basePixelRatio / normalizedScale

  const wasVisible = transformer.visible()
  transformer.visible(false)
  stage.batchDraw()

  try {
    return stage.toDataURL({
      x: minX,
      y: minY,
      width,
      height,
      pixelRatio,
    })
  } finally {
    transformer.visible(wasVisible)
    stage.batchDraw()
  }
}
